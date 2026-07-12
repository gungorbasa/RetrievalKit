use std::ffi::c_char;
use std::path::Path;
use std::slice;

use serde::Deserialize;
use vectorkit_core::{
    CandidateScope, ChunkKey, CorpusId, ExactVectorIndex, HybridHit, HybridQuery, IndexConfig,
    KeywordHit, KeywordQuery, Metadata, Record, RecordChunkInput, SearchHit, SearchQuery,
};
use vectorkit_graph::{
    CancellationToken, Direction, GraphIndex, GraphQuery, GraphResult, GraphScalar, NodeId,
    NodeSource, QueryLimits, Seed, Traverse,
};
use vectorkit_graph::{FieldPath, GraphSchema, NodeType, RelationshipType};

use super::{
    ffi_bool, ffi_ptr, optional_filter, parse_encoding_code, parse_hybrid_fusion, parse_metric,
    read_c_string, read_f32_slice, string_array, string_to_owned_ptr, FfiError, VkFilter,
    VkHybridHit, VkHybridOptions, VkHybridResultBuffer, VkKeywordHit, VkKeywordResultBuffer,
    VkSearchHit, VkSearchResultBuffer, VkStatus,
};

const VK_GRAPH_STATUS_INVALID_SCHEMA: i32 = 100;
const VK_GRAPH_STATUS_INVALID_IDENTITY: i32 = 101;
const VK_GRAPH_STATUS_STALE_GENERATION: i32 = 102;
const VK_GRAPH_STATUS_INCOMPATIBLE_VERSION: i32 = 103;
const VK_GRAPH_STATUS_GRAPH_UNAVAILABLE: i32 = 104;
const VK_GRAPH_STATUS_CORRUPT_SNAPSHOT: i32 = 105;
const VK_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED: i32 = 106;
const VK_GRAPH_STATUS_CANCELLED: i32 = 107;
const VK_GRAPH_STATUS_TIMED_OUT: i32 = 108;
const VK_GRAPH_STATUS_LOCK_UNAVAILABLE: i32 = 109;
const VK_GRAPH_STATUS_INTERNAL: i32 = 110;

pub struct VkGraphBuilder {
    core: ExactVectorIndex,
}

pub struct VkGraphIndex {
    index: GraphIndex,
}

