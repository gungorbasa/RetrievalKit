use std::collections::BTreeMap;
use std::path::PathBuf;

use pyo3::exceptions::{PyException, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use vectorkit_core::{
    ChunkInput, CompactionReport, Document, ExactVectorIndex, Filter, HybridFusionTrace, HybridHit,
    HybridQuery, IndexConfig, IndexFileSizeReport, KeywordHit, KeywordQuery, Metadata,
    MetadataValue, SearchHit, SearchQuery, StoredChunk, VectorEncoding,
    VectorKitError as CoreError, VectorMetric,
};
use vectorkit_ingest::{chunk_text as split_text, ChunkingConfig, ChunkingStrategy};

pyo3::create_exception!(_native, VectorKitError, PyException);
pyo3::create_exception!(_native, DimensionMismatchError, VectorKitError);
pyo3::create_exception!(_native, PersistenceError, VectorKitError);
pyo3::create_exception!(_native, FilterError, VectorKitError);
pyo3::create_exception!(_native, UnsupportedFormatError, VectorKitError);
pyo3::create_exception!(_native, CorruptIndexError, VectorKitError);

#[pyfunction]
#[pyo3(signature = (text, *, max_characters, overlap_characters = 0, strategy = "sentence"))]
fn chunk_text(
    py: Python<'_>,
    text: &str,
    max_characters: usize,
    overlap_characters: usize,
    strategy: &str,
) -> PyResult<Py<PyAny>> {
    let strategy = match strategy.to_ascii_lowercase().as_str() {
        "fixed" => ChunkingStrategy::Fixed,
        "sentence" => ChunkingStrategy::Sentence,
        _ => {
            return Err(PyValueError::new_err(format!(
                "unsupported chunking strategy '{strategy}'; expected 'fixed' or 'sentence'"
            )))
        }
    };
    let config = ChunkingConfig::new(max_characters, overlap_characters, strategy)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let output = PyList::empty(py);
    for chunk in split_text(text, config) {
        let item = PyDict::new(py);
        item.set_item("text", chunk.text)?;
        item.set_item("start_byte", chunk.start_byte)?;
        item.set_item("end_byte", chunk.end_byte)?;
        output.append(item)?;
    }
    Ok(output.into_any().unbind())
}

#[pyclass(name = "Index")]
struct PyIndex {
    inner: ExactVectorIndex,
}

#[pymethods]
impl PyIndex {
    #[new]
    #[pyo3(signature = (dimension, metric = "cosine", encoding = "i8"))]
    fn new(dimension: usize, metric: &str, encoding: &str) -> PyResult<Self> {
        let config = IndexConfig::new(dimension, parse_metric(metric)?)
            .with_vector_encoding(parse_encoding(encoding)?);
        Ok(Self {
            inner: ExactVectorIndex::try_with_config(config).map_err(py_error)?,
        })
    }

    #[staticmethod]
    fn load(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        py.detach(move || {
            Ok(Self {
                inner: ExactVectorIndex::load_from_dir(path).map_err(py_error)?,
            })
        })
    }

    /// Verifies a saved index without changing it.
    #[staticmethod]
    fn validate(py: Python<'_>, path: PathBuf) -> PyResult<()> {
        py.detach(move || ExactVectorIndex::validate_dir(path).map_err(py_error))
    }

    #[getter]
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    #[getter]
    fn active_chunk_count(&self) -> usize {
        self.inner.active_chunk_count()
    }

    #[getter]
    fn total_chunk_count(&self) -> usize {
        self.inner.len()
    }

    #[getter]
    fn tombstoned_chunk_count(&self) -> usize {
        self.inner.tombstoned_chunk_count()
    }

    fn add(&mut self, py: Python<'_>, documents: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let documents = documents.cast::<PyList>().map_err(|_| {
            PyTypeError::new_err("documents must be a list of document dictionaries")
        })?;

        let mut parsed_documents = Vec::with_capacity(documents.len());
        for document in documents {
            let document = document
                .cast::<PyDict>()
                .map_err(|_| PyTypeError::new_err("each document must be a dictionary"))?;
            let document_id = required_string(document, "id")?;
            let document_metadata = optional_metadata(document, "metadata")?;
            let chunks = required_list(document, "chunks")?;
            let mut chunk_inputs = Vec::with_capacity(chunks.len());

            for chunk in chunks {
                let chunk = chunk
                    .cast::<PyDict>()
                    .map_err(|_| PyTypeError::new_err("each chunk must be a dictionary"))?;
                let text = required_string(chunk, "text")?;
                let embedding = required_embedding(chunk, "embedding")?;
                let metadata = optional_metadata(chunk, "metadata")?;
                chunk_inputs.push(ChunkInput {
                    text,
                    embedding,
                    metadata,
                });
            }

            parsed_documents.push((
                document_id.clone(),
                Document {
                    id: document_id,
                    text: String::new(),
                    metadata: document_metadata,
                },
                chunk_inputs,
            ));
        }

        let added = py.detach(move || {
            parsed_documents
                .into_iter()
                .map(|(document_id, document, chunk_inputs)| {
                    let chunk_ids = self
                        .inner
                        .upsert_document(document, chunk_inputs)
                        .map_err(py_error)?;
                    Ok((document_id, chunk_ids))
                })
                .collect::<PyResult<Vec<_>>>()
        })?;

        let result = PyList::empty(py);
        for (document_id, chunk_ids) in added {
            let item = PyDict::new(py);
            item.set_item("id", document_id)?;
            item.set_item("chunk_ids", chunk_ids)?;
            result.append(item)?;
        }

        Ok(result.into_any().unbind())
    }

    fn delete_document(&mut self, py: Python<'_>, document_id: String) -> usize {
        py.detach(move || self.inner.delete_document(&document_id))
    }

    fn compact(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let report = py.detach(|| self.inner.compact().map_err(py_error))?;
        compaction_report_to_py(py, report)
    }

    #[pyo3(signature = (embedding, *, limit = 10, r#where = None))]
    fn search(
        &self,
        py: Python<'_>,
        embedding: Vec<f32>,
        limit: usize,
        r#where: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let query = SearchQuery {
            embedding,
            top_k: limit,
            filter,
        };
        let hits = py.detach(move || self.inner.search(&query).map_err(py_error))?;
        search_hits_to_py(py, &self.inner, &hits)
    }

    #[pyo3(signature = (text, *, limit = 10, r#where = None))]
    fn keyword_search(
        &self,
        py: Python<'_>,
        text: String,
        limit: usize,
        r#where: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let query = KeywordQuery {
            text,
            top_k: limit,
            filter,
        };
        let hits = py.detach(move || self.inner.keyword_search(&query).map_err(py_error))?;
        keyword_hits_to_py(py, &self.inner, &hits)
    }

    #[pyo3(signature = (
        text,
        embedding,
        *,
        limit = 10,
        r#where = None,
        vector_candidates = None,
        keyword_candidates = None,
        fusion = "rrf",
        vector_weight = 0.6,
        keyword_weight = 0.4,
        rrf_k = 60.0
    ))]
    // PyO3 maps the Pythonic keyword-only API directly to Rust parameters here.
    #[allow(clippy::too_many_arguments)]
    fn hybrid_search(
        &self,
        py: Python<'_>,
        text: String,
        embedding: Vec<f32>,
        limit: usize,
        r#where: Option<&Bound<'_, PyAny>>,
        vector_candidates: Option<usize>,
        keyword_candidates: Option<usize>,
        fusion: &str,
        vector_weight: f32,
        keyword_weight: f32,
        rrf_k: f32,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let mut query = HybridQuery::new(text, embedding, limit);
        let vector_top_k = vector_candidates.unwrap_or(query.vector_top_k);
        let keyword_top_k = keyword_candidates.unwrap_or(query.keyword_top_k);
        query = query.with_candidate_limits(vector_top_k, keyword_top_k);
        if let Some(filter) = filter {
            query = query.with_filter(filter);
        }
        query = match fusion.to_ascii_lowercase().as_str() {
            "weighted" | "weighted_normalized" | "weighted-normalized" => {
                query.with_weighted_normalized_score(vector_weight, keyword_weight)
            }
            "rrf" | "reciprocal_rank" | "reciprocal-rank" => query.with_rrf_k(rrf_k),
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unsupported fusion '{fusion}'; expected 'weighted' or 'rrf'"
                )));
            }
        };

        let hits = py.detach(move || self.inner.hybrid_search(&query).map_err(py_error))?;
        hybrid_hits_to_py(py, &self.inner, &hits)
    }

    #[pyo3(signature = (path, *, include_bm25 = true))]
    fn save(&mut self, py: Python<'_>, path: PathBuf, include_bm25: bool) -> PyResult<Py<PyAny>> {
        let options = if include_bm25 {
            vectorkit_core::IndexPersistenceOptions::hybrid()
        } else {
            vectorkit_core::IndexPersistenceOptions::vector_only()
        };
        let report = py.detach(move || {
            self.inner
                .save_to_dir_with_options(path, options)
                .map_err(py_error)
        })?;
        file_size_report_to_py(py, report)
    }
}

