use std::path::PathBuf;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use retrievalkit_core::{
    CorpusId, Document, HybridHit, HybridQuery, Metadata, Record, RecordChunkInput,
    RetrievalDatabase, RetrievalDatabaseBuilder as RustRetrievalDatabaseBuilder, SearchHit,
    SearchQuery,
};
use serde::Deserialize;

use crate::{
    file_size_report_to_py, hybrid_trace_to_py, parse_encoding, parse_metadata, parse_metric,
    parse_optional_filter, py_error, search_hit_to_py, RetrievalKitError,
};

#[derive(Debug, Deserialize)]
struct RetrievalRecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<RetrievalChunkBatch>,
}

#[derive(Debug, Deserialize)]
struct RetrievalChunkBatch {
    key: retrievalkit_core::ChunkKey,
    text: String,
    embedding: Vec<f32>,
    #[serde(default)]
    metadata: Metadata,
}

#[pyclass(name = "_RetrievalDatabaseBuilder")]
pub(crate) struct PyRetrievalDatabaseBuilder {
    builder: Option<RustRetrievalDatabaseBuilder>,
}

#[pymethods]
impl PyRetrievalDatabaseBuilder {
    #[new]
    #[pyo3(signature = (
        corpus_id,
        metric = "cosine",
        encoding = "i8"
    ))]
    fn new(corpus_id: String, metric: &str, encoding: &str) -> PyResult<Self> {
        Ok(Self {
            builder: Some(RustRetrievalDatabaseBuilder::new(
                CorpusId::new(corpus_id).map_err(py_error)?,
                parse_metric(metric)?,
                parse_encoding(encoding)?,
            )),
        })
    }

    fn upsert_document(
        &mut self,
        py: Python<'_>,
        document_id: String,
        text: String,
        metadata: &Bound<'_, PyAny>,
        embedding: Vec<f32>,
    ) -> PyResult<Vec<u64>> {
        let document = Document {
            id: document_id,
            text,
            metadata: parse_metadata(metadata)?,
        };
        let builder = self.require_builder()?;
        py.detach(move || {
            builder
                .upsert_document(document, embedding)
                .map_err(py_error)
        })
    }

    fn add(&mut self, py: Python<'_>, records_json: String) -> PyResult<Vec<Vec<u64>>> {
        let records: Vec<RetrievalRecordBatch> =
            serde_json::from_str(&records_json).map_err(|error| {
                PyValueError::new_err(format!("invalid retrieval records: {error}"))
            })?;
        let builder = self.require_builder()?;
        py.detach(move || {
            records
                .into_iter()
                .map(|batch| {
                    builder
                        .upsert_record(
                            batch.record,
                            batch.projected_metadata,
                            batch
                                .chunks
                                .into_iter()
                                .map(|chunk| RecordChunkInput {
                                    key: chunk.key,
                                    text: chunk.text,
                                    embedding: chunk.embedding,
                                    metadata: chunk.metadata,
                                })
                                .collect(),
                        )
                        .map_err(py_error)
                })
                .collect()
        })
    }

    fn build(&mut self, py: Python<'_>) -> PyResult<PyRetrievalDatabase> {
        let builder = self.builder.take().ok_or_else(|| {
            PyRuntimeError::new_err("retrieval builder has already been consumed")
        })?;
        py.detach(move || {
            Ok(PyRetrievalDatabase {
                database: Some(builder.build().map_err(py_error)?),
            })
        })
    }

    #[getter]
    fn dimension(&self) -> Option<usize> {
        self.builder
            .as_ref()
            .and_then(|builder| builder.dimension())
    }
}

impl PyRetrievalDatabaseBuilder {
    fn require_builder(&mut self) -> PyResult<&mut RustRetrievalDatabaseBuilder> {
        self.builder
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("retrieval builder has already been consumed"))
    }
}

#[pyclass(name = "_RetrievalDatabase")]
pub(crate) struct PyRetrievalDatabase {
    database: Option<RetrievalDatabase>,
}

