use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyInt, PyList, PyString};
use retrievalkit_core::{
    ChunkIdentity, ChunkKey, CorpusChunkInput, CorpusId, CorpusIndex, HybridHit, HybridQuery,
    Metadata, Record, RecordChunkInput, RecordId, RecordInput, RetrievalConfiguration,
    RetrievalDatabase, RetrievalKitError as CoreError, SearchHit, SearchQuery,
};
use retrievalkit_graph::{
    CancellationToken, Direction, GraphCandidateProjection, GraphDatabase, GraphDatabaseFileSizes,
    GraphError as RustGraphError, GraphQuery, GraphResult, GraphRetrievalDatabase, GraphScalar,
    GraphSchema, NodeId, NodeSource, NodeType, QueryLimits, RelationshipType, Seed, Traverse,
    TruncationReason,
};
use serde::Deserialize;

use crate::{
    hybrid_trace_to_py, metadata_to_py, parse_encoding, parse_metric, parse_optional_filter,
    py_error, search_hit_to_py, DimensionMismatchError, GraphCancelledError, GraphError,
    GraphQueryError, GraphTimeoutError, InvalidGraphSchemaError,
    RetrievalCapabilityUnavailableError, RetrievalKitError, StaleGraphSelectionError,
};

#[derive(Debug, Deserialize)]
struct RecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<RecordChunkBatch>,
}

#[derive(Debug, Deserialize)]
struct RecordChunkBatch {
    key: ChunkKey,
    text: String,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
    #[serde(default)]
    metadata: Metadata,
}

type QueryNodeInput = (String, String, Option<String>);
type QueryTraversalInput = (String, String, i64, i64);
type QueryLimitsInput = (i64, i64, i64, i64);

#[pyclass(name = "_GraphCancellationToken")]
pub(crate) struct PyGraphCancellationToken {
    token: CancellationToken,
}

#[pymethods]
impl PyGraphCancellationToken {
    #[new]
    fn new() -> Self {
        Self {
            token: CancellationToken::default(),
        }
    }

    fn cancel(&self) {
        self.token.cancel();
    }

    #[getter]
    fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

#[pyclass(name = "_GraphSelection")]
pub(crate) struct PyGraphSelection {
    result: Option<GraphResult>,
    projected_chunk_count: usize,
}

#[pymethods]
impl PyGraphSelection {
    #[getter]
    fn projected_chunk_count(&self) -> usize {
        self.projected_chunk_count
    }

    fn materialize(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = self
            .result
            .as_ref()
            .ok_or_else(|| StaleGraphSelectionError::new_err("graph selection has been closed"))?;
        graph_result_to_py(py, result, self.projected_chunk_count)
    }

    fn close(&mut self) {
        self.result = None;
    }

    #[getter]
    fn closed(&self) -> bool {
        self.result.is_none()
    }
}

#[pyclass(name = "_GraphDatabaseBuilder")]
pub(crate) struct PyGraphDatabaseBuilder {
    corpus: Option<CorpusIndex>,
    schema: GraphSchema,
}

#[pymethods]
impl PyGraphDatabaseBuilder {
    #[new]
    fn new(corpus_id: String, schema_json: String) -> PyResult<Self> {
        Ok(Self {
            corpus: Some(CorpusIndex::new(
                CorpusId::new(corpus_id).map_err(core_error)?,
            )),
            schema: parse_schema(&schema_json)?,
        })
    }

    fn add(&mut self, records_json: String) -> PyResult<Vec<Vec<u64>>> {
        let records = parse_records(&records_json)?;
        let corpus = self
            .corpus
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("graph builder has already been consumed"))?;
        records
            .into_iter()
            .map(|batch| {
                if let Some(chunk) = batch.chunks.iter().find(|chunk| chunk.embedding.is_some()) {
                    return Err(PyValueError::new_err(format!(
                        "graph-only record '{}' chunk '{}' must not include an embedding",
                        batch.record.id.as_str(),
                        chunk.key.as_str()
                    )));
                }
                corpus
                    .upsert(RecordInput {
                        record: batch.record,
                        metadata: batch.projected_metadata,
                        chunks: batch
                            .chunks
                            .into_iter()
                            .map(|chunk| CorpusChunkInput {
                                key: chunk.key,
                                text: chunk.text,
                                metadata: chunk.metadata,
                            })
                            .collect(),
                    })
                    .map_err(core_error)
            })
            .collect()
    }