#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyIndex>()?;
    m.add_function(wrap_pyfunction!(chunk_text, m)?)?;
    m.add("VectorKitError", py.get_type::<VectorKitError>())?;
    m.add(
        "DimensionMismatchError",
        py.get_type::<DimensionMismatchError>(),
    )?;
    m.add("PersistenceError", py.get_type::<PersistenceError>())?;
    m.add("FilterError", py.get_type::<FilterError>())?;
    m.add(
        "UnsupportedFormatError",
        py.get_type::<UnsupportedFormatError>(),
    )?;
    m.add("CorruptIndexError", py.get_type::<CorruptIndexError>())?;
    Ok(())
}

fn parse_metric(metric: &str) -> PyResult<VectorMetric> {
    match metric.to_ascii_lowercase().as_str() {
        "cosine" => Ok(VectorMetric::Cosine),
        "dot_product" | "dotproduct" | "dot-product" => Ok(VectorMetric::DotProduct),
        _ => Err(PyValueError::new_err(format!(
            "unsupported metric '{metric}'; expected 'cosine' or 'dot_product'"
        ))),
    }
}

fn parse_encoding(encoding: &str) -> PyResult<VectorEncoding> {
    match encoding.to_ascii_lowercase().as_str() {
        "f32" => Ok(VectorEncoding::F32),
        "f16" => Ok(VectorEncoding::F16),
        "bf16" => Ok(VectorEncoding::BF16),
        "i8" | "i8_scalar_quantized" | "i8scalarquantized" => Ok(VectorEncoding::I8ScalarQuantized),
        "binary" | "binary_quantized" | "binaryquantized" => Ok(VectorEncoding::BinaryQuantized),
        _ => Err(PyValueError::new_err(format!(
            "unsupported encoding '{encoding}'; expected 'f32', 'f16', 'bf16', 'i8', or 'binary'"
        ))),
    }
}