#[pymethods]
impl PyRetrievalDatabase {
    #[staticmethod]
    fn load(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        py.detach(move || {
            Ok(Self {
                database: Some(RetrievalDatabase::load_from_dir(path).map_err(py_error)?),
            })
        })
    }

    #[staticmethod]
    fn validate(py: Python<'_>, path: PathBuf) -> PyResult<()> {
        py.detach(move || RetrievalDatabase::validate_dir(path).map_err(py_error))
    }

    fn save(&self, py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
        let database = self.require_database()?;
        let report = py.detach(move || database.save_to_dir(path).map_err(py_error))?;
        file_size_report_to_py(py, report)
    }

    #[pyo3(signature = (embedding, *, limit = 10, r#where = None))]
    fn semantic_search(
        &self,
        py: Python<'_>,
        embedding: Vec<f32>,
        limit: usize,
        r#where: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let query = SearchQuery {
            embedding,
            top_k: limit,
            filter: parse_optional_filter(r#where)?,
        };
        let database = self.require_database()?;
        let hits = py.detach(move || database.semantic_search(&query).map_err(py_error))?;
        search_hits_to_py(py, database, &hits)
    }

    #[pyo3(signature = (
        text,
        embedding,
        *,
        limit = 10,
        r#where = None,
        vector_candidates = None,
        keyword_candidates = None,
        alpha = 0.6
    ))]
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
        alpha: f32,
    ) -> PyResult<Py<PyAny>> {
        let mut query = HybridQuery::new(text, embedding, limit);
        let vector_top_k = vector_candidates.unwrap_or(query.vector_top_k);
        let keyword_top_k = keyword_candidates.unwrap_or(query.keyword_top_k);
        query = query.with_candidate_limits(vector_top_k, keyword_top_k);
        if let Some(filter) = parse_optional_filter(r#where)? {
            query = query.with_filter(filter);
        }
        query = query.try_with_alpha(alpha).map_err(py_error)?;
        let database = self.require_database()?;
        let hits = py.detach(move || database.hybrid_search(&query).map_err(py_error))?;
        hybrid_hits_to_py(py, database, &hits, alpha)
    }

    fn close(&mut self) {
        self.database = None;
    }

    #[getter]
    fn closed(&self) -> bool {
        self.database.is_none()
    }
}

impl PyRetrievalDatabase {
    fn require_database(&self) -> PyResult<&RetrievalDatabase> {
        self.database
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("retrieval database has been closed"))
    }
}

fn search_hits_to_py(
    py: Python<'_>,
    database: &RetrievalDatabase,
    hits: &[SearchHit],
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = database
            .chunk(hit.chunk_id)
            .ok_or_else(|| RetrievalKitError::new_err("search hit referenced a missing chunk"))?;
        result.append(search_hit_to_py(py, hit, chunk)?)?;
    }
    Ok(result.into_any().unbind())
}

fn hybrid_hits_to_py(
    py: Python<'_>,
    database: &RetrievalDatabase,
    hits: &[HybridHit],
    alpha: f32,
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = database
            .chunk(hit.chunk_id)
            .ok_or_else(|| RetrievalKitError::new_err("hybrid hit referenced a missing chunk"))?;
        let item = PyDict::new(py);
        item.set_item("chunk_id", hit.chunk_id)?;
        item.set_item("document_id", &hit.document_id)?;
        item.set_item("text", &chunk.text)?;
        item.set_item("metadata", crate::metadata_to_py(py, &chunk.metadata)?)?;
        item.set_item("score", hit.score)?;
        item.set_item("vector_score", hit.vector_score)?;
        item.set_item("keyword_score", hit.keyword_score)?;
        item.set_item("matched_terms", &hit.trace.matched_terms)?;
        item.set_item("trace", hybrid_trace_to_py(py, hit, alpha)?)?;
        result.append(item)?;
    }
    Ok(result.into_any().unbind())
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyRetrievalDatabaseBuilder>()?;
    module.add_class::<PyRetrievalDatabase>()?;
    Ok(())
}