    fn build(&mut self, py: Python<'_>) -> PyResult<PyGraphDatabase> {
        let corpus = self
            .corpus
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("graph builder has already been consumed"))?;
        let schema = self.schema.clone();
        py.detach(move || {
            Ok(PyGraphDatabase {
                database: Some(GraphDatabase::build(corpus, schema).map_err(graph_error)?),
            })
        })
    }
}

#[pyclass(name = "_GraphRetrievalDatabaseBuilder")]
pub(crate) struct PyGraphRetrievalDatabaseBuilder {
    retrieval: Option<RetrievalDatabase>,
    schema: GraphSchema,
}

#[pymethods]
impl PyGraphRetrievalDatabaseBuilder {
    #[new]
    #[pyo3(signature = (
        dimension,
        corpus_id,
        schema_json,
        metric = "cosine",
        encoding = "i8"
    ))]
    fn new(
        dimension: usize,
        corpus_id: String,
        schema_json: String,
        metric: &str,
        encoding: &str,
    ) -> PyResult<Self> {
        let vector = retrievalkit_core::IndexConfig::new(dimension, parse_metric(metric)?)
            .with_vector_encoding(parse_encoding(encoding)?);
        let configuration = RetrievalConfiguration::semantic(vector);
        Ok(Self {
            retrieval: Some(
                RetrievalDatabase::new(
                    configuration,
                    CorpusId::new(corpus_id).map_err(core_error)?,
                )
                .map_err(core_error)?,
            ),
            schema: parse_schema(&schema_json)?,
        })
    }

    fn add(&mut self, records_json: String) -> PyResult<Vec<Vec<u64>>> {
        let records = parse_records(&records_json)?;
        let retrieval = self.retrieval.as_mut().ok_or_else(|| {
            PyRuntimeError::new_err("graph retrieval builder has already been consumed")
        })?;
        records
            .into_iter()
            .map(|batch| {
                let record_id = batch.record.id.as_str().to_owned();
                let chunks = batch
                    .chunks
                    .into_iter()
                    .map(|chunk| {
                        let embedding = chunk.embedding.ok_or_else(|| {
                            PyValueError::new_err(format!(
                                "record '{record_id}' chunk '{}' is missing an embedding",
                                chunk.key.as_str()
                            ))
                        })?;
                        Ok(RecordChunkInput {
                            key: chunk.key,
                            text: chunk.text,
                            embedding,
                            metadata: chunk.metadata,
                        })
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                retrieval
                    .upsert_record(batch.record, batch.projected_metadata, chunks)
                    .map_err(core_error)
            })
            .collect()
    }

    fn build(&mut self, py: Python<'_>) -> PyResult<PyGraphRetrievalDatabase> {
        let retrieval = self.retrieval.take().ok_or_else(|| {
            PyRuntimeError::new_err("graph retrieval builder has already been consumed")
        })?;
        let schema = self.schema.clone();
        py.detach(move || {
            Ok(PyGraphRetrievalDatabase {
                database: Some(
                    GraphRetrievalDatabase::build(retrieval, schema).map_err(graph_error)?,
                ),
            })
        })
    }
}

#[pyclass(name = "_GraphDatabase")]
pub(crate) struct PyGraphDatabase {
    database: Option<GraphDatabase>,
}

#[pymethods]
impl PyGraphDatabase {
    #[staticmethod]
    fn load(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        py.detach(move || {
            Ok(Self {
                database: Some(GraphDatabase::load_from_dir(path).map_err(graph_error)?),
            })
        })
    }

    #[staticmethod]
    fn validate(py: Python<'_>, path: PathBuf) -> PyResult<()> {
        py.detach(move || GraphDatabase::validate_dir(path).map_err(graph_error))
    }

    fn save(&self, py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
        let database = self.require_database()?;
        let sizes = py.detach(move || database.save_to_dir(path).map_err(graph_error))?;
        graph_size_report_to_py(py, sizes)
    }

    #[pyo3(signature = (nodes, traversals, limits, *, cancellation = None, timeout_ms = None))]
    fn query_nodes(
        &self,
        py: Python<'_>,
        nodes: Vec<QueryNodeInput>,
        traversals: Vec<QueryTraversalInput>,
        limits: QueryLimitsInput,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let query = node_query(nodes, traversals, limits)?;
        self.execute_query(py, query, cancellation, timeout_ms)
    }

    #[pyo3(signature = (
        node_type,
        field,
        values,
        traversals,
        limits,
        *,
        cancellation = None,
        timeout_ms = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn query_equals(
        &self,
        py: Python<'_>,
        node_type: String,
        field: Vec<String>,
        values: Vec<Py<PyAny>>,
        traversals: Vec<QueryTraversalInput>,
        limits: QueryLimitsInput,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let query = equals_query(py, node_type, field, values, traversals, limits)?;
        self.execute_query(py, query, cancellation, timeout_ms)
    }

    #[pyo3(signature = (selection, *, r#where = None))]
    fn project_candidates(
        &self,
        py: Python<'_>,
        selection: &PyGraphSelection,
        r#where: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let selection = selection
            .result
            .as_ref()
            .ok_or_else(|| StaleGraphSelectionError::new_err("graph selection has been closed"))?;
        let database = self.require_database()?;
        let projection = py
            .detach(move || database.project_candidate_identities(selection, filter.as_ref()))
            .map_err(graph_error)?;
        graph_candidate_projection_to_py(py, projection)
    }

    fn records_json(&self, record_ids: Vec<String>) -> PyResult<String> {
        records_json(self.require_database()?.corpus(), record_ids)
    }

    fn chunks_json(&self, chunk_ids: Vec<u64>) -> PyResult<String> {
        chunks_json(self.require_database()?.corpus(), chunk_ids)
    }

    fn close(&mut self) {
        self.database = None;
    }

    #[getter]
    fn closed(&self) -> bool {
        self.database.is_none()
    }
}

impl PyGraphDatabase {
    fn require_database(&self) -> PyResult<&GraphDatabase> {
        self.database
            .as_ref()
            .ok_or_else(|| GraphError::new_err("graph database has been closed"))
    }

    fn execute_query(
        &self,
        py: Python<'_>,
        query: GraphQuery,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let database = self.require_database()?;
        let cancellation = cancellation.map(|value| value.token.clone());
        py.detach(move || {
            let result = execute_query_with_controls(cancellation.as_ref(), timeout_ms, |token| {
                database.graph_query(&query, token)
            })
            .map_err(graph_error)?;
            let projected = database.project_candidates(&result).map_err(graph_error)?;
            Ok(PyGraphSelection {
                result: Some(result),
                projected_chunk_count: projected.trace.resolved_chunks,
            })
        })
    }
}

#[pyclass(name = "_GraphRetrievalDatabase")]
pub(crate) struct PyGraphRetrievalDatabase {
    database: Option<GraphRetrievalDatabase>,
}

#[pymethods]
impl PyGraphRetrievalDatabase {
    #[staticmethod]
    fn load(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        py.detach(move || {
            Ok(Self {
                database: Some(GraphRetrievalDatabase::load_from_dir(path).map_err(graph_error)?),
            })
        })
    }

    #[staticmethod]
    fn validate(py: Python<'_>, path: PathBuf) -> PyResult<()> {
        py.detach(move || GraphRetrievalDatabase::validate_dir(path).map_err(graph_error))
    }

    fn save(&self, py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
        let database = self.require_database()?;
        let sizes = py.detach(move || database.save_to_dir(path).map_err(graph_error))?;
        graph_size_report_to_py(py, sizes)
    }

    #[pyo3(signature = (nodes, traversals, limits, *, cancellation = None, timeout_ms = None))]
    fn query_nodes(
        &self,
        py: Python<'_>,
        nodes: Vec<QueryNodeInput>,
        traversals: Vec<QueryTraversalInput>,
        limits: QueryLimitsInput,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let query = node_query(nodes, traversals, limits)?;
        self.execute_query(py, query, cancellation, timeout_ms)
    }

    #[pyo3(signature = (
        node_type,
        field,
        values,
        traversals,
        limits,
        *,
        cancellation = None,
        timeout_ms = None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn query_equals(
        &self,
        py: Python<'_>,
        node_type: String,
        field: Vec<String>,
        values: Vec<Py<PyAny>>,
        traversals: Vec<QueryTraversalInput>,
        limits: QueryLimitsInput,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let query = equals_query(py, node_type, field, values, traversals, limits)?;
        self.execute_query(py, query, cancellation, timeout_ms)
    }

    #[pyo3(signature = (selection, *, r#where = None))]
    fn project_candidates(
        &self,
        py: Python<'_>,
        selection: &PyGraphSelection,
        r#where: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let selection = selection
            .result
            .as_ref()
            .ok_or_else(|| StaleGraphSelectionError::new_err("graph selection has been closed"))?;
        let database = self.require_database()?;
        let projection = py
            .detach(move || database.project_candidate_identities(selection, filter.as_ref()))
            .map_err(graph_error)?;
        graph_candidate_projection_to_py(py, projection)
    }

    #[pyo3(signature = (embedding, *, limit = 10, r#where = None, selection = None))]
    fn search(
        &self,
        py: Python<'_>,
        embedding: Vec<f32>,
        limit: usize,
        r#where: Option<&Bound<'_, PyAny>>,
        selection: Option<&PyGraphSelection>,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let query = SearchQuery {
            embedding,
            top_k: limit,
            filter,
        };
        let database = self.require_database()?;
        let selection = selection
            .map(|selection| {
                selection.result.as_ref().ok_or_else(|| {
                    StaleGraphSelectionError::new_err("graph selection has been closed")
                })
            })
            .transpose()?;
        let hits = py
            .detach(move || match selection {
                Some(selection) => database.semantic_search_in_selection(&query, selection),
                None => database.semantic_search(&query),
            })
            .map_err(graph_error)?;
        graph_search_hits_to_py(py, database.corpus(), &hits)
    }

    #[pyo3(signature = (
        text,
        embedding,
        *,
        limit = 10,
        r#where = None,
        selection = None,
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
        selection: Option<&PyGraphSelection>,
        vector_candidates: Option<usize>,
        keyword_candidates: Option<usize>,
        alpha: f32,
    ) -> PyResult<Py<PyAny>> {
        let filter = parse_optional_filter(r#where)?;
        let mut query = HybridQuery::new(text, embedding, limit);
        let vector_top_k = vector_candidates.unwrap_or(query.vector_top_k);
        let keyword_top_k = keyword_candidates.unwrap_or(query.keyword_top_k);
        query = query.with_candidate_limits(vector_top_k, keyword_top_k);
        if let Some(filter) = filter {
            query = query.with_filter(filter);
        }
        query = query.try_with_alpha(alpha).map_err(py_error)?;
        let database = self.require_database()?;
        let selection = selection
            .map(|selection| {
                selection.result.as_ref().ok_or_else(|| {
                    StaleGraphSelectionError::new_err("graph selection has been closed")
                })
            })
            .transpose()?;
        let hits = py
            .detach(move || match selection {
                Some(selection) => database.hybrid_search_in_selection(&query, selection),
                None => database.hybrid_search(&query),
            })
            .map_err(graph_error)?;
        graph_hybrid_hits_to_py(py, database.corpus(), &hits, alpha)
    }

    fn records_json(&self, record_ids: Vec<String>) -> PyResult<String> {
        records_json(self.require_database()?.corpus(), record_ids)
    }

    fn chunks_json(&self, chunk_ids: Vec<u64>) -> PyResult<String> {
        chunks_json(self.require_database()?.corpus(), chunk_ids)
    }

    fn close(&mut self) {
        self.database = None;
    }

    #[getter]
    fn closed(&self) -> bool {
        self.database.is_none()
    }
}

impl PyGraphRetrievalDatabase {
    fn require_database(&self) -> PyResult<&GraphRetrievalDatabase> {
        self.database
            .as_ref()
            .ok_or_else(|| GraphError::new_err("graph retrieval database has been closed"))
    }

    fn execute_query(
        &self,
        py: Python<'_>,
        query: GraphQuery,
        cancellation: Option<&PyGraphCancellationToken>,
        timeout_ms: Option<u64>,
    ) -> PyResult<PyGraphSelection> {
        let database = self.require_database()?;
        let cancellation = cancellation.map(|value| value.token.clone());
        py.detach(move || {
            let result = execute_query_with_controls(cancellation.as_ref(), timeout_ms, |token| {
                database.graph_query(&query, token)
            })
            .map_err(graph_error)?;
            let projected = database.project_candidates(&result).map_err(graph_error)?;
            Ok(PyGraphSelection {
                result: Some(result),
                projected_chunk_count: projected.trace.resolved_chunks,
            })
        })
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyGraphCancellationToken>()?;
    module.add_class::<PyGraphSelection>()?;
    module.add_class::<PyGraphDatabaseBuilder>()?;
    module.add_class::<PyGraphRetrievalDatabaseBuilder>()?;
    module.add_class::<PyGraphDatabase>()?;
    module.add_class::<PyGraphRetrievalDatabase>()?;
    Ok(())
}

fn execute_query_with_controls(
    cancellation: Option<&CancellationToken>,
    timeout_ms: Option<u64>,
    execute: impl FnOnce(Option<&CancellationToken>) -> Result<GraphResult, RustGraphError>,
) -> Result<GraphResult, RustGraphError> {
    let Some(timeout_ms) = timeout_ms else {
        return execute(cancellation);
    };

    let local = CancellationToken::default();
    if timeout_ms == 0 {
        local.cancel();
        return execute(Some(&local)).map_err(|error| match error {
            RustGraphError::Cancelled => RustGraphError::TimedOut {
                message: "graph query exceeded timeout of 0 ms".to_owned(),
            },
            other => other,
        });
    }

    let monitor_token = local.clone();
    let external = cancellation.cloned();
    let timed_out = Arc::new(AtomicBool::new(false));
    let monitor_timed_out = Arc::clone(&timed_out);
    let (done_tx, done_rx) = mpsc::channel();
    let timeout = Duration::from_millis(timeout_ms);
    let monitor = std::thread::spawn(move || {
        let deadline = Instant::now() + timeout;
        loop {
            if external
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                monitor_token.cancel();
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                monitor_timed_out.store(true, Ordering::Release);
                monitor_token.cancel();
                return;
            }
            let remaining = deadline.saturating_duration_since(now);
            let poll = if external.is_some() {
                remaining.min(Duration::from_millis(5))
            } else {
                remaining
            };
            match done_rx.recv_timeout(poll) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    });
    let result = execute(Some(&local));
    let _ = done_tx.send(());
    let _ = monitor.join();
    if timed_out.load(Ordering::Acquire) && matches!(result, Err(RustGraphError::Cancelled)) {
        return Err(RustGraphError::TimedOut {
            message: format!("graph query exceeded timeout of {timeout_ms} ms"),
        });
    }
    result
}

fn parse_schema(value: &str) -> PyResult<GraphSchema> {
    serde_json::from_str(value)
        .map_err(|error| InvalidGraphSchemaError::new_err(format!("invalid graph schema: {error}")))
}

fn parse_records(value: &str) -> PyResult<Vec<RecordBatch>> {
    serde_json::from_str(value)
        .map_err(|error| PyValueError::new_err(format!("invalid graph records: {error}")))
}

fn node_query(
    nodes: Vec<QueryNodeInput>,
    traversals: Vec<QueryTraversalInput>,
    limits: QueryLimitsInput,
) -> PyResult<GraphQuery> {
    graph_query(
        Seed::NodeIds(
            nodes
                .into_iter()
                .map(|(node_type, record_id, chunk_key)| {
                    query_node(node_type, record_id, chunk_key)
                })
                .collect::<PyResult<Vec<_>>>()?,
        ),
        traversals,
        limits,
    )
}

fn equals_query(
    py: Python<'_>,
    node_type: String,
    field: Vec<String>,
    values: Vec<Py<PyAny>>,
    traversals: Vec<QueryTraversalInput>,
    limits: QueryLimitsInput,
) -> PyResult<GraphQuery> {
    let seed = Seed::Equals {
        node_type: NodeType::new(node_type).map_err(graph_error)?,
        field: retrievalkit_graph::FieldPath::new(
            field
                .into_iter()
                .map(|field| retrievalkit_core::FieldName::new(field).map_err(core_error))
                .collect::<PyResult<Vec<_>>>()?,
        )
        .map_err(graph_error)?,
        values: values
            .into_iter()
            .map(|value| query_scalar(value.bind(py)))
            .collect::<PyResult<Vec<_>>>()?,
    };
    graph_query(seed, traversals, limits)
}

fn graph_query(
    seed: Seed,
    traversals: Vec<QueryTraversalInput>,
    limits: QueryLimitsInput,
) -> PyResult<GraphQuery> {
    let steps = traversals
        .into_iter()
        .map(
            |(relationship, direction, min_hops, max_hops)| -> PyResult<Traverse> {
                let direction = match direction.to_ascii_lowercase().as_str() {
                    "outgoing" => Direction::Outgoing,
                    "incoming" => Direction::Incoming,
                    other => {
                        return Err(GraphQueryError::new_err(format!(
                            "unsupported graph direction '{other}'"
                        )))
                    }
                };
                Ok(Traverse {
                    relationship: RelationshipType::new(relationship).map_err(graph_error)?,
                    direction,
                    min_hops: non_negative_size("traversal min_hops", min_hops)?,
                    max_hops: non_negative_size("traversal max_hops", max_hops)?,
                })
            },
        )
        .collect::<PyResult<Vec<_>>>()?;
    let (max_hops, max_visited, max_results, max_working_bytes) = limits;
    Ok(GraphQuery {
        seed,
        steps,
        limits: QueryLimits {
            max_hops: non_negative_size("max_hops", max_hops)?,
            max_visited: non_negative_size("max_visited", max_visited)?,
            max_results: non_negative_size("max_results", max_results)?,
            max_working_bytes: non_negative_size("max_working_bytes", max_working_bytes)?,
        },
    })
}

fn non_negative_size(name: &str, value: i64) -> PyResult<usize> {
    usize::try_from(value).map_err(|_| {
        GraphQueryError::new_err(format!(
            "{name} must be non-negative; received {value}; pass zero or a positive integer"
        ))
    })
}

fn query_node(node_type: String, record_id: String, chunk_key: Option<String>) -> PyResult<NodeId> {
    let node_type = NodeType::new(node_type).map_err(graph_error)?;
    let record_id = RecordId::new(record_id).map_err(core_error)?;
    match chunk_key {
        Some(chunk_key) => Ok(NodeId::chunk(
            node_type,
            ChunkIdentity::new(record_id, ChunkKey::new(chunk_key).map_err(core_error)?),
        )),
        None => Ok(NodeId::record(node_type, record_id)),
    }
}

fn query_scalar(value: &Bound<'_, PyAny>) -> PyResult<GraphScalar> {
    if value.is_instance_of::<PyBool>() {
        Ok(GraphScalar::Bool(value.extract()?))
    } else if value.is_instance_of::<PyInt>() {
        Ok(GraphScalar::I64(value.extract()?))
    } else if value.is_instance_of::<PyString>() {
        Ok(GraphScalar::String(value.extract()?))
    } else {
        Err(PyTypeError::new_err(
            "graph equality values must be str, int, or bool",
        ))
    }
}

fn graph_result_to_py(
    py: Python<'_>,
    result: &GraphResult,
    projected_chunk_count: usize,
) -> PyResult<Py<PyAny>> {
    let output = PyDict::new(py);
    output.set_item("corpus_id", result.corpus_id.as_str())?;
    output.set_item("generation", result.generation.get())?;

    let matches = PyList::empty(py);
    for matched in &result.matches {
        let item = PyDict::new(py);
        item.set_item("node", node_to_py(py, &matched.node_id)?)?;
        item.set_item("depth", matched.depth)?;
        let path = PyList::empty(py);
        for edge in &matched.path {
            let edge_item = PyDict::new(py);
            edge_item.set_item("relationship", edge.edge_id.relationship_type.as_str())?;
            edge_item.set_item("source", node_to_py(py, &edge.edge_id.source)?)?;
            edge_item.set_item("target", node_to_py(py, &edge.edge_id.target)?)?;
            edge_item.set_item("occurrence_ordinal", edge.edge_id.occurrence_ordinal)?;

            let provenance = PyDict::new(py);
            provenance.set_item("schema_rule_index", edge.provenance.schema_rule_index)?;
            provenance.set_item(
                "source_record_id",
                edge.provenance.source_record_id.as_str(),
            )?;
            let source_field = edge.provenance.source_field.as_ref().map(|path| {
                path.segments()
                    .iter()
                    .map(|field| field.as_str())
                    .collect::<Vec<_>>()
            });
            provenance.set_item("source_field", source_field)?;
            provenance.set_item("derived_inverse", edge.provenance.derived_inverse)?;
            provenance.set_item("built_in", edge.provenance.built_in)?;
            edge_item.set_item("provenance", provenance)?;
            path.append(edge_item)?;
        }
        item.set_item("path", path)?;
        matches.append(item)?;
    }
    output.set_item("matches", matches)?;
    output.set_item("truncated", result.truncated.map(truncation_reason))?;

    let trace = PyDict::new(py);
    trace.set_item("seed_count", result.trace.seed_count)?;
    trace.set_item("visited_states", result.trace.visited_states)?;
    trace.set_item("traversed_edges", result.trace.traversed_edges)?;
    trace.set_item("result_count", result.trace.result_count)?;
    trace.set_item("diagnostics", result.trace.diagnostics)?;
    trace.set_item("projected_chunk_count", projected_chunk_count)?;
    output.set_item("trace", trace)?;
    Ok(output.into_any().unbind())
}

fn node_to_py(py: Python<'_>, node: &NodeId) -> PyResult<Py<PyAny>> {
    let output = PyDict::new(py);
    output.set_item("node_type", node.node_type.as_str())?;
    match &node.source {
        NodeSource::Record(record_id) => {
            output.set_item("record_id", record_id.as_str())?;
            output.set_item("chunk_key", py.None())?;
        }
        NodeSource::Chunk(identity) => {
            output.set_item("record_id", identity.record_id.as_str())?;
            output.set_item("chunk_key", identity.chunk_key.as_str())?;
        }
    }
    Ok(output.into_any().unbind())
}

fn truncation_reason(reason: TruncationReason) -> &'static str {
    match reason {
        TruncationReason::MaxHops => "MaxHops",
        TruncationReason::MaxVisited => "MaxVisited",
        TruncationReason::MaxResults => "MaxResults",
        TruncationReason::MaxWorkingBytes => "MaxWorkingBytes",
    }
}

fn graph_candidate_projection_to_py(
    py: Python<'_>,
    projection: GraphCandidateProjection,
) -> PyResult<Py<PyAny>> {
    let output = PyDict::new(py);
    let candidates = PyList::empty(py);
    for candidate in projection.candidates {
        let item = PyDict::new(py);
        item.set_item("record_id", candidate.record_id.as_str())?;
        item.set_item("chunk_key", candidate.chunk_key.as_str())?;
        candidates.append(item)?;
    }
    output.set_item("candidates", candidates)?;
    output.set_item("source_nodes", projection.source_nodes)?;
    output.set_item(
        "projected_chunks_before_filter",
        projection.projected_chunks_before_filter,
    )?;
    output.set_item(
        "projected_chunks_after_filter",
        projection.projected_chunks_after_filter,
    )?;
    Ok(output.into_any().unbind())
}

fn records_json(corpus: &CorpusIndex, record_ids: Vec<String>) -> PyResult<String> {
    let ids = record_ids
        .into_iter()
        .map(|record_id| RecordId::new(record_id).map_err(core_error))
        .collect::<PyResult<Vec<_>>>()?;
    serde_json::to_string(&corpus.hydrate_records(&ids)).map_err(json_error)
}

fn chunks_json(corpus: &CorpusIndex, chunk_ids: Vec<u64>) -> PyResult<String> {
    serde_json::to_string(&corpus.hydrate_chunks(&chunk_ids)).map_err(json_error)
}

fn graph_search_hits_to_py(
    py: Python<'_>,
    corpus: &CorpusIndex,
    hits: &[SearchHit],
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = corpus
            .chunk(hit.chunk_id)
            .ok_or_else(|| RetrievalKitError::new_err("search hit referenced a missing chunk"))?;
        result.append(search_hit_to_py(py, hit, chunk)?)?;
    }
    Ok(result.into_any().unbind())
}