fn py_error(error: CoreError) -> PyErr {
    match error {
        CoreError::InvalidDimension { .. } => DimensionMismatchError::new_err(error.to_string()),
        CoreError::InvalidRange { .. } => FilterError::new_err(error.to_string()),
        CoreError::Persistence { .. } => PersistenceError::new_err(error.to_string()),
        CoreError::InvalidFormat { .. } => UnsupportedFormatError::new_err(error.to_string()),
        CoreError::CorruptIndex { .. } => CorruptIndexError::new_err(error.to_string()),
        CoreError::UnsupportedVectorEncoding { .. } => {
            UnsupportedFormatError::new_err(error.to_string())
        }
    }
}

fn required_item<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    dict.get_item(key)?
        .ok_or_else(|| PyKeyError::new_err(format!("missing required key '{key}'")))
}

fn optional_item<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    dict.get_item(key)
}

fn required_string(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    required_item(dict, key)?.extract()
}

fn required_list<'py>(dict: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyList>> {
    required_item(dict, key)?
        .cast::<PyList>()
        .cloned()
        .map_err(|_| PyTypeError::new_err(format!("'{key}' must be a list")))
}

fn required_embedding(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Vec<f32>> {
    required_item(dict, key)?
        .extract::<Vec<f32>>()
        .map_err(|_| PyTypeError::new_err(format!("'{key}' must be a sequence of floats")))
}

fn optional_metadata(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Metadata> {
    let Some(value) = optional_item(dict, key)? else {
        return Ok(Metadata::new());
    };
    parse_metadata(&value)
}

fn parse_metadata(value: &Bound<'_, PyAny>) -> PyResult<Metadata> {
    let dict = value
        .cast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("metadata must be a dictionary"))?;
    let mut metadata = BTreeMap::new();
    for (key, value) in dict {
        let key: String = key
            .extract()
            .map_err(|_| PyTypeError::new_err("metadata keys must be strings"))?;
        metadata.insert(key, parse_metadata_value(&value)?);
    }
    Ok(metadata)
}

fn parse_metadata_value(value: &Bound<'_, PyAny>) -> PyResult<MetadataValue> {
    if value.is_instance_of::<PyBool>() {
        Ok(MetadataValue::Boolean(value.extract()?))
    } else if value.is_instance_of::<PyString>() {
        Ok(MetadataValue::String(value.extract()?))
    } else if value.is_instance_of::<PyInt>() {
        Ok(MetadataValue::Integer(value.extract()?))
    } else if value.is_instance_of::<PyFloat>() {
        Ok(MetadataValue::Float(value.extract()?))
    } else {
        Err(PyTypeError::new_err(
            "metadata values must be str, int, float, or bool",
        ))
    }
}

fn parse_optional_filter(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Filter>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }
    Ok(Some(parse_filter(value)?))
}