pub struct VkGraphResult {
    result: GraphResult,
}
pub struct VkGraphCancellation {
    token: CancellationToken,
}
pub struct VkGraphScope {
    scope: CandidateScope,
    pub source_nodes: usize,
    pub resolved_chunks: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkGraphNodeRef {
    pub node_type: *const c_char,
    pub source_type: u32,
    pub record_id: *const c_char,
    pub chunk_key: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkGraphScalar {
    pub value_type: u32,
    pub string_value: *const c_char,
    pub integer_value: i64,
    pub bool_value: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkGraphStep {
    pub relationship: *const c_char,
    pub direction: u32,
    pub min_hops: usize,
    pub max_hops: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkGraphLimits {
    pub max_hops: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_working_bytes: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkGraphQuery {
    pub seed_type: u32,
    pub node_ids: *const VkGraphNodeRef,
    pub node_id_count: usize,
    pub seed_node_type: *const c_char,
    pub field_segments: *const *const c_char,
    pub field_segment_count: usize,
    pub values: *const VkGraphScalar,
    pub value_count: usize,
    pub steps: *const VkGraphStep,
    pub step_count: usize,
    pub limits: VkGraphLimits,
}
#[repr(C)]
pub struct VkGraphMatch {
    pub node_type: *mut c_char,
    pub source_type: u32,
    pub record_id: *mut c_char,
    pub chunk_key: *mut c_char,
    pub depth: usize,
    pub path_length: usize,
}
#[repr(C)]
pub struct VkGraphOwnedNode {
    pub node_type: *mut c_char,
    pub source_type: u32,
    pub record_id: *mut c_char,
    pub chunk_key: *mut c_char,
}
#[repr(C)]
pub struct VkGraphPathEdge {
    pub relationship_type: *mut c_char,
    pub source: VkGraphOwnedNode,
    pub target: VkGraphOwnedNode,
    pub occurrence_ordinal: u32,
    pub schema_rule_index: u32,
    pub source_record_id: *mut c_char,
    pub source_field_segments: super::VkStringArray,
    pub derived_inverse: bool,
    pub built_in: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VkGraphTrace {
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

#[no_mangle]
pub extern "C" fn vectorkit_graph_ffi_abi_version() -> u32 {
    3
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_new(
    dimension: usize,
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphBuilder {
    ffi_ptr(status, || {
        let config = IndexConfig::new(dimension, parse_metric(metric)?)
            .with_vector_encoding(parse_encoding_code(encoding)?);
        let corpus = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let core = ExactVectorIndex::try_with_config_in_corpus(config, corpus)?;
        Ok(Box::into_raw(Box::new(VkGraphBuilder { core })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_upsert_record_json(
    builder: *mut VkGraphBuilder,
    record_json: *const c_char,
    status: *mut VkStatus,
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
pub unsafe extern "C" fn vectorkit_graph_builder_build_json(
    builder: *mut VkGraphBuilder,
    schema_json: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphIndex {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument("graph builder must not be null"));
        }
        let builder = unsafe { Box::from_raw(builder) };
        let json = unsafe { read_c_string(schema_json, "schema_json") }?;
        let schema: GraphSchema = serde_json::from_str(&json)
            .map_err(|error| FfiError::invalid_argument(format!("invalid schema JSON: {error}")))?;
        let index = GraphIndex::build(builder.core, schema)?;
        Ok(Box::into_raw(Box::new(VkGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_builder_free(builder: *mut VkGraphBuilder) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_load(
    directory: *const c_char,
    status: *mut VkStatus,
) -> *mut VkGraphIndex {
    ffi_ptr(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        let index = GraphIndex::load_from_dir(Path::new(&path))?;
        Ok(Box::into_raw(Box::new(VkGraphIndex { index })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_save(
    index: *const VkGraphIndex,
    directory: *const c_char,
    status: *mut VkStatus,
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
pub unsafe extern "C" fn vectorkit_graph_index_validate(
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let path = unsafe { read_c_string(directory, "directory") }?;
        GraphIndex::validate_dir(Path::new(&path))?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_index_free(index: *mut VkGraphIndex) {
    if !index.is_null() {
        drop(unsafe { Box::from_raw(index) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_query(
    index: *const VkGraphIndex,
    query: VkGraphQuery,
    cancellation: *const VkGraphCancellation,
    status: *mut VkStatus,
) -> *mut VkGraphResult {
    ffi_ptr(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let query = unsafe { decode_query(query) }?;
        let cancellation = unsafe { cancellation.as_ref() }.map(|value| &value.token);
        let result = index.index.graph_query(&query, cancellation)?;
        Ok(Box::into_raw(Box::new(VkGraphResult { result })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_result_count(result: *const VkGraphResult) -> usize {
    unsafe { result.as_ref() }.map_or(0, |value| value.result.matches.len())
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_result_match(
    result: *const VkGraphResult,
    index: usize,
    out_match: *mut VkGraphMatch,
    status: *mut VkStatus,
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
            *out_match = VkGraphMatch {
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
pub unsafe extern "C" fn vectorkit_graph_result_path_edge(
    result: *const VkGraphResult,
    match_index: usize,
    edge_index: usize,
    out_edge: *mut VkGraphPathEdge,
    status: *mut VkStatus,
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
            *out_edge = VkGraphPathEdge {
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
pub unsafe extern "C" fn vectorkit_graph_path_edge_clear(value: *mut VkGraphPathEdge) {
    if let Some(value) = unsafe { value.as_mut() } {
        unsafe {
            super::vectorkit_string_free(value.relationship_type);
            owned_node_clear(&mut value.source);
            owned_node_clear(&mut value.target);
            super::vectorkit_string_free(value.source_record_id);
            super::string_array_free(value.source_field_segments);
        }
        value.relationship_type = std::ptr::null_mut();
        value.source_record_id = std::ptr::null_mut();
        value.source_field_segments = super::VkStringArray {
            values: std::ptr::null_mut(),
            count: 0,
        };
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_match_clear(value: *mut VkGraphMatch) {
    if let Some(value) = unsafe { value.as_mut() } {
        unsafe {
            super::vectorkit_string_free(value.node_type);
            super::vectorkit_string_free(value.record_id);
            super::vectorkit_string_free(value.chunk_key)
        };
        value.node_type = std::ptr::null_mut();
        value.record_id = std::ptr::null_mut();
        value.chunk_key = std::ptr::null_mut();
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_result_trace(
    result: *const VkGraphResult,
) -> VkGraphTrace {
    let Some(result) = (unsafe { result.as_ref() }) else {
        return VkGraphTrace::default();
    };
    let trace = &result.result.trace;
    VkGraphTrace {
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
pub unsafe extern "C" fn vectorkit_graph_result_free(result: *mut VkGraphResult) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result) });
    }
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_result_project(
    index: *const VkGraphIndex,
    result: *const VkGraphResult,
    status: *mut VkStatus,
) -> *mut VkGraphScope {
    ffi_ptr(status, || {
        let index = unsafe { index.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))?;
        let result = unsafe { result.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("graph result must not be null"))?;
        let projected = index.index.project_candidates(&result.result)?;
        Ok(Box::into_raw(Box::new(VkGraphScope {
            scope: projected.scope,
            source_nodes: projected.trace.source_nodes,
            resolved_chunks: projected.trace.resolved_chunks,
        })))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_free(scope: *mut VkGraphScope) {
    if !scope.is_null() {
        drop(unsafe { Box::from_raw(scope) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_source_nodes(scope: *const VkGraphScope) -> usize {
    unsafe { scope.as_ref() }.map_or(0, |value| value.source_nodes)
}
#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_resolved_chunks(
    scope: *const VkGraphScope,
) -> usize {
    unsafe { scope.as_ref() }.map_or(0, |value| value.resolved_chunks)
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_search(
    index: *const VkGraphIndex,
    scope: *const VkGraphScope,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const VkFilter,
    out_results: *mut VkSearchResultBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
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
            )
        };
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_keyword_search(
    index: *const VkGraphIndex,
    scope: *const VkGraphScope,
    text: *const c_char,
    top_k: usize,
    filter: *const VkFilter,
    out_results: *mut VkKeywordResultBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
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
            )
        };
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_scope_hybrid_search(
    index: *const VkGraphIndex,
    scope: *const VkGraphScope,
    text: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    top_k: usize,
    filter: *const VkFilter,
    options: VkHybridOptions,
    out_results: *mut VkHybridResultBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        let index = unsafe { graph_index(index) }?;
        let scope = unsafe { graph_scope(scope) }?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        query.fusion = parse_hybrid_fusion(options)?;
        unsafe {
            *out_results = graph_hybrid_buffer(
                index,
                index
                    .index
                    .hybrid_search_in_candidates(&query, &scope.scope)?,
            )
        };
        Ok(())
    })
}
#[no_mangle]
pub extern "C" fn vectorkit_graph_cancellation_new() -> *mut VkGraphCancellation {
    Box::into_raw(Box::new(VkGraphCancellation {
        token: CancellationToken::default(),
    }))
}
#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_cancellation_cancel(value: *const VkGraphCancellation) {
    if let Some(value) = unsafe { value.as_ref() } {
        value.token.cancel();
    }
}
#[no_mangle]
pub unsafe extern "C" fn vectorkit_graph_cancellation_free(value: *mut VkGraphCancellation) {
    if !value.is_null() {
        drop(unsafe { Box::from_raw(value) });
    }
}

unsafe fn decode_query(value: VkGraphQuery) -> Result<GraphQuery, FfiError> {
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
                    .and_then(|s| vectorkit_core::FieldName::new(s).map_err(FfiError::from))
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

fn owned_node(node: &NodeId) -> VkGraphOwnedNode {
    let (source_type, record_id, chunk_key) = match &node.source {
        NodeSource::Record(id) => (0, string_to_owned_ptr(id.as_str()), std::ptr::null_mut()),
        NodeSource::Chunk(identity) => (
            1,
            string_to_owned_ptr(identity.record_id.as_str()),
            string_to_owned_ptr(identity.chunk_key.as_str()),
        ),
    };
    VkGraphOwnedNode {
        node_type: string_to_owned_ptr(node.node_type.as_str()),
        source_type,
        record_id,
        chunk_key,
    }
}

unsafe fn owned_node_clear(node: &mut VkGraphOwnedNode) {
    unsafe {
        super::vectorkit_string_free(node.node_type);
        super::vectorkit_string_free(node.record_id);
        super::vectorkit_string_free(node.chunk_key);
    }
    node.node_type = std::ptr::null_mut();
    node.record_id = std::ptr::null_mut();
    node.chunk_key = std::ptr::null_mut();
}

unsafe fn decode_node(value: VkGraphNodeRef) -> Result<NodeId, FfiError> {
    let node_type = NodeType::new(unsafe { read_c_string(value.node_type, "node_type") }?)
        .map_err(FfiError::from)?;
    let record =
        vectorkit_core::RecordId::new(unsafe { read_c_string(value.record_id, "record_id") }?)?;
    match value.source_type {
        0 => Ok(NodeId::record(node_type, record)),
        1 => Ok(NodeId::chunk(
            node_type,
            vectorkit_core::ChunkIdentity::new(
                record,
                ChunkKey::new(unsafe { read_c_string(value.chunk_key, "chunk_key") }?)?,
            ),
        )),
        other => Err(FfiError::invalid_argument(format!(
            "unsupported node source type {other}"
        ))),
    }
}
unsafe fn decode_scalar(value: VkGraphScalar) -> Result<GraphScalar, FfiError> {
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

unsafe fn graph_index<'a>(value: *const VkGraphIndex) -> Result<&'a VkGraphIndex, FfiError> {
    unsafe { value.as_ref() }
        .ok_or_else(|| FfiError::invalid_argument("graph index must not be null"))
}
unsafe fn graph_scope<'a>(value: *const VkGraphScope) -> Result<&'a VkGraphScope, FfiError> {
    unsafe { value.as_ref() }
        .ok_or_else(|| FfiError::invalid_argument("graph scope must not be null"))
}
fn boxed_buffer<T>(values: Vec<T>) -> (*mut T, usize) {
    let mut values = values.into_boxed_slice();
    let result = (values.as_mut_ptr(), values.len());
    std::mem::forget(values);
    result
}
fn graph_search_buffer(index: &VkGraphIndex, hits: Vec<SearchHit>) -> VkSearchResultBuffer {
    let hits = hits
        .into_iter()
        .map(|hit| VkSearchHit {
            chunk_id: hit.chunk_id,
            document_id: string_to_owned_ptr(&hit.document_id),
            text: string_to_owned_ptr(index.index.chunk_text(hit.chunk_id).unwrap_or("")),
            score: hit.score,
            vector_score: hit.trace.vector_score,
            filter_matched: hit.trace.filter_matched,
        })
        .collect();
    let (hits, count) = boxed_buffer(hits);
    VkSearchResultBuffer { hits, count }
}
fn graph_keyword_buffer(index: &VkGraphIndex, hits: Vec<KeywordHit>) -> VkKeywordResultBuffer {
    let hits = hits
        .into_iter()
        .map(|hit| VkKeywordHit {
            chunk_id: hit.chunk_id,
            document_id: string_to_owned_ptr(&hit.document_id),
            text: string_to_owned_ptr(index.index.chunk_text(hit.chunk_id).unwrap_or("")),
            score: hit.score,
            matched_terms: string_array(hit.matched_terms),
        })
        .collect();
    let (hits, count) = boxed_buffer(hits);
    VkKeywordResultBuffer { hits, count }
}
fn graph_hybrid_buffer(index: &VkGraphIndex, hits: Vec<HybridHit>) -> VkHybridResultBuffer {
    let hits = hits
        .into_iter()
        .map(|hit| VkHybridHit {
            chunk_id: hit.chunk_id,
            document_id: string_to_owned_ptr(&hit.document_id),
            text: string_to_owned_ptr(index.index.chunk_text(hit.chunk_id).unwrap_or("")),
            score: hit.score,
            has_vector_score: hit.vector_score.is_some(),
            vector_score: hit.vector_score.unwrap_or_default(),
            has_keyword_score: hit.keyword_score.is_some(),
            keyword_score: hit.keyword_score.unwrap_or_default(),
            has_vector_rank: hit.trace.vector_rank.is_some(),
            vector_rank: hit.trace.vector_rank.unwrap_or_default(),
            has_keyword_rank: hit.trace.keyword_rank.is_some(),
            keyword_rank: hit.trace.keyword_rank.unwrap_or_default(),
            has_normalized_vector_score: hit.trace.normalized_vector_score.is_some(),
            normalized_vector_score: hit.trace.normalized_vector_score.unwrap_or_default(),
            has_normalized_keyword_score: hit.trace.normalized_keyword_score.is_some(),
            normalized_keyword_score: hit.trace.normalized_keyword_score.unwrap_or_default(),
            matched_terms: string_array(hit.trace.matched_terms),
            filter_matched: hit.trace.filter_matched,
        })
        .collect();
    let (hits, count) = boxed_buffer(hits);
    VkHybridResultBuffer { hits, count }
}

impl From<vectorkit_graph::GraphError> for FfiError {
    fn from(error: vectorkit_graph::GraphError) -> Self {
        let code = match &error {
            vectorkit_graph::GraphError::InvalidSchema { .. } => VK_GRAPH_STATUS_INVALID_SCHEMA,
            vectorkit_graph::GraphError::InvalidRecord { .. }
            | vectorkit_graph::GraphError::InvalidQuery { .. }
            | vectorkit_graph::GraphError::MissingTarget { .. } => VK_GRAPH_STATUS_INVALID_IDENTITY,
            vectorkit_graph::GraphError::InvalidSnapshot { .. } => VK_GRAPH_STATUS_CORRUPT_SNAPSHOT,
            vectorkit_graph::GraphError::StaleGeneration { .. } => VK_GRAPH_STATUS_STALE_GENERATION,
            vectorkit_graph::GraphError::IncompatibleVersion { .. } => {
                VK_GRAPH_STATUS_INCOMPATIBLE_VERSION
            }
            vectorkit_graph::GraphError::GraphUnavailable { .. } => {
                VK_GRAPH_STATUS_GRAPH_UNAVAILABLE
            }
            vectorkit_graph::GraphError::WriterBusy { .. } => VK_GRAPH_STATUS_LOCK_UNAVAILABLE,
            vectorkit_graph::GraphError::QueryLimitExceeded { .. } => {
                VK_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED
            }
            vectorkit_graph::GraphError::Cancelled => VK_GRAPH_STATUS_CANCELLED,
            vectorkit_graph::GraphError::TimedOut { .. } => VK_GRAPH_STATUS_TIMED_OUT,
            vectorkit_graph::GraphError::Core { .. } => VK_GRAPH_STATUS_INTERNAL,
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

    use vectorkit_core::{FieldName, RecordId, RecordType, RecordValue};
    use vectorkit_graph::{
        ChunkNodeSchema, GraphSchema, NodeType, RecordNodeSchema, RelationshipType,
    };

    use super::*;

    #[test]
    fn graph_errors_map_to_the_stable_public_status_taxonomy() {
        use vectorkit_graph::GraphError;

        let cases = [
            (
                GraphError::InvalidSchema {
                    message: "schema".to_owned(),
                },
                VK_GRAPH_STATUS_INVALID_SCHEMA,
            ),
            (
                GraphError::InvalidRecord {
                    record_id: "record".to_owned(),
                    message: "record".to_owned(),
                },
                VK_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::InvalidQuery {
                    message: "query".to_owned(),
                },
                VK_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::InvalidSnapshot {
                    message: "snapshot".to_owned(),
                },
                VK_GRAPH_STATUS_CORRUPT_SNAPSHOT,
            ),
            (
                GraphError::StaleGeneration {
                    message: "stale".to_owned(),
                },
                VK_GRAPH_STATUS_STALE_GENERATION,
            ),
            (
                GraphError::IncompatibleVersion {
                    message: "version".to_owned(),
                },
                VK_GRAPH_STATUS_INCOMPATIBLE_VERSION,
            ),
            (
                GraphError::GraphUnavailable {
                    message: "unavailable".to_owned(),
                },
                VK_GRAPH_STATUS_GRAPH_UNAVAILABLE,
            ),
            (
                GraphError::WriterBusy {
                    path: "database".to_owned(),
                },
                VK_GRAPH_STATUS_LOCK_UNAVAILABLE,
            ),
            (
                GraphError::MissingTarget {
                    relationship: "related_to".to_owned(),
                    source_record_id: "source".to_owned(),
                    target_record_id: "target".to_owned(),
                },
                VK_GRAPH_STATUS_INVALID_IDENTITY,
            ),
            (
                GraphError::QueryLimitExceeded {
                    message: "limit".to_owned(),
                },
                VK_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED,
            ),
            (GraphError::Cancelled, VK_GRAPH_STATUS_CANCELLED),
            (
                GraphError::TimedOut {
                    message: "deadline".to_owned(),
                },
                VK_GRAPH_STATUS_TIMED_OUT,
            ),
            (
                GraphError::Core {
                    message: "core".to_owned(),
                },
                VK_GRAPH_STATUS_INTERNAL,
            ),
        ];
        for (error, expected) in cases {
            let error = FfiError::from(error);
            assert_eq!(error.code, expected);
        }
    }

    #[test]
    fn builder_finalization_and_composite_persistence_round_trip() {
        let corpus = CString::new("swift-generic").unwrap();
        let mut status = VkStatus {
            code: 0,
            message: std::ptr::null_mut(),
        };
        let builder = unsafe { vectorkit_graph_builder_new(2, 1, 0, corpus.as_ptr(), &mut status) };
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
            vectorkit_graph_builder_upsert_record_json(builder, batch.as_ptr(), &mut status)
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
            unsafe { vectorkit_graph_builder_build_json(builder, schema.as_ptr(), &mut status) };
        assert!(!index.is_null());

        let node_type = CString::new("Item").unwrap();
        let record_id = CString::new("item-1").unwrap();
        let nodes = [VkGraphNodeRef {
            node_type: node_type.as_ptr(),
            source_type: 0,
            record_id: record_id.as_ptr(),
            chunk_key: std::ptr::null(),
        }];
        let query = VkGraphQuery {
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
            limits: VkGraphLimits {
                max_hops: 8,
                max_visited: 100,
                max_results: 10,
                max_working_bytes: 1024 * 1024,
            },
        };
        let result = unsafe { vectorkit_graph_query(index, query, std::ptr::null(), &mut status) };
        assert!(!result.is_null());
        assert_eq!(unsafe { vectorkit_graph_result_count(result) }, 1);
        let mut matched = VkGraphMatch {
            node_type: std::ptr::null_mut(),
            source_type: 0,
            record_id: std::ptr::null_mut(),
            chunk_key: std::ptr::null_mut(),
            depth: 0,
            path_length: 0,
        };
        assert!(unsafe { vectorkit_graph_result_match(result, 0, &mut matched, &mut status) });
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(matched.record_id) }
                .to_str()
                .unwrap(),
            "item-1"
        );
        assert_eq!(
            unsafe { vectorkit_graph_result_trace(result) }.result_count,
            1
        );
        let scope = unsafe { vectorkit_graph_result_project(index, result, &mut status) };
        assert!(!scope.is_null());
        assert_eq!(unsafe { vectorkit_graph_scope_resolved_chunks(scope) }, 1);
        let embedding = [1.0_f32, 0.0];
        let mut exact = VkSearchResultBuffer {
            hits: std::ptr::null_mut(),
            count: 0,
        };
        assert!(unsafe {
            vectorkit_graph_scope_search(
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
        let mut keyword = VkKeywordResultBuffer {
            hits: std::ptr::null_mut(),
            count: 0,
        };
        assert!(unsafe {
            vectorkit_graph_scope_keyword_search(
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
            super::super::vectorkit_search_results_free(exact);
            super::super::vectorkit_keyword_results_free(keyword);
            vectorkit_graph_scope_free(scope)
        };

        let name = CString::new("name").unwrap();
        let fields = [name.as_ptr()];
        let generic_item = CString::new("Generic item").unwrap();
        let values = [VkGraphScalar {
            value_type: 0,
            string_value: generic_item.as_ptr(),
            integer_value: 0,
            bool_value: false,
        }];
        let equality_query = VkGraphQuery {
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
        let equality_result =
            unsafe { vectorkit_graph_query(index, equality_query, std::ptr::null(), &mut status) };
        assert!(!equality_result.is_null());
        assert_eq!(unsafe { vectorkit_graph_result_count(equality_result) }, 1);
        unsafe { vectorkit_graph_result_free(equality_result) };

        let owns = CString::new("owns").unwrap();
        let steps = [VkGraphStep {
            relationship: owns.as_ptr(),
            direction: 0,
            min_hops: 1,
            max_hops: 1,
        }];
        let traversal_query = VkGraphQuery {
            steps: steps.as_ptr(),
            step_count: steps.len(),
            ..query
        };
        let traversal_result =
            unsafe { vectorkit_graph_query(index, traversal_query, std::ptr::null(), &mut status) };
        assert!(!traversal_result.is_null());
        assert_eq!(unsafe { vectorkit_graph_result_count(traversal_result) }, 1);
        let mut edge = VkGraphPathEdge {
            relationship_type: std::ptr::null_mut(),
            source: VkGraphOwnedNode {
                node_type: std::ptr::null_mut(),
                source_type: 0,
                record_id: std::ptr::null_mut(),
                chunk_key: std::ptr::null_mut(),
            },
            target: VkGraphOwnedNode {
                node_type: std::ptr::null_mut(),
                source_type: 0,
                record_id: std::ptr::null_mut(),
                chunk_key: std::ptr::null_mut(),
            },
            occurrence_ordinal: 0,
            schema_rule_index: 0,
            source_record_id: std::ptr::null_mut(),
            source_field_segments: super::super::VkStringArray {
                values: std::ptr::null_mut(),
                count: 0,
            },
            derived_inverse: false,
            built_in: false,
        };
        assert!(unsafe {
            vectorkit_graph_result_path_edge(traversal_result, 0, 0, &mut edge, &mut status)
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
            vectorkit_graph_path_edge_clear(&mut edge);
            vectorkit_graph_result_free(traversal_result);
        }
        unsafe {
            vectorkit_graph_match_clear(&mut matched);
            vectorkit_graph_result_free(result)
        };

        let cancellation = vectorkit_graph_cancellation_new();
        unsafe { vectorkit_graph_cancellation_cancel(cancellation) };
        assert!(
            unsafe { vectorkit_graph_query(index, query, cancellation, &mut status) }.is_null()
        );
        assert_eq!(status.code, VK_GRAPH_STATUS_CANCELLED);
        unsafe {
            vectorkit_graph_cancellation_free(cancellation);
            super::super::vectorkit_status_clear(&mut status)
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "vectorkit-graph-ffi-{}-{nonce}",
            std::process::id()
        ));
        let path = CString::new(directory.to_string_lossy().as_bytes()).unwrap();
        assert!(unsafe { vectorkit_graph_index_save(index, path.as_ptr(), &mut status) });
        assert!(unsafe { vectorkit_graph_index_validate(path.as_ptr(), &mut status) });
        let loaded = unsafe { vectorkit_graph_index_load(path.as_ptr(), &mut status) };
        assert!(!loaded.is_null());

        unsafe {
            vectorkit_graph_index_free(loaded);
            vectorkit_graph_index_free(index);
            super::super::vectorkit_status_clear(&mut status);
        }
        let _ = std::fs::remove_dir_all(directory);
    }
}