fn graph_hybrid_hits_to_py(
    py: Python<'_>,
    corpus: &CorpusIndex,
    hits: &[HybridHit],
    alpha: f32,
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for hit in hits {
        let chunk = corpus
            .chunk(hit.chunk_id)
            .ok_or_else(|| RetrievalKitError::new_err("hybrid hit referenced a missing chunk"))?;
        let item = PyDict::new(py);
        item.set_item("chunk_id", hit.chunk_id)?;
        item.set_item("document_id", &hit.document_id)?;
        item.set_item("text", &chunk.text)?;
        item.set_item("metadata", metadata_to_py(py, &chunk.metadata)?)?;
        item.set_item("score", hit.score)?;
        item.set_item("vector_score", hit.vector_score)?;
        item.set_item("keyword_score", hit.keyword_score)?;
        item.set_item("matched_terms", &hit.trace.matched_terms)?;
        item.set_item("trace", hybrid_trace_to_py(py, hit, alpha)?)?;
        result.append(item)?;
    }
    Ok(result.into_any().unbind())
}

fn graph_size_report_to_py(py: Python<'_>, report: GraphDatabaseFileSizes) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    result.set_item("corpus_bytes", report.corpus_bytes)?;
    result.set_item("schema_bytes", report.schema_bytes)?;
    result.set_item("graph_bytes", report.graph_bytes)?;
    Ok(result.into_any().unbind())
}

fn json_error(error: serde_json::Error) -> PyErr {
    PyRuntimeError::new_err(format!("could not encode graph result: {error}"))
}

fn core_error(error: CoreError) -> PyErr {
    py_error(error)
}

fn graph_error(error: RustGraphError) -> PyErr {
    match error {
        RustGraphError::InvalidSchema { .. } => InvalidGraphSchemaError::new_err(error.to_string()),
        RustGraphError::InvalidQuery { .. } | RustGraphError::QueryLimitExceeded { .. } => {
            GraphQueryError::new_err(error.to_string())
        }
        RustGraphError::StaleGeneration { .. } => {
            StaleGraphSelectionError::new_err(error.to_string())
        }
        RustGraphError::Cancelled => GraphCancelledError::new_err(error.to_string()),
        RustGraphError::TimedOut { .. } => GraphTimeoutError::new_err(error.to_string()),
        RustGraphError::Core { ref message } if message.contains("invalid vector dimension") => {
            DimensionMismatchError::new_err(error.to_string())
        }
        RustGraphError::Core { ref message } if message.contains("retrieval is unavailable") => {
            RetrievalCapabilityUnavailableError::new_err(error.to_string())
        }
        _ => GraphError::new_err(error.to_string()),
    }
}