fn parse_filter(value: &Bound<'_, PyAny>) -> PyResult<Filter> {
    let dict = value
        .cast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("where must be a dictionary"))?;
    parse_filter_dict(dict)
}

fn parse_filter_dict(dict: &Bound<'_, PyDict>) -> PyResult<Filter> {
    let mut filters = Vec::new();
    for (key, value) in dict {
        let key: String = key
            .extract()
            .map_err(|_| PyTypeError::new_err("filter keys must be strings"))?;
        match key.as_str() {
            "$and" => filters.push(Filter::All(parse_filter_list(&value, "$and")?)),
            "$or" => filters.push(Filter::Any(parse_filter_list(&value, "$or")?)),
            _ => filters.push(parse_field_filter(key, &value)?),
        }
    }
    match filters.len() {
        0 => Err(FilterError::new_err("where filter cannot be empty")),
        1 => Ok(filters.remove(0)),
        _ => Ok(Filter::All(filters)),
    }
}

fn parse_filter_list(value: &Bound<'_, PyAny>, operator: &str) -> PyResult<Vec<Filter>> {
    let list = value
        .cast::<PyList>()
        .map_err(|_| FilterError::new_err(format!("{operator} must be a list")))?;
    let mut filters = Vec::with_capacity(list.len());
    for item in list {
        filters.push(parse_filter(&item)?);
    }
    Ok(filters)
}

