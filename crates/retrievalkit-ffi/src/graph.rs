use std::ffi::c_char;
use std::path::Path;
use std::slice;

use retrievalkit_core::{
    Bm25Config, CandidateScope, ChunkKey, CorpusId, EmbeddedDocument, ExactVectorIndex, HybridHit,
    HybridQuery, IndexConfig, KeywordHit, KeywordQuery, Metadata, Record, RecordChunkInput,
    RecordInput as CoreRecordInput, SearchHit, SearchQuery,
};
use retrievalkit_graph::{
    CancellationToken, Direction, GraphDatabase, GraphDatabaseBuilder, GraphIndex, GraphQuery,
    GraphResult, GraphRetrievalDatabase, GraphRetrievalDatabaseBuilder, GraphScalar, NodeId,
    NodeSource, QueryLimits, Seed, Traverse,
};
use retrievalkit_graph::{FieldPath, GraphSchema, NodeType, RelationshipType};
use serde::Deserialize;

use super::{
    empty_hybrid_result_buffer, empty_keyword_result_buffer, empty_search_result_buffer, ffi_bool,
    ffi_ptr, optional_filter, packed_hybrid_result_buffer, packed_keyword_result_buffer,
    packed_search_result_buffer, parse_encoding_code, parse_metric, read_c_string, read_f32_slice,
    string_array, string_to_owned_ptr, FfiError, PackedRecordId, PackedResultPayload,
    RetrievalKitFilter, RetrievalKitHybridQueryOptions, RetrievalKitHybridResultBuffer,
    RetrievalKitKeywordResultBuffer, RetrievalKitSearchResultBuffer, RetrievalKitStatus,
};

const RETRIEVALKIT_GRAPH_STATUS_INVALID_SCHEMA: i32 = 100;
const RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY: i32 = 101;
const RETRIEVALKIT_GRAPH_STATUS_STALE_GENERATION: i32 = 102;
const RETRIEVALKIT_GRAPH_STATUS_INCOMPATIBLE_VERSION: i32 = 103;
const RETRIEVALKIT_GRAPH_STATUS_GRAPH_UNAVAILABLE: i32 = 104;
const RETRIEVALKIT_GRAPH_STATUS_CORRUPT_SNAPSHOT: i32 = 105;
const RETRIEVALKIT_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED: i32 = 106;
const RETRIEVALKIT_GRAPH_STATUS_CANCELLED: i32 = 107;
const RETRIEVALKIT_GRAPH_STATUS_TIMED_OUT: i32 = 108;
const RETRIEVALKIT_GRAPH_STATUS_LOCK_UNAVAILABLE: i32 = 109;
const RETRIEVALKIT_GRAPH_STATUS_INTERNAL: i32 = 110;
const RETRIEVALKIT_GRAPH_STATUS_INVALID_DIMENSION: i32 = 111;
const RETRIEVALKIT_GRAPH_STATUS_MISSING_EMBEDDING: i32 = 112;
const RETRIEVALKIT_GRAPH_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE: i32 = 113;

pub struct RetrievalKitGraphBuilder {
    core: ExactVectorIndex,
}

pub struct RetrievalKitGraphIndex {
    index: GraphIndex,
}

pub struct RetrievalKitGraphDatabaseBuilder {
    builder: GraphDatabaseBuilder,
}

pub struct RetrievalKitGraphDatabase {
    database: GraphDatabase,
}

pub struct RetrievalKitGraphRetrievalBuilder {
    builder: GraphRetrievalDatabaseBuilder,
}

pub struct RetrievalKitGraphRetrievalDatabase {
    database: GraphRetrievalDatabase,
}

pub struct RetrievalKitGraphResult {
    result: GraphResult,
}
pub struct RetrievalKitGraphCancellation {
    token: CancellationToken,
}
pub struct RetrievalKitGraphScope {
    scope: CandidateScope,
    pub source_nodes: usize,
    pub resolved_chunks: usize,
}

#[repr(C)]
pub struct RetrievalKitGraphChunkIdentity {
    pub record_id: *mut c_char,
    pub chunk_key: *mut c_char,
}

#[repr(C)]
pub struct RetrievalKitGraphCandidateProjection {
    pub candidates: *mut RetrievalKitGraphChunkIdentity,
    pub count: usize,
    pub source_nodes: usize,
    pub projected_chunks_before_filter: usize,
    pub projected_chunks_after_filter: usize,
}