fn parse_field_filter(field: String, value: &Bound<'_, PyAny>) -> PyResult<Filter> {
    if let Ok(spec) = value.cast::<PyDict>() {
        let mut filters = Vec::new();
        let mut range_lower = None;
        let mut range_upper = None;

        for (operator, operand) in spec {
            let operator: String = operator
                .extract()
                .map_err(|_| PyTypeError::new_err("filter operators must be strings"))?;
            match operator.as_str() {
                "$eq" => filters.push(Filter::Equals {
                    field: field.clone(),
                    value: parse_metadata_value(&operand)?,
                }),
                "$ne" => filters.push(Filter::NotEquals {
                    field: field.clone(),
                    value: parse_metadata_value(&operand)?,
                }),
                "$in" => filters.push(Filter::In {
                    field: field.clone(),
                    values: parse_metadata_value_list(&operand, "$in")?,
                }),
                "$gte" => range_lower = Some(parse_metadata_value(&operand)?),
                "$lte" => range_upper = Some(parse_metadata_value(&operand)?),
                "$exists" => {
                    let exists: bool = operand
                        .extract()
                        .map_err(|_| FilterError::new_err("$exists value must be a boolean"))?;
                    if exists {
                        filters.push(Filter::Exists {
                            field: field.clone(),
                        });
                    } else {
                        return Err(FilterError::new_err(
                            "$exists: false is not supported by VectorKit V1 filters",
                        ));
                    }
                }
                _ => {
                    return Err(FilterError::new_err(format!(
                        "unsupported filter operator '{operator}'"
                    )));
                }
            }
        }

        if range_lower.is_some() || range_upper.is_some() {
            filters.push(Filter::Range {
                field,
                lower: range_lower,
                upper: range_upper,
            });
        }

        match filters.len() {
            0 => Err(FilterError::new_err("field filter cannot be empty")),
            1 => Ok(filters.remove(0)),
            _ => Ok(Filter::All(filters)),
        }
    } else {
        Ok(Filter::Equals {
            field,
            value: parse_metadata_value(value)?,
        })
    }
}

fn parse_metadata_value_list(
    value: &Bound<'_, PyAny>,
    operator: &str,
) -> PyResult<Vec<MetadataValue>> {
    let list = value
        .cast::<PyList>()
        .map_err(|_| FilterError::new_err(format!("{operator} value must be a list")))?;
    let mut values = Vec::with_capacity(list.len());
    for item in list {
        values.push(parse_metadata_value(&item)?);
    }
    Ok(values)
}

fn search_hits_to_py(
    py: Python<'_>,
    index: &ExactVectorIndex,
    hits: &[SearchHit],
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = index
            .chunk(hit.chunk_id)
            .ok_or_else(|| VectorKitError::new_err("search hit referenced a missing chunk"))?;
        result.append(search_hit_to_py(py, hit, chunk)?)?;
    }
    Ok(result.into_any().unbind())
}

fn search_hit_to_py(py: Python<'_>, hit: &SearchHit, chunk: &StoredChunk) -> PyResult<Py<PyAny>> {
    let item = PyDict::new(py);
    item.set_item("chunk_id", hit.chunk_id)?;
    item.set_item("document_id", &hit.document_id)?;
    item.set_item("text", &chunk.text)?;
    item.set_item("metadata", metadata_to_py(py, &chunk.metadata)?)?;
    item.set_item("score", hit.score)?;

    let trace = PyDict::new(py);
    trace.set_item("vector_score", hit.trace.vector_score)?;
    trace.set_item("keyword_score", hit.trace.keyword_score)?;
    trace.set_item("filter_matched", hit.trace.filter_matched)?;
    item.set_item("trace", trace)?;

    Ok(item.into_any().unbind())
}

fn keyword_hits_to_py(
    py: Python<'_>,
    index: &ExactVectorIndex,
    hits: &[KeywordHit],
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = index
            .chunk(hit.chunk_id)
            .ok_or_else(|| VectorKitError::new_err("keyword hit referenced a missing chunk"))?;
        let item = PyDict::new(py);
        item.set_item("chunk_id", hit.chunk_id)?;
        item.set_item("document_id", &hit.document_id)?;
        item.set_item("text", &chunk.text)?;
        item.set_item("metadata", metadata_to_py(py, &chunk.metadata)?)?;
        item.set_item("score", hit.score)?;
        item.set_item("matched_terms", &hit.matched_terms)?;
        result.append(item)?;
    }
    Ok(result.into_any().unbind())
}

fn hybrid_hits_to_py(
    py: Python<'_>,
    index: &ExactVectorIndex,
    hits: &[HybridHit],
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = index
            .chunk(hit.chunk_id)
            .ok_or_else(|| VectorKitError::new_err("hybrid hit referenced a missing chunk"))?;
        let item = PyDict::new(py);
        item.set_item("chunk_id", hit.chunk_id)?;
        item.set_item("document_id", &hit.document_id)?;
        item.set_item("text", &chunk.text)?;
        item.set_item("metadata", metadata_to_py(py, &chunk.metadata)?)?;
        item.set_item("score", hit.score)?;
        item.set_item("vector_score", hit.vector_score)?;
        item.set_item("keyword_score", hit.keyword_score)?;
        item.set_item("matched_terms", &hit.trace.matched_terms)?;
        item.set_item("trace", hybrid_trace_to_py(py, hit)?)?;
        result.append(item)?;
    }
    Ok(result.into_any().unbind())
}

fn hybrid_trace_to_py(py: Python<'_>, hit: &HybridHit) -> PyResult<Py<PyAny>> {
    let trace = PyDict::new(py);
    trace.set_item("vector_rank", hit.trace.vector_rank)?;
    trace.set_item("keyword_rank", hit.trace.keyword_rank)?;
    trace.set_item("normalized_vector_score", hit.trace.normalized_vector_score)?;
    trace.set_item(
        "normalized_keyword_score",
        hit.trace.normalized_keyword_score,
    )?;
    trace.set_item("matched_terms", &hit.trace.matched_terms)?;
    trace.set_item("filter_matched", hit.trace.filter_matched)?;
    trace.set_item("fusion", fusion_trace_to_py(py, hit.trace.fusion)?)?;
    Ok(trace.into_any().unbind())
}

fn fusion_trace_to_py(py: Python<'_>, fusion: HybridFusionTrace) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    match fusion {
        HybridFusionTrace::ReciprocalRank { rrf_k } => {
            dict.set_item("kind", "rrf")?;
            dict.set_item("rrf_k", rrf_k)?;
        }
        HybridFusionTrace::WeightedNormalizedScore {
            vector_weight,
            keyword_weight,
        } => {
            dict.set_item("kind", "weighted_normalized")?;
            dict.set_item("vector_weight", vector_weight)?;
            dict.set_item("keyword_weight", keyword_weight)?;
        }
    }
    Ok(dict.into_any().unbind())
}

fn metadata_to_py(py: Python<'_>, metadata: &Metadata) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (key, value) in metadata {
        match value {
            MetadataValue::String(value) => dict.set_item(key, value)?,
            MetadataValue::Integer(value) | MetadataValue::TimestampMillis(value) => {
                dict.set_item(key, value)?
            }
            MetadataValue::Float(value) => dict.set_item(key, value)?,
            MetadataValue::Boolean(value) => dict.set_item(key, value)?,
        }
    }
    Ok(dict.into_any().unbind())
}

fn file_size_report_to_py(py: Python<'_>, report: IndexFileSizeReport) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("manifest_bytes", report.manifest_bytes)?;
    dict.set_item("vectors_bytes", report.vectors_bytes)?;
    dict.set_item("chunks_bytes", report.chunks_bytes)?;
    dict.set_item("bm25_bytes", report.bm25_bytes)?;
    dict.set_item("tombstones_bytes", report.tombstones_bytes)?;
    dict.set_item("total_bytes", report.total_bytes())?;
    Ok(dict.into_any().unbind())
}

fn compaction_report_to_py(py: Python<'_>, report: CompactionReport) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("chunks_before", report.chunks_before)?;
    dict.set_item("chunks_after", report.chunks_after)?;
    dict.set_item("chunks_removed", report.chunks_removed)?;
    dict.set_item("estimated_bytes_before", report.estimated_bytes_before)?;
    dict.set_item("estimated_bytes_after", report.estimated_bytes_after)?;
    dict.set_item(
        "estimated_bytes_reclaimed",
        report.estimated_bytes_reclaimed,
    )?;
    Ok(dict.into_any().unbind())
}