impl Default for RetrievalKitGraphCandidateProjection {
    fn default() -> Self {
        Self {
            candidates: std::ptr::null_mut(),
            count: 0,
            source_nodes: 0,
            projected_chunks_before_filter: 0,
            projected_chunks_after_filter: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitGraphNodeRef {
    pub node_type: *const c_char,
    pub source_type: u32,
    pub record_id: *const c_char,
    pub chunk_key: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitGraphScalar {
    pub value_type: u32,
    pub string_value: *const c_char,
    pub integer_value: i64,
    pub bool_value: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitGraphStep {
    pub relationship: *const c_char,
    pub direction: u32,
    pub min_hops: usize,
    pub max_hops: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitGraphLimits {
    pub max_hops: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_working_bytes: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitGraphQuery {
    pub seed_type: u32,
    pub node_ids: *const RetrievalKitGraphNodeRef,
    pub node_id_count: usize,
    pub seed_node_type: *const c_char,
    pub field_segments: *const *const c_char,
    pub field_segment_count: usize,
    pub values: *const RetrievalKitGraphScalar,
    pub value_count: usize,
    pub steps: *const RetrievalKitGraphStep,
    pub step_count: usize,
    pub limits: RetrievalKitGraphLimits,
}
#[repr(C)]
pub struct RetrievalKitGraphMatch {
    pub node_type: *mut c_char,
    pub source_type: u32,
    pub record_id: *mut c_char,
    pub chunk_key: *mut c_char,
    pub depth: usize,
    pub path_length: usize,
}
#[repr(C)]
pub struct RetrievalKitGraphOwnedNode {
    pub node_type: *mut c_char,
    pub source_type: u32,
    pub record_id: *mut c_char,
    pub chunk_key: *mut c_char,
}
#[repr(C)]
pub struct RetrievalKitGraphPathEdge {
    pub relationship_type: *mut c_char,
    pub source: RetrievalKitGraphOwnedNode,
    pub target: RetrievalKitGraphOwnedNode,
    pub occurrence_ordinal: u32,
    pub schema_rule_index: u32,
    pub source_record_id: *mut c_char,
    pub source_field_segments: super::RetrievalKitStringArray,
    pub derived_inverse: bool,
    pub built_in: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RetrievalKitGraphTrace {
    pub seed_count: usize,
    pub visited_states: usize,
    pub traversed_edges: usize,
    pub result_count: usize,
    pub diagnostics: usize,
    pub truncation_reason: u32,
}

#[derive(Deserialize)]
struct RecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<RecordChunk>,
}

#[derive(Deserialize)]
struct RecordChunk {
    key: ChunkKey,
    text: String,
    embedding: Vec<f32>,
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Deserialize)]
struct GraphOnlyRecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<GraphOnlyRecordChunk>,
}

#[derive(Deserialize)]
struct GraphOnlyRecordChunk {
    key: ChunkKey,
    text: String,
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Deserialize)]
struct EmbeddedRecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    documents: Vec<EmbeddedDocument>,
}

#[no_mangle]
pub extern "C" fn retrievalkit_graph_ffi_abi_version() -> u32 {
    12
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_builder_new(
    corpus_id: *const c_char,
    schema_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphDatabaseBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let schema = decode_schema(unsafe { read_c_string(schema_json, "schema_json") }?)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphDatabaseBuilder {
            builder: GraphDatabaseBuilder::new(corpus_id, schema),
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_builder_upsert_record_json(
    builder: *mut RetrievalKitGraphDatabaseBuilder,
    record_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("graph builder must not be null"))?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: GraphOnlyRecordBatch = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid record JSON: {error}")))?;
        if batch.chunks.is_empty() {
            builder
                .builder
                .upsert_record(batch.record, batch.projected_metadata)?;
        } else {
            builder.builder.upsert_input(CoreRecordInput {
                record: batch.record,
                metadata: batch.projected_metadata,
                chunks: batch
                    .chunks
                    .into_iter()
                    .map(|chunk| retrievalkit_core::CorpusChunkInput {
                        key: chunk.key,
                        text: chunk.text,
                        metadata: chunk.metadata,
                    })
                    .collect(),
            })?;
        }
        Ok(())
    })
}

/// Consumes `builder` whether graph construction succeeds or fails.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_builder_build(
    builder: *mut RetrievalKitGraphDatabaseBuilder,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphDatabase {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument("graph builder must not be null"));
        }
        let builder = unsafe { Box::from_raw(builder) };
        let database = builder.builder.build()?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphDatabase {
            database,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_builder_free(
    builder: *mut RetrievalKitGraphDatabaseBuilder,
) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_new(
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    schema_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphRetrievalBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let schema = decode_schema(unsafe { read_c_string(schema_json, "schema_json") }?)?;
        let builder = GraphRetrievalDatabaseBuilder::new(
            corpus_id,
            schema,
            parse_metric(metric)?,
            parse_encoding_code(encoding)?,
        );
        Ok(Box::into_raw(Box::new(RetrievalKitGraphRetrievalBuilder {
            builder,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_new_with_bm25(
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    schema_json: *const c_char,
    bm25_k1: f32,
    bm25_b: f32,
    stop_words_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphRetrievalBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let schema = decode_schema(unsafe { read_c_string(schema_json, "schema_json") }?)?;
        let stop_words_json = unsafe { read_c_string(stop_words_json, "stop_words_json")? };
        let stop_words =
            serde_json::from_str::<Vec<String>>(&stop_words_json).map_err(|error| {
                FfiError::invalid_argument(format!("invalid stop_words_json: {error}"))
            })?;
        let builder = GraphRetrievalDatabaseBuilder::new(
            corpus_id,
            schema,
            parse_metric(metric)?,
            parse_encoding_code(encoding)?,
        )
        .try_with_bm25_config(Bm25Config::try_new(bm25_k1, bm25_b, stop_words)?)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphRetrievalBuilder {
            builder,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_upsert_record_json(
    builder: *mut RetrievalKitGraphRetrievalBuilder,
    record_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval builder must not be null")
        })?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: RecordBatch = serde_json::from_str(&json).map_err(|error| {
            let code = if error.to_string().contains("missing field `embedding`") {
                RETRIEVALKIT_GRAPH_STATUS_MISSING_EMBEDDING
            } else {
                super::RETRIEVALKIT_STATUS_INVALID_ARGUMENT
            };
            FfiError {
                code,
                message: format!("invalid retrieval record JSON: {error}"),
            }
        })?;
        if batch.chunks.is_empty() {
            builder
                .builder
                .upsert_record(batch.record, batch.projected_metadata)?;
        } else {
            builder.builder.upsert_record_chunks(
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
            )?;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_upsert_record_with_embedding_json(
    builder: *mut RetrievalKitGraphRetrievalBuilder,
    record_json: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval builder must not be null")
        })?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: GraphOnlyRecordBatch = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid record JSON: {error}")))?;
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec();
        builder.builder.upsert_record_with_embedding(
            batch.record,
            batch.projected_metadata,
            embedding,
        )?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_upsert_documents_json(
    builder: *mut RetrievalKitGraphRetrievalBuilder,
    batch_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval builder must not be null")
        })?;
        let json = unsafe { read_c_string(batch_json, "batch_json") }?;
        let batch: EmbeddedRecordBatch = serde_json::from_str(&json).map_err(|error| {
            FfiError::invalid_argument(format!("invalid embedded-document JSON: {error}"))
        })?;
        builder.builder.upsert_record_documents(
            batch.record,
            batch.projected_metadata,
            batch.documents,
        )?;
        Ok(())
    })
}

/// Consumes `builder` whether graph construction succeeds or fails.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_build(
    builder: *mut RetrievalKitGraphRetrievalBuilder,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphRetrievalDatabase {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument(
                "graph retrieval builder must not be null",
            ));
        }
        let builder = unsafe { Box::from_raw(builder) };
        let database = builder.builder.build()?;
        Ok(Box::into_raw(Box::new(
            RetrievalKitGraphRetrievalDatabase { database },
        )))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_builder_free(
    builder: *mut RetrievalKitGraphRetrievalBuilder,
) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_load(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphDatabase {
    ffi_ptr(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        let database = GraphDatabase::load_from_dir(Path::new(&path))?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphDatabase {
            database,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_save(
    database: *const RetrievalKitGraphDatabase,
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph database must not be null"))?;
        let path = unsafe { read_c_string(directory, "directory") }?;
        database.database.save_to_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_validate(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        GraphDatabase::validate_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_free(
    database: *mut RetrievalKitGraphDatabase,
) {
    if !database.is_null() {
        drop(unsafe { Box::from_raw(database) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_load(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphRetrievalDatabase {
    ffi_ptr(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        let database = GraphRetrievalDatabase::load_from_dir(Path::new(&path))?;
        Ok(Box::into_raw(Box::new(
            RetrievalKitGraphRetrievalDatabase { database },
        )))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_save(
    database: *const RetrievalKitGraphRetrievalDatabase,
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let path = unsafe { read_c_string(directory, "directory") }?;
        database.database.save_to_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_validate(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        GraphRetrievalDatabase::validate_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_free(
    database: *mut RetrievalKitGraphRetrievalDatabase,
) {
    if !database.is_null() {
        drop(unsafe { Box::from_raw(database) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_query(
    database: *const RetrievalKitGraphDatabase,
    query: RetrievalKitGraphQuery,
    cancellation: *const RetrievalKitGraphCancellation,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphResult {
    ffi_ptr(status, || {
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph database must not be null"))?;
        let query = unsafe { decode_query(query) }?;
        let cancellation = unsafe { cancellation.as_ref() }.map(|value| &value.token);
        let result = database.database.graph_query(&query, cancellation)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphResult { result })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_query(
    database: *const RetrievalKitGraphRetrievalDatabase,
    query: RetrievalKitGraphQuery,
    cancellation: *const RetrievalKitGraphCancellation,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphResult {
    ffi_ptr(status, || {
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let query = unsafe { decode_query(query) }?;
        let cancellation = unsafe { cancellation.as_ref() }.map(|value| &value.token);
        let result = database.database.graph_query(&query, cancellation)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphResult { result })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_database_project_candidates(
    database: *const RetrievalKitGraphDatabase,
    result: *const RetrievalKitGraphResult,
    filter: *const RetrievalKitFilter,
    out_projection: *mut RetrievalKitGraphCandidateProjection,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph database must not be null"))?;
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let out_projection = unsafe { out_projection.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("out_projection must not be null"))?;
        let filter = unsafe { optional_filter(filter) };
        let projection = database
            .database
            .project_candidate_identities(&result.result, filter.as_ref())?;
        *out_projection = candidate_projection_buffer(projection);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_database_project_candidates(
    database: *const RetrievalKitGraphRetrievalDatabase,
    result: *const RetrievalKitGraphResult,
    filter: *const RetrievalKitFilter,
    out_projection: *mut RetrievalKitGraphCandidateProjection,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let out_projection = unsafe { out_projection.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("out_projection must not be null"))?;
        let filter = unsafe { optional_filter(filter) };
        let projection = database
            .database
            .project_candidate_identities(&result.result, filter.as_ref())?;
        *out_projection = candidate_projection_buffer(projection);
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_candidate_projection_free(
    projection: RetrievalKitGraphCandidateProjection,
) {
    if projection.candidates.is_null() {
        return;
    }
    let candidates = unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            projection.candidates,
            projection.count,
        ))
    };
    for candidate in candidates.iter() {
        unsafe {
            super::retrievalkit_string_free(candidate.record_id);
            super::retrievalkit_string_free(candidate.chunk_key);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_candidate_projection_clear(
    projection: *mut RetrievalKitGraphCandidateProjection,
) {
    let Some(projection) = (unsafe { projection.as_mut() }) else {
        return;
    };
    let owned = std::mem::take(projection);
    unsafe { retrievalkit_graph_candidate_projection_free(owned) };
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_semantic_search(
    database: *const RetrievalKitGraphRetrievalDatabase,
    within: *const RetrievalKitGraphResult,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    out_results: *mut RetrievalKitSearchResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_search_result_buffer() };
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let mut query = SearchQuery::new(
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        );
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = unsafe { within.as_ref() } {
            database
                .database
                .semantic_search_in_selection(&query, &selection.result)?
        } else {
            database.database.semantic_search(&query)?
        };
        unsafe {
            *out_results = capability_search_buffer(&database.database, hits)?;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_keyword_search(
    database: *const RetrievalKitGraphRetrievalDatabase,
    within: *const RetrievalKitGraphResult,
    text: *const c_char,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    out_results: *mut RetrievalKitKeywordResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_keyword_result_buffer() };
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = unsafe { within.as_ref() } {
            database
                .database
                .keyword_search_in_selection(&query, &selection.result)?
        } else {
            database.database.keyword_search(&query)?
        };
        unsafe {
            *out_results = capability_keyword_buffer(&database.database, hits)?;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_retrieval_hybrid_search_alpha(
    database: *const RetrievalKitGraphRetrievalDatabase,
    within: *const RetrievalKitGraphResult,
    text: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    options: RetrievalKitHybridQueryOptions,
    out_results: *mut RetrievalKitHybridResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_hybrid_result_buffer() };
        let database = unsafe { database.as_ref() }.ok_or_else(|| {
            FfiError::invalid_argument("graph retrieval database must not be null")
        })?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k)
        .try_with_alpha(options.alpha)
        .map_err(|error| FfiError {
            code: super::RETRIEVALKIT_STATUS_INVALID_ARGUMENT,
            message: error.to_string(),
        })?;
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = unsafe { within.as_ref() } {
            database
                .database
                .hybrid_search_in_selection(&query, &selection.result)?
        } else {
            database.database.hybrid_search(&query)?
        };
        unsafe {
            *out_results = capability_hybrid_buffer(&database.database, hits, options.alpha)?;
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_builder_new(
    dimension: usize,
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphBuilder {
    ffi_ptr(status, || {
        let config = IndexConfig::new(dimension, parse_metric(metric)?)
            .with_vector_encoding(parse_encoding_code(encoding)?);
        let corpus = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let core = ExactVectorIndex::try_with_config_in_corpus(config, corpus)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphBuilder { core })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_builder_upsert_record_json(
    builder: *mut RetrievalKitGraphBuilder,
    record_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("graph builder must not be null"))?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: RecordBatch = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid record JSON: {error}")))?;
        let chunks = batch
            .chunks
            .into_iter()
            .map(|chunk| RecordChunkInput {
                key: chunk.key,
                text: chunk.text,
                embedding: chunk.embedding,
                metadata: chunk.metadata,
            })
            .collect();
        builder
            .core
            .upsert_record(batch.record, batch.projected_metadata, chunks)?;
        Ok(())
    })
}

/// Consumes `builder` whether graph construction succeeds or fails.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_builder_build_json(
    builder: *mut RetrievalKitGraphBuilder,
    schema_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphIndex {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument("graph builder must not be null"));
        }
        let builder = unsafe { Box::from_raw(builder) };
        let json = unsafe { read_c_string(schema_json, "schema_json") }?;
        let schema: GraphSchema = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid schema JSON: {error}")))?;
        let index = GraphIndex::build(builder.core, schema)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_builder_free(builder: *mut RetrievalKitGraphBuilder) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_index_load(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphIndex {
    ffi_ptr(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        let index = GraphIndex::load_from_dir(Path::new(&path))?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_index_save(
    index: *const RetrievalKitGraphIndex,
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let path = unsafe { read_c_string(directory, "directory") }?;
        index.index.save_to_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_index_validate(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        GraphIndex::validate_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_index_free(index: *mut RetrievalKitGraphIndex) {
    if !index.is_null() {
        drop(unsafe { Box::from_raw(index) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_query(
    index: *const RetrievalKitGraphIndex,
    query: RetrievalKitGraphQuery,
    cancellation: *const RetrievalKitGraphCancellation,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphResult {
    ffi_ptr(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let query = unsafe { decode_query(query) }?;
        let cancellation = unsafe { cancellation.as_ref() }.map(|value| &value.token);
        let result = index.index.graph_query(&query, cancellation)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphResult { result })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_count(
    result: *const RetrievalKitGraphResult,
) -> usize {
    unsafe { result.as_ref() }.map_or(0, |value| value.result.matches.len())
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_match(
    result: *const RetrievalKitGraphResult,
    index: usize,
    out_match: *mut RetrievalKitGraphMatch,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_match.is_null() {
            return Err(FfiError::invalid_argument("out_match must not be null"));
        }
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let matched = result
            .result
            .matches
            .get(index)
            .ok_or_else(|| FfiError::invalid_argument("graph match index is out of bounds"))?;
        let (source_type, record_id, chunk_key) = match &matched.node_id.source {
            NodeSource::Record(id) => (
                0,
                super::string_to_owned_ptr(id.as_str()),
                std::ptr::null_mut(),
            ),
            NodeSource::Chunk(identity) => (
                1,
                super::string_to_owned_ptr(identity.record_id.as_str()),
                super::string_to_owned_ptr(identity.chunk_key.as_str()),
            ),
        };
        unsafe {
            *out_match = RetrievalKitGraphMatch {
                node_type: super::string_to_owned_ptr(matched.node_id.node_type.as_str()),
                source_type,
                record_id,
                chunk_key,
                depth: matched.depth,
                path_length: matched.path.len(),
            }
        };
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_path_edge(
    result: *const RetrievalKitGraphResult,
    match_index: usize,
    edge_index: usize,
    out_edge: *mut RetrievalKitGraphPathEdge,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_edge.is_null() {
            return Err(FfiError::invalid_argument("out_edge must not be null"));
        }
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let edge = result
            .result
            .matches
            .get(match_index)
            .ok_or_else(|| FfiError::invalid_argument("graph match index is out of bounds"))?
            .path
            .get(edge_index)
            .ok_or_else(|| FfiError::invalid_argument("graph path edge index is out of bounds"))?;
        let source_field_segments =
            edge.provenance
                .source_field
                .as_ref()
                .map_or_else(Vec::new, |field| {
                    field
                        .segments()
                        .iter()
                        .map(|segment| segment.as_str().to_owned())
                        .collect()
                });
        unsafe {
            *out_edge = RetrievalKitGraphPathEdge {
                relationship_type: string_to_owned_ptr(edge.edge_id.relationship_type.as_str()),
                source: owned_node(&edge.edge_id.source),
                target: owned_node(&edge.edge_id.target),
                occurrence_ordinal: edge.edge_id.occurrence_ordinal,
                schema_rule_index: edge.provenance.schema_rule_index,
                source_record_id: string_to_owned_ptr(edge.provenance.source_record_id.as_str()),
                source_field_segments: string_array(source_field_segments),
                derived_inverse: edge.provenance.derived_inverse,
                built_in: edge.provenance.built_in,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_path_edge_clear(value: *mut RetrievalKitGraphPathEdge) {
    if let Some(value) = unsafe { value.as_mut() } {
        unsafe {
            super::retrievalkit_string_free(value.relationship_type);
            owned_node_clear(&mut value.source);
            owned_node_clear(&mut value.target);
            super::retrievalkit_string_free(value.source_record_id);
            super::string_array_free(value.source_field_segments);
        }
        value.relationship_type = std::ptr::null_mut();
        value.source_record_id = std::ptr::null_mut();
        value.source_field_segments = super::RetrievalKitStringArray {
            values: std::ptr::null_mut(),
            count: 0,
        };
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_match_clear(value: *mut RetrievalKitGraphMatch) {
    if let Some(value) = unsafe { value.as_mut() } {
        unsafe {
            super::retrievalkit_string_free(value.node_type);
            super::retrievalkit_string_free(value.record_id);
            super::retrievalkit_string_free(value.chunk_key)
        };
        value.node_type = std::ptr::null_mut();
        value.record_id = std::ptr::null_mut();
        value.chunk_key = std::ptr::null_mut();
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_trace(
    result: *const RetrievalKitGraphResult,
) -> RetrievalKitGraphTrace {
    let Some(result) = (unsafe { result.as_ref() }) else {
        return RetrievalKitGraphTrace::default();
    };
    let trace = &result.result.trace;
    RetrievalKitGraphTrace {
        seed_count: trace.seed_count,
        visited_states: trace.visited_states,
        traversed_edges: trace.traversed_edges,
        result_count: trace.result_count,
        diagnostics: trace.diagnostics,
        truncation_reason: result
            .result
            .truncated
            .map_or(0, |reason| reason as u32 + 1),
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_free(result: *mut RetrievalKitGraphResult) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_result_project(
    index: *const RetrievalKitGraphIndex,
    result: *const RetrievalKitGraphResult,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitGraphScope {
    ffi_ptr(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let projected = index.index.project_candidates(&result.result)?;
        Ok(Box::into_raw(Box::new(RetrievalKitGraphScope {
            scope: projected.scope,
            source_nodes: projected.trace.source_nodes,
            resolved_chunks: projected.trace.resolved_chunks,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_free(scope: *mut RetrievalKitGraphScope) {
    if !scope.is_null() {
        drop(unsafe { Box::from_raw(scope) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_source_nodes(
    scope: *const RetrievalKitGraphScope,
) -> usize {
    unsafe { scope.as_ref() }.map_or(0, |value| value.source_nodes)
}
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_resolved_chunks(
    scope: *const RetrievalKitGraphScope,
) -> usize {
    unsafe { scope.as_ref() }.map_or(0, |value| value.resolved_chunks)
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_search(
    index: *const RetrievalKitGraphIndex,
    scope: *const RetrievalKitGraphScope,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    out_results: *mut RetrievalKitSearchResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_search_result_buffer() };
        let index = unsafe { graph_index(index) }?;
        let scope = unsafe { graph_scope(scope) }?;
        let mut query = SearchQuery::new(
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        );
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        unsafe {
            *out_results = graph_search_buffer(
                index,
                index.index.search_in_candidates(&query, &scope.scope)?,
            )?
        };
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_keyword_search(
    index: *const RetrievalKitGraphIndex,
    scope: *const RetrievalKitGraphScope,
    text: *const c_char,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    out_results: *mut RetrievalKitKeywordResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_keyword_result_buffer() };
        let index = unsafe { graph_index(index) }?;
        let scope = unsafe { graph_scope(scope) }?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        unsafe {
            *out_results = graph_keyword_buffer(
                index,
                index
                    .index
                    .keyword_search_in_candidates(&query, &scope.scope)?,
            )?
        };
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_scope_hybrid_search_alpha(
    index: *const RetrievalKitGraphIndex,
    scope: *const RetrievalKitGraphScope,
    text: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const RetrievalKitFilter,
    options: RetrievalKitHybridQueryOptions,
    out_results: *mut RetrievalKitHybridResultBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        unsafe { *out_results = empty_hybrid_result_buffer() };
        let index = unsafe { graph_index(index) }?;
        let scope = unsafe { graph_scope(scope) }?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k)
        .try_with_alpha(options.alpha)
        .map_err(|error| FfiError {
            code: super::RETRIEVALKIT_STATUS_INVALID_ARGUMENT,
            message: error.to_string(),
        })?;
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        unsafe {
            *out_results = graph_hybrid_buffer(
                index,
                index
                    .index
                    .hybrid_search_in_candidates(&query, &scope.scope)?,
                options.alpha,
            )?
        };
        Ok(())
    })
}
#[no_mangle]
pub extern "C" fn retrievalkit_graph_cancellation_new() -> *mut RetrievalKitGraphCancellation {
    Box::into_raw(Box::new(RetrievalKitGraphCancellation {
        token: CancellationToken::default(),
    }))
}
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_cancellation_cancel(
    value: *const RetrievalKitGraphCancellation,
) {
    if let Some(value) = unsafe { value.as_ref() } {
        value.token.cancel();
    }
}
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_graph_cancellation_free(
    value: *mut RetrievalKitGraphCancellation,
) {
    if !value.is_null() {
        drop(unsafe { Box::from_raw(value) });
    }
}

unsafe fn decode_query(value: RetrievalKitGraphQuery) -> Result<GraphQuery, FfiError> {
    let seed = match value.seed_type {
        0 => Seed::NodeIds(
            unsafe { ffi_slice(value.node_ids, value.node_id_count, "node_ids") }?
                .iter()
                .map(|node| unsafe { decode_node(*node) })
                .collect::<Result<_, _>>()?,
        ),
        1 => {
            let node_type =
                NodeType::new(unsafe { read_c_string(value.seed_node_type, "seed_node_type") }?)
                    .map_err(FfiError::from)?;
            let segments = unsafe {
                ffi_slice(
                    value.field_segments,
                    value.field_segment_count,
                    "field_segments",
                )
            }?
            .iter()
            .map(|segment| {
                unsafe { read_c_string(*segment, "field segment") }
                    .and_then(|s| retrievalkit_core::FieldName::new(s).map_err(FfiError::from))
            })
            .collect::<Result<Vec<_>, _>>()?;
            let field = FieldPath::new(segments).map_err(FfiError::from)?;
            let values = unsafe { ffi_slice(value.values, value.value_count, "values") }?
                .iter()
                .map(|scalar| unsafe { decode_scalar(*scalar) })
                .collect::<Result<_, _>>()?;
            Seed::Equals {
                node_type,
                field,
                values,
            }
        }
        other => {
            return Err(FfiError::invalid_argument(format!(
                "unsupported graph seed type {other}"
            )))
        }
    };
    let steps = unsafe { ffi_slice(value.steps, value.step_count, "steps") }?
        .iter()
        .map(|step| {
            Ok(Traverse {
                relationship: RelationshipType::new(unsafe {
                    read_c_string(step.relationship, "relationship")
                }?)
                .map_err(FfiError::from)?,
                direction: match step.direction {
                    0 => Direction::Outgoing,
                    1 => Direction::Incoming,
                    other => {
                        return Err(FfiError::invalid_argument(format!(
                            "unsupported direction {other}"
                        )))
                    }
                },
                min_hops: step.min_hops,
                max_hops: step.max_hops,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(GraphQuery {
        seed,
        steps,
        limits: QueryLimits {
            max_hops: value.limits.max_hops,
            max_visited: value.limits.max_visited,
            max_results: value.limits.max_results,
            max_working_bytes: value.limits.max_working_bytes,
        },
    })
}

fn owned_node(node: &NodeId) -> RetrievalKitGraphOwnedNode {
    let (source_type, record_id, chunk_key) = match &node.source {
        NodeSource::Record(id) => (0, string_to_owned_ptr(id.as_str()), std::ptr::null_mut()),
        NodeSource::Chunk(identity) => (
            1,
            string_to_owned_ptr(identity.record_id.as_str()),
            string_to_owned_ptr(identity.chunk_key.as_str()),
        ),
    };
    RetrievalKitGraphOwnedNode {
        node_type: string_to_owned_ptr(node.node_type.as_str()),
        source_type,
        record_id,
        chunk_key,
    }
}

unsafe fn owned_node_clear(node: &mut RetrievalKitGraphOwnedNode) {
    unsafe {
        super::retrievalkit_string_free(node.node_type);
        super::retrievalkit_string_free(node.record_id);
        super::retrievalkit_string_free(node.chunk_key);
    }
    node.node_type = std::ptr::null_mut();
    node.record_id = std::ptr::null_mut();
    node.chunk_key = std::ptr::null_mut();
}

unsafe fn decode_node(value: RetrievalKitGraphNodeRef) -> Result<NodeId, FfiError> {
    let node_type = NodeType::new(unsafe { read_c_string(value.node_type, "node_type") }?)
        .map_err(FfiError::from)?;
    let record =
        retrievalkit_core::RecordId::new(unsafe { read_c_string(value.record_id, "record_id") }?)?;
    match value.source_type {
        0 => Ok(NodeId::record(node_type, record)),
        1 => Ok(NodeId::chunk(
            node_type,
            retrievalkit_core::ChunkIdentity::new(
                record,
                ChunkKey::new(unsafe { read_c_string(value.chunk_key, "chunk_key") }?)?,
            ),
        )),
        other => Err(FfiError::invalid_argument(format!(
            "unsupported node source type {other}"
        ))),
    }
}
unsafe fn decode_scalar(value: RetrievalKitGraphScalar) -> Result<GraphScalar, FfiError> {
    match value.value_type {
        0 => Ok(GraphScalar::String(unsafe {
            read_c_string(value.string_value, "scalar string")
        }?)),
        1 => Ok(GraphScalar::I64(value.integer_value)),
        2 => Ok(GraphScalar::Bool(value.bool_value)),
        other => Err(FfiError::invalid_argument(format!(
            "unsupported graph scalar type {other}"
        ))),
    }
}
unsafe fn ffi_slice<'a, T>(
    pointer: *const T,
    count: usize,
    name: &str,
) -> Result<&'a [T], FfiError> {
    if count == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe { slice::from_raw_parts(pointer, count) })
}

unsafe fn graph_index<'a>(
    value: *const RetrievalKitGraphIndex,
) -> Result<&'a RetrievalKitGraphIndex, FfiError> {
    unsafe { value.as_ref() }
        .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))
}
unsafe fn graph_scope<'a>(
    value: *const RetrievalKitGraphScope,
) -> Result<&'a RetrievalKitGraphScope, FfiError> {
    unsafe { value.as_ref() }
        .ok_or_else(|| FfiError::invalid_argument("graph scope must not be null"))
}
fn boxed_buffer<T>(values: Vec<T>) -> (*mut T, usize) {
    let mut values = values.into_boxed_slice();
    let result = (values.as_mut_ptr(), values.len());
    std::mem::forget(values);
    result
}

fn candidate_projection_buffer(
    projection: retrievalkit_graph::GraphCandidateProjection,
) -> RetrievalKitGraphCandidateProjection {
    let candidates = projection
        .candidates
        .into_iter()
        .map(|identity| RetrievalKitGraphChunkIdentity {
            record_id: string_to_owned_ptr(identity.record_id.as_str()),
            chunk_key: string_to_owned_ptr(identity.chunk_key.as_str()),
        })
        .collect();
    let (candidates, count) = boxed_buffer(candidates);
    RetrievalKitGraphCandidateProjection {
        candidates,
        count,
        source_nodes: projection.source_nodes,
        projected_chunks_before_filter: projection.projected_chunks_before_filter,
        projected_chunks_after_filter: projection.projected_chunks_after_filter,
    }
}

fn decode_schema(json: String) -> std::result::Result<GraphSchema, FfiError> {
    serde_json::from_str(&json)
        .map_err(|error| FfiError::invalid_argument(format!("invalid schema JSON: {error}")))
}

fn capability_search_buffer(
    database: &GraphRetrievalDatabase,
    hits: Vec<SearchHit>,
) -> std::result::Result<RetrievalKitSearchResultBuffer, FfiError> {
    packed_search_result_buffer(hits, |chunk_id| {
        capability_result_payload(database, chunk_id)
    })
}

fn capability_keyword_buffer(
    database: &GraphRetrievalDatabase,
    hits: Vec<KeywordHit>,
) -> std::result::Result<RetrievalKitKeywordResultBuffer, FfiError> {
    packed_keyword_result_buffer(hits, |chunk_id| {
        capability_result_payload(database, chunk_id)
    })
}

fn capability_hybrid_buffer(
    database: &GraphRetrievalDatabase,
    hits: Vec<HybridHit>,
    alpha: f32,
) -> std::result::Result<RetrievalKitHybridResultBuffer, FfiError> {
    packed_hybrid_result_buffer(hits, alpha, |chunk_id| {
        capability_result_payload(database, chunk_id)
    })
}

fn capability_result_payload(
    database: &GraphRetrievalDatabase,
    chunk_id: u64,
) -> std::result::Result<PackedResultPayload<'_>, FfiError> {
    let identity = database.corpus().chunk_identity(chunk_id).ok_or_else(|| {
        FfiError::missing_result_payload("graph retrieval result", chunk_id, "stable identity")
    })?;
    let chunk = database.corpus().chunk(chunk_id).ok_or_else(|| {
        FfiError::missing_result_payload("graph retrieval result", chunk_id, "chunk")
    })?;
    Ok(PackedResultPayload {
        document_id: Some(identity.chunk_key.as_str()),
        record_id: PackedRecordId::Value(identity.record_id.as_str()),
        text: &chunk.text,
        metadata: &chunk.metadata,
    })
}

fn graph_search_buffer(
    index: &RetrievalKitGraphIndex,
    hits: Vec<SearchHit>,
) -> std::result::Result<RetrievalKitSearchResultBuffer, FfiError> {
    packed_search_result_buffer(hits, |chunk_id| {
        let (text, metadata) = index.index.chunk_payload(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("graph search result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::DocumentId,
            text,
            metadata,
        })
    })
}

fn graph_keyword_buffer(
    index: &RetrievalKitGraphIndex,
    hits: Vec<KeywordHit>,
) -> std::result::Result<RetrievalKitKeywordResultBuffer, FfiError> {
    packed_keyword_result_buffer(hits, |chunk_id| {
        let (text, metadata) = index.index.chunk_payload(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("graph keyword result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::DocumentId,
            text,
            metadata,
        })
    })
}

fn graph_hybrid_buffer(
    index: &RetrievalKitGraphIndex,
    hits: Vec<HybridHit>,
    alpha: f32,
) -> std::result::Result<RetrievalKitHybridResultBuffer, FfiError> {
    packed_hybrid_result_buffer(hits, alpha, |chunk_id| {
        let (text, metadata) = index.index.chunk_payload(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("graph hybrid result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::DocumentId,
            text,
            metadata,
        })
    })
}

impl From<retrievalkit_graph::GraphError> for FfiError {
    fn from(error: retrievalkit_graph::GraphError) -> Self {
        let code = match &error {
            retrievalkit_graph::GraphError::InvalidSchema { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_INVALID_SCHEMA
            }
            retrievalkit_graph::GraphError::InvalidRecord { .. }
            | retrievalkit_graph::GraphError::InvalidQuery { .. }
            | retrievalkit_graph::GraphError::MissingTarget { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY
            }
            retrievalkit_graph::GraphError::InvalidDimension { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_INVALID_DIMENSION
            }
            retrievalkit_graph::GraphError::MissingEmbedding { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_MISSING_EMBEDDING
            }
            retrievalkit_graph::GraphError::InvalidSnapshot { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_CORRUPT_SNAPSHOT
            }
            retrievalkit_graph::GraphError::StaleGeneration { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_STALE_GENERATION
            }
            retrievalkit_graph::GraphError::IncompatibleVersion { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_INCOMPATIBLE_VERSION
            }
            retrievalkit_graph::GraphError::GraphUnavailable { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_GRAPH_UNAVAILABLE
            }
            retrievalkit_graph::GraphError::WriterBusy { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_LOCK_UNAVAILABLE
            }
            retrievalkit_graph::GraphError::QueryLimitExceeded { .. } => {
                RETRIEVALKIT_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED
            }
            retrievalkit_graph::GraphError::Cancelled => RETRIEVALKIT_GRAPH_STATUS_CANCELLED,
            retrievalkit_graph::GraphError::TimedOut { .. } => RETRIEVALKIT_GRAPH_STATUS_TIMED_OUT,
            retrievalkit_graph::GraphError::Core { message }
                if message.contains("hybrid retrieval is unavailable") =>
            {
                RETRIEVALKIT_GRAPH_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE
            }
            retrievalkit_graph::GraphError::Core { .. } => RETRIEVALKIT_GRAPH_STATUS_INTERNAL,
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use retrievalkit_core::{
        CorpusChunkInput, CorpusIndex, FieldName, Filter, RecordId, RecordType, RecordValue,
        RetrievalConfiguration, RetrievalDatabase, VectorMetric,
    };
    use retrievalkit_graph::{
        ChunkNodeSchema, GraphSchema, NodeType, RecordNodeSchema, RelationshipType,
    };

    use super::*;

    #[test]
    fn graph_errors_map_to_the_stable_public_status_taxonomy() {
        use retrievalkit_graph::GraphError;

        assert_eq!(retrievalkit_graph_ffi_abi_version(), 12);
        let cases = [
            (
                GraphError::InvalidSchema {
                    message: "schema".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INVALID_SCHEMA,
            ),
            (
                GraphError::InvalidRecord {
                    record_id: "record".to_owned(),
                    message: "record".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::InvalidQuery {
                    message: "query".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::InvalidDimension {
                    message: "dimension".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INVALID_DIMENSION,
            ),
            (
                GraphError::MissingEmbedding {
                    message: "embedding".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_MISSING_EMBEDDING,
            ),
            (
                GraphError::InvalidSnapshot {
                    message: "snapshot".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_CORRUPT_SNAPSHOT,
            ),
            (
                GraphError::StaleGeneration {
                    message: "stale".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_STALE_GENERATION,
            ),
            (
                GraphError::IncompatibleVersion {
                    message: "version".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INCOMPATIBLE_VERSION,
            ),
            (
                GraphError::GraphUnavailable {
                    message: "unavailable".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_GRAPH_UNAVAILABLE,
            ),
            (
                GraphError::WriterBusy {
                    path: "database".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_LOCK_UNAVAILABLE,
            ),
            (
                GraphError::MissingTarget {
                    relationship: "related_to".to_owned(),
                    source_record_id: "source".to_owned(),
                    target_record_id: "target".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::QueryLimitExceeded {
                    message: "limit".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED,
            ),
            (GraphError::Cancelled, RETRIEVALKIT_GRAPH_STATUS_CANCELLED),
            (
                GraphError::TimedOut {
                    message: "deadline".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_TIMED_OUT,
            ),
            (
                GraphError::Core {
                    message: "core".to_owned(),
                },
                RETRIEVALKIT_GRAPH_STATUS_INTERNAL,
            ),
        ];
        for (error, expected) in cases {
            let error = FfiError::from(error);
            assert_eq!(error.code, expected);
        }
    }

    #[test]
    fn capability_builders_separate_graph_and_retrieval_inputs() {
        let corpus = CString::new("capability-ffi").unwrap();
        let schema = GraphSchema::new(vec![RecordNodeSchema {
            record_type: RecordType::new("Item").unwrap(),
            node_type: NodeType::new("Item").unwrap(),
            queryable_fields: vec![],
        }]);
        let schema_json = CString::new(serde_json::to_vec(&schema).unwrap()).unwrap();
        let record = Record {
            id: RecordId::new("item-1").unwrap(),
            record_type: RecordType::new("Item").unwrap(),
            fields: BTreeMap::new(),
            content: None,
        };
        let mut status = RetrievalKitStatus {
            code: 0,
            message: std::ptr::null_mut(),
        };

        let graph_builder = unsafe {
            retrievalkit_graph_database_builder_new(
                corpus.as_ptr(),
                schema_json.as_ptr(),
                &mut status,
            )
        };
        assert!(!graph_builder.is_null());
        let graph_batch = CString::new(
            serde_json::to_vec(&serde_json::json!({
                "record": record,
                "projected_metadata": {},
                "chunks": [{"key": "body", "text": "graph-only", "metadata": {}}]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(unsafe {
            retrievalkit_graph_database_builder_upsert_record_json(
                graph_builder,
                graph_batch.as_ptr(),
                &mut status,
            )
        });
        let graph =
            unsafe { retrievalkit_graph_database_builder_build(graph_builder, &mut status) };
        assert!(!graph.is_null());
        unsafe { retrievalkit_graph_database_free(graph) };

        let retrieval_builder = unsafe {
            retrievalkit_graph_retrieval_builder_new(
                1,
                0,
                corpus.as_ptr(),
                schema_json.as_ptr(),
                &mut status,
            )
        };
        assert!(!retrieval_builder.is_null());
        let valid = CString::new(
            serde_json::to_vec(&serde_json::json!({
                "record": record,
                "projected_metadata": {},
                "chunks": [{"key": "body", "text": "semantic", "embedding": [1.0, 0.0], "metadata": {}}]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(unsafe {
            retrievalkit_graph_retrieval_builder_upsert_record_json(
                retrieval_builder,
                valid.as_ptr(),
                &mut status,
            )
        });

        let wrong_dimension = CString::new(
            serde_json::to_vec(&serde_json::json!({
                "record": record,
                "projected_metadata": {},
                "chunks": [{"key": "body", "text": "semantic", "embedding": [1.0], "metadata": {}}]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(!unsafe {
            retrievalkit_graph_retrieval_builder_upsert_record_json(
                retrieval_builder,
                wrong_dimension.as_ptr(),
                &mut status,
            )
        });
        assert_eq!(status.code, RETRIEVALKIT_GRAPH_STATUS_INVALID_DIMENSION);
        let database =
            unsafe { retrievalkit_graph_retrieval_builder_build(retrieval_builder, &mut status) };
        assert!(!database.is_null());
        let text = CString::new("semantic").unwrap();
        let embedding = [1.0_f32, 0.0];
        let mut results = empty_hybrid_result_buffer();
        assert!(unsafe {
            retrievalkit_graph_retrieval_hybrid_search_alpha(
                database,
                std::ptr::null(),
                text.as_ptr(),
                embedding.as_ptr(),
                embedding.len(),
                1,
                std::ptr::null(),
                RetrievalKitHybridQueryOptions {
                    vector_top_k: 1,
                    keyword_top_k: 1,
                    alpha: 0.6,
                },
                &mut results,
                &mut status,
            )
        });
        assert_eq!(status.code, super::super::RETRIEVALKIT_STATUS_OK);
        assert_eq!(results.count, 1);
        let hit = unsafe { &*results.hits };
        assert!(hit.has_record_id);
        assert_eq!(unsafe { result_string(&results, hit.document_id) }, "body");
        assert_eq!(unsafe { result_string(&results, hit.record_id) }, "item-1");
        assert_eq!(unsafe { result_string(&results, hit.text) }, "semantic");
        unsafe {
            super::super::retrievalkit_hybrid_results_free(results);
            retrievalkit_graph_retrieval_database_free(database);
            super::super::retrievalkit_status_clear(&mut status);
        }
    }

    unsafe fn result_string(
        buffer: &RetrievalKitHybridResultBuffer,
        range: super::super::RetrievalKitUtf8Range,
    ) -> &str {
        assert!(range.offset <= buffer.utf8_len);
        assert!(range.length <= buffer.utf8_len - range.offset);
        let bytes =
            unsafe { std::slice::from_raw_parts(buffer.utf8.add(range.offset), range.length) };
        std::str::from_utf8(bytes).unwrap()
    }

    #[test]
    fn candidate_projection_ffi_is_typed_filtered_ordered_and_generation_safe() {
        fn schema() -> GraphSchema {
            GraphSchema::new(vec![RecordNodeSchema {
                record_type: RecordType::new("Item").unwrap(),
                node_type: NodeType::new("Item").unwrap(),
                queryable_fields: vec![],
            }])
        }
        fn graph_database(corpus_id: &str) -> RetrievalKitGraphDatabase {
            let mut corpus = CorpusIndex::new(CorpusId::new(corpus_id).unwrap());
            for (record_id, team) in [("z-item", "platform"), ("a-item", "mobile")] {
                corpus
                    .upsert(CoreRecordInput {
                        record: Record {
                            id: RecordId::new(record_id).unwrap(),
                            record_type: RecordType::new("Item").unwrap(),
                            fields: BTreeMap::new(),
                            content: None,
                        },
                        metadata: BTreeMap::from([("team".to_owned(), team.into())]),
                        chunks: vec![CorpusChunkInput {
                            key: ChunkKey::new("body").unwrap(),
                            text: record_id.to_owned(),
                            metadata: BTreeMap::new(),
                        }],
                    })
                    .unwrap();
            }
            RetrievalKitGraphDatabase {
                database: GraphDatabase::build(corpus, schema()).unwrap(),
            }
        }
        fn selection(database: &RetrievalKitGraphDatabase) -> RetrievalKitGraphResult {
            let nodes = ["z-item", "a-item"]
                .into_iter()
                .map(|record_id| {
                    NodeId::record(
                        NodeType::new("Item").unwrap(),
                        RecordId::new(record_id).unwrap(),
                    )
                })
                .collect();
            RetrievalKitGraphResult {
                result: database
                    .database
                    .graph_query(&GraphQuery::new(Seed::NodeIds(nodes)), None)
                    .unwrap(),
            }
        }

        let database = graph_database("projection");
        let result = selection(&database);
        let filter = RetrievalKitFilter {
            filter: Filter::eq("team", "mobile"),
        };
        let mut status = RetrievalKitStatus {
            code: 0,
            message: std::ptr::null_mut(),
        };
        let mut output = RetrievalKitGraphCandidateProjection::default();
        assert!(unsafe {
            retrievalkit_graph_database_project_candidates(
                &database,
                &result,
                &filter,
                &mut output,
                &mut status,
            )
        });
        assert_eq!(output.source_nodes, 2);
        assert_eq!(output.projected_chunks_before_filter, 2);
        assert_eq!(output.projected_chunks_after_filter, 1);
        assert_eq!(output.count, 1);
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr((*output.candidates).record_id) }.to_bytes(),
            b"a-item"
        );
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr((*output.candidates).chunk_key) }.to_bytes(),
            b"body"
        );
        unsafe { retrievalkit_graph_candidate_projection_clear(&mut output) };
        assert!(output.candidates.is_null());
        assert_eq!(output.count, 0);
        unsafe { retrievalkit_graph_candidate_projection_clear(&mut output) };

        let foreign = graph_database("foreign");
        let foreign_result = selection(&foreign);
        assert!(!unsafe {
            retrievalkit_graph_database_project_candidates(
                &database,
                &foreign_result,
                std::ptr::null(),
                &mut output,
                &mut status,
            )
        });
        assert_eq!(status.code, RETRIEVALKIT_GRAPH_STATUS_STALE_GENERATION);
        assert!(output.candidates.is_null());
        assert!(!unsafe {
            retrievalkit_graph_database_project_candidates(
                &database,
                &result,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut status,
            )
        });
        assert_eq!(
            status.code,
            super::super::RETRIEVALKIT_STATUS_INVALID_ARGUMENT
        );

        let mut retrieval = RetrievalDatabase::new(
            RetrievalConfiguration::semantic(IndexConfig::new(1, VectorMetric::DotProduct)),
            CorpusId::new("retrieval-projection").unwrap(),
        )
        .unwrap();
        retrieval
            .upsert_record(
                Record {
                    id: RecordId::new("item").unwrap(),
                    record_type: RecordType::new("Item").unwrap(),
                    fields: BTreeMap::new(),
                    content: None,
                },
                BTreeMap::new(),
                vec![RecordChunkInput {
                    key: ChunkKey::new("body").unwrap(),
                    text: "item".to_owned(),
                    embedding: vec![1.0],
                    metadata: BTreeMap::new(),
                }],
            )
            .unwrap();
        let retrieval = RetrievalKitGraphRetrievalDatabase {
            database: GraphRetrievalDatabase::build(retrieval, schema()).unwrap(),
        };
        let retrieval_result = RetrievalKitGraphResult {
            result: retrieval
                .database
                .graph_query(
                    &GraphQuery::new(Seed::NodeIds(vec![NodeId::record(
                        NodeType::new("Item").unwrap(),
                        RecordId::new("item").unwrap(),
                    )])),
                    None,
                )
                .unwrap(),
        };
        assert!(unsafe {
            retrievalkit_graph_retrieval_database_project_candidates(
                &retrieval,
                &retrieval_result,
                std::ptr::null(),
                &mut output,
                &mut status,
            )
        });
        assert_eq!(output.count, 1);
        unsafe {
            retrievalkit_graph_candidate_projection_free(output);
            super::super::retrievalkit_status_clear(&mut status);
        }
    }

    #[test]
    fn builder_finalization_and_composite_persistence_round_trip() {
        let corpus = CString::new("swift-generic").unwrap();
        let mut status = RetrievalKitStatus {
            code: 0,
            message: std::ptr::null_mut(),
        };
        let builder =
            unsafe { retrievalkit_graph_builder_new(2, 1, 0, corpus.as_ptr(), &mut status) };
        assert!(!builder.is_null());

        let record = Record {
            id: RecordId::new("item-1").unwrap(),
            record_type: RecordType::new("Item").unwrap(),
            fields: BTreeMap::from([(
                FieldName::new("name").unwrap(),
                RecordValue::String("Generic item".to_owned()),
            )]),
            content: None,
        };
        let batch = serde_json::json!({
            "record": record,
            "projected_metadata": {},
            "chunks": [{
                "key": "body",
                "text": "generic searchable item",
                "embedding": [1.0, 0.0],
                "metadata": {}
            }]
        });
        let batch = CString::new(serde_json::to_vec(&batch).unwrap()).unwrap();
        assert!(unsafe {
            retrievalkit_graph_builder_upsert_record_json(builder, batch.as_ptr(), &mut status)
        });

        let schema = GraphSchema::new(vec![RecordNodeSchema {
            record_type: RecordType::new("Item").unwrap(),
            node_type: NodeType::new("Item").unwrap(),
            queryable_fields: vec![FieldPath::single(FieldName::new("name").unwrap())],
        }])
        .with_chunk_nodes(ChunkNodeSchema {
            node_type: NodeType::new("Chunk").unwrap(),
            owns_relationship: RelationshipType::new("owns").unwrap(),
            inverse_relationship: Some(RelationshipType::new("owned_by").unwrap()),
        });
        let schema = CString::new(serde_json::to_vec(&schema).unwrap()).unwrap();
        let index =
            unsafe { retrievalkit_graph_builder_build_json(builder, schema.as_ptr(), &mut status) };
        assert!(!index.is_null());

        let node_type = CString::new("Item").unwrap();
        let record_id = CString::new("item-1").unwrap();
        let nodes = [RetrievalKitGraphNodeRef {
            node_type: node_type.as_ptr(),
            source_type: 0,
            record_id: record_id.as_ptr(),
            chunk_key: std::ptr::null(),
        }];
        let query = RetrievalKitGraphQuery {
            seed_type: 0,
            node_ids: nodes.as_ptr(),
            node_id_count: 1,
            seed_node_type: std::ptr::null(),
            field_segments: std::ptr::null(),
            field_segment_count: 0,
            values: std::ptr::null(),
            value_count: 0,
            steps: std::ptr::null(),
            step_count: 0,
            limits: RetrievalKitGraphLimits {
                max_hops: 8,
                max_visited: 100,
                max_results: 10,
                max_working_bytes: 1024 * 1024,
            },
        };
        let result =
            unsafe { retrievalkit_graph_query(index, query, std::ptr::null(), &mut status) };
        assert!(!result.is_null());
        assert_eq!(unsafe { retrievalkit_graph_result_count(result) }, 1);
        let mut matched = RetrievalKitGraphMatch {
            node_type: std::ptr::null_mut(),
            source_type: 0,
            record_id: std::ptr::null_mut(),
            chunk_key: std::ptr::null_mut(),
            depth: 0,
            path_length: 0,
        };
        assert!(unsafe { retrievalkit_graph_result_match(result, 0, &mut matched, &mut status) });
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(matched.record_id) }
                .to_str()
                .unwrap(),
            "item-1"
        );
        assert_eq!(
            unsafe { retrievalkit_graph_result_trace(result) }.result_count,
            1
        );
        let scope = unsafe { retrievalkit_graph_result_project(index, result, &mut status) };
        assert!(!scope.is_null());
        assert_eq!(
            unsafe { retrievalkit_graph_scope_resolved_chunks(scope) },
            1
        );
        let embedding = [1.0_f32, 0.0];
        let mut exact = empty_search_result_buffer();
        assert!(unsafe {
            retrievalkit_graph_scope_search(
                index,
                scope,
                embedding.as_ptr(),
                2,
                10,
                std::ptr::null(),
                &mut exact,
                &mut status,
            )
        });
        assert_eq!(exact.count, 1);
        let text = CString::new("generic").unwrap();
        let mut keyword = empty_keyword_result_buffer();
        assert!(unsafe {
            retrievalkit_graph_scope_keyword_search(
                index,
                scope,
                text.as_ptr(),
                10,
                std::ptr::null(),
                &mut keyword,
                &mut status,
            )
        });
        assert_eq!(keyword.count, 1);
        unsafe {
            super::super::retrievalkit_search_results_free(exact);
            super::super::retrievalkit_keyword_results_free(keyword);
            retrievalkit_graph_scope_free(scope)
        };

        let name = CString::new("name").unwrap();
        let fields = [name.as_ptr()];
        let generic_item = CString::new("Generic item").unwrap();
        let values = [RetrievalKitGraphScalar {
            value_type: 0,
            string_value: generic_item.as_ptr(),
            integer_value: 0,
            bool_value: false,
        }];
        let equality_query = RetrievalKitGraphQuery {
            seed_type: 1,
            node_ids: std::ptr::null(),
            node_id_count: 0,
            seed_node_type: node_type.as_ptr(),
            field_segments: fields.as_ptr(),
            field_segment_count: fields.len(),
            values: values.as_ptr(),
            value_count: values.len(),
            ..query
        };
        let equality_result = unsafe {
            retrievalkit_graph_query(index, equality_query, std::ptr::null(), &mut status)
        };
        assert!(!equality_result.is_null());
        assert_eq!(
            unsafe { retrievalkit_graph_result_count(equality_result) },
            1
        );
        unsafe { retrievalkit_graph_result_free(equality_result) };

        let owns = CString::new("owns").unwrap();
        let steps = [RetrievalKitGraphStep {
            relationship: owns.as_ptr(),
            direction: 0,
            min_hops: 1,
            max_hops: 1,
        }];
        let traversal_query = RetrievalKitGraphQuery {
            steps: steps.as_ptr(),
            step_count: steps.len(),
            ..query
        };
        let traversal_result = unsafe {
            retrievalkit_graph_query(index, traversal_query, std::ptr::null(), &mut status)
        };
        assert!(!traversal_result.is_null());
        assert_eq!(
            unsafe { retrievalkit_graph_result_count(traversal_result) },
            1
        );
        let mut edge = RetrievalKitGraphPathEdge {
            relationship_type: std::ptr::null_mut(),
            source: RetrievalKitGraphOwnedNode {
                node_type: std::ptr::null_mut(),
                source_type: 0,
                record_id: std::ptr::null_mut(),
                chunk_key: std::ptr::null_mut(),
            },
            target: RetrievalKitGraphOwnedNode {
                node_type: std::ptr::null_mut(),
                source_type: 0,
                record_id: std::ptr::null_mut(),
                chunk_key: std::ptr::null_mut(),
            },
            occurrence_ordinal: 0,
            schema_rule_index: 0,
            source_record_id: std::ptr::null_mut(),
            source_field_segments: super::super::RetrievalKitStringArray {
                values: std::ptr::null_mut(),
                count: 0,
            },
            derived_inverse: false,
            built_in: false,
        };
        assert!(unsafe {
            retrievalkit_graph_result_path_edge(traversal_result, 0, 0, &mut edge, &mut status)
        });
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(edge.relationship_type) }.to_bytes(),
            b"owns"
        );
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(edge.source.record_id) }.to_bytes(),
            b"item-1"
        );
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(edge.target.chunk_key) }.to_bytes(),
            b"body"
        );
        assert!(edge.built_in);
        assert_eq!(edge.source_field_segments.count, 0);
        unsafe {
            retrievalkit_graph_path_edge_clear(&mut edge);
            retrievalkit_graph_result_free(traversal_result);
        }
        unsafe {
            retrievalkit_graph_match_clear(&mut matched);
            retrievalkit_graph_result_free(result)
        };

        let cancellation = retrievalkit_graph_cancellation_new();
        unsafe { retrievalkit_graph_cancellation_cancel(cancellation) };
        assert!(
            unsafe { retrievalkit_graph_query(index, query, cancellation, &mut status) }.is_null()
        );
        assert_eq!(status.code, RETRIEVALKIT_GRAPH_STATUS_CANCELLED);
        unsafe {
            retrievalkit_graph_cancellation_free(cancellation);
            super::super::retrievalkit_status_clear(&mut status)
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "retrievalkit-graph-ffi-{}-{nonce}",
            std::process::id()
        ));
        let path = CString::new(directory.to_string_lossy().as_bytes()).unwrap();
        assert!(unsafe { retrievalkit_graph_index_save(index, path.as_ptr(), &mut status) });
        assert!(unsafe { retrievalkit_graph_index_validate(path.as_ptr(), &mut status) });
        let loaded = unsafe { retrievalkit_graph_index_load(path.as_ptr(), &mut status) };
        assert!(!loaded.is_null());

        unsafe {
            retrievalkit_graph_index_free(loaded);
            retrievalkit_graph_index_free(index);
            super::super::retrievalkit_status_clear(&mut status);
        }
        let _ = std::fs::remove_dir_all(directory);
    }
}
