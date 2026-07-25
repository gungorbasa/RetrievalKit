use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use retrievalkit_core::{
    ChunkInput, ChunkKey, CompactionReport, CorpusId, Document, ExactVectorIndex, Filter,
    HybridFusion, HybridHit, HybridQuery, IndexConfig, IndexPersistenceOptions, KeywordHit,
    KeywordQuery, Metadata, MetadataValue, Record, RecordChunkInput, RetrievalDatabase,
    RetrievalDatabaseBuilder, SearchHit, SearchQuery, VectorEncoding, VectorMetric,
};
use retrievalkit_ingest::{chunk_text, ChunkingConfig, ChunkingStrategy};
use serde::Deserialize;

mod bench;
#[cfg(feature = "graph")]
mod device_graph_bench;
#[cfg(feature = "graph")]
mod graph;
mod memory_bench;
mod phase4_graph_free;

pub use bench::retrievalkit_bench_synthetic_json;
#[cfg(feature = "graph")]
pub use device_graph_bench::{
    retrievalkit_phase4_device_lifecycle_sample_json, retrievalkit_phase4_device_query_session_json,
};
#[cfg(feature = "graph")]
pub use graph::retrievalkit_graph_ffi_abi_version;
pub use memory_bench::{memory_benchmark_json, retrievalkit_bench_memory_json};
pub use phase4_graph_free::retrievalkit_phase4_graph_free_regression_json;

const VK_STATUS_OK: i32 = 0;
const VK_STATUS_INVALID_ARGUMENT: i32 = 1;
const VK_STATUS_CORE_ERROR: i32 = 2;
const VK_STATUS_PANIC: i32 = 3;
const VK_STATUS_CORRUPT_INDEX: i32 = 4;
const VK_STATUS_INVALID_DIMENSION: i32 = 5;
const VK_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE: i32 = 6;
const VK_STATUS_INVALID_IDENTITY: i32 = 7;
const VK_STATUS_MISSING_EMBEDDING: i32 = 8;

const VK_METRIC_COSINE: u32 = 0;
const VK_METRIC_DOT_PRODUCT: u32 = 1;

const VK_ENCODING_F32: u32 = 0;
const VK_ENCODING_F16: u32 = 1;
const VK_ENCODING_BF16: u32 = 2;
const VK_ENCODING_I8_SCALAR_QUANTIZED: u32 = 3;

const VK_METADATA_STRING: u32 = 0;
const VK_METADATA_INTEGER: u32 = 1;
const VK_METADATA_FLOAT: u32 = 2;
const VK_METADATA_BOOLEAN: u32 = 3;
const VK_METADATA_TIMESTAMP_MILLIS: u32 = 4;

const VK_FUSION_WEIGHTED_NORMALIZED_SCORE: u32 = 0;
const VK_FUSION_RECIPROCAL_RANK: u32 = 1;

const VK_CHUNKING_FIXED: u32 = 0;
const VK_CHUNKING_SENTENCE: u32 = 1;

#[repr(C)]
pub struct VkStatus {
    pub code: i32,
    pub message: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VkCompactionReport {
    pub chunks_before: usize,
    pub chunks_after: usize,
    pub chunks_removed: usize,
    pub estimated_bytes_before: usize,
    pub estimated_bytes_after: usize,
    pub estimated_bytes_reclaimed: usize,
}

impl From<CompactionReport> for VkCompactionReport {
    fn from(report: CompactionReport) -> Self {
        Self {
            chunks_before: report.chunks_before,
            chunks_after: report.chunks_after,
            chunks_removed: report.chunks_removed,
            estimated_bytes_before: report.estimated_bytes_before,
            estimated_bytes_after: report.estimated_bytes_after,
            estimated_bytes_reclaimed: report.estimated_bytes_reclaimed,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMetadataValue {
    pub value_type: u32,
    pub string_value: *const c_char,
    pub integer_value: i64,
    pub float_value: f64,
    pub bool_value: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkMetadataEntry {
    pub field: *const c_char,
    pub value: VkMetadataValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkChunkInput {
    pub text: *const c_char,
    pub embedding: *const c_float,
    pub embedding_len: usize,
    pub metadata: *const VkMetadataEntry,
    pub metadata_len: usize,
}

#[repr(C)]
pub struct VkChunkIdBuffer {
    pub values: *mut u64,
    pub count: usize,
}

#[repr(C)]
pub struct VkTextChunk {
    pub text: *mut c_char,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[repr(C)]
pub struct VkTextChunkBuffer {
    pub chunks: *mut VkTextChunk,
    pub count: usize,
}

#[repr(C)]
pub struct VkSearchHit {
    pub chunk_id: u64,
    pub document_id: *mut c_char,
    pub record_id: *mut c_char,
    pub text: *mut c_char,
    pub score: c_float,
    pub vector_score: c_float,
    pub filter_matched: bool,
}

#[repr(C)]
pub struct VkSearchResultBuffer {
    pub hits: *mut VkSearchHit,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkStringArray {
    pub values: *mut *mut c_char,
    pub count: usize,
}

#[repr(C)]
pub struct VkKeywordHit {
    pub chunk_id: u64,
    pub document_id: *mut c_char,
    pub record_id: *mut c_char,
    pub text: *mut c_char,
    pub score: c_float,
    pub matched_terms: VkStringArray,
}

#[repr(C)]
pub struct VkKeywordResultBuffer {
    pub hits: *mut VkKeywordHit,
    pub count: usize,
}

#[repr(C)]
pub struct VkHybridHit {
    pub chunk_id: u64,
    pub document_id: *mut c_char,
    pub record_id: *mut c_char,
    pub text: *mut c_char,
    pub score: c_float,
    pub has_vector_score: bool,
    pub vector_score: c_float,
    pub has_keyword_score: bool,
    pub keyword_score: c_float,
    pub has_vector_rank: bool,
    pub vector_rank: usize,
    pub has_keyword_rank: bool,
    pub keyword_rank: usize,
    pub has_normalized_vector_score: bool,
    pub normalized_vector_score: c_float,
    pub has_normalized_keyword_score: bool,
    pub normalized_keyword_score: c_float,
    pub matched_terms: VkStringArray,
    pub filter_matched: bool,
}

#[repr(C)]
pub struct VkHybridResultBuffer {
    pub hits: *mut VkHybridHit,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkHybridOptions {
    pub vector_top_k: usize,
    pub keyword_top_k: usize,
    pub fusion_type: u32,
    pub vector_weight: c_float,
    pub keyword_weight: c_float,
    pub rrf_k: c_float,
}

/// Public hybrid controls shared by high-level language bindings.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VkHybridQueryOptions {
    pub vector_top_k: usize,
    pub keyword_top_k: usize,
    pub alpha: c_float,
}

pub struct VkIndex {
    index: ExactVectorIndex,
}

pub struct VkRetrievalBuilder {
    builder: RetrievalDatabaseBuilder,
}

pub struct VkRetrievalDatabase {
    database: RetrievalDatabase,
}

#[derive(Deserialize)]
struct RetrievalRecordBatch {
    record: Record,
    #[serde(default)]
    projected_metadata: Metadata,
    chunks: Vec<RetrievalRecordChunk>,
}

#[derive(Deserialize)]
struct RetrievalRecordChunk {
    key: ChunkKey,
    text: String,
    embedding: Vec<f32>,
    #[serde(default)]
    metadata: Metadata,
}

pub struct VkFilter {
    filter: Filter,
}

/// Clears a status value populated by any RetrievalKit FFI function.
///
/// # Safety
///
/// `status`, when non-null, must point to a valid `VkStatus`. Its `message`
/// field must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_status_clear(status: *mut VkStatus) {
    if status.is_null() {
        return;
    }

    let status = unsafe { &mut *status };
    if !status.message.is_null() {
        unsafe { retrievalkit_string_free(status.message) };
    }
    status.code = VK_STATUS_OK;
    status.message = ptr::null_mut();
}

/// # Safety
/// String and status pointers must be valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_new(
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    status: *mut VkStatus,
) -> *mut VkRetrievalBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let builder = RetrievalDatabaseBuilder::new(
            corpus_id,
            parse_metric(metric)?,
            parse_encoding_code(encoding)?,
        );
        Ok(Box::into_raw(Box::new(VkRetrievalBuilder { builder })))
    })
}

/// Adds one public document without exposing its canonical record/chunk
/// projection to the caller.
///
/// # Safety
/// Every pointer must remain valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_upsert_document(
    builder: *mut VkRetrievalBuilder,
    document_id: *const c_char,
    text: *const c_char,
    metadata: *const VkMetadataEntry,
    metadata_len: usize,
    embedding: *const c_float,
    embedding_len: usize,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval builder must not be null"))?;
        let document = Document {
            id: unsafe { read_c_string(document_id, "document_id") }?,
            text: unsafe { read_c_string(text, "text") }?,
            metadata: unsafe { read_metadata(metadata, metadata_len) }?,
        };
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec();
        builder.builder.upsert_document(document, embedding)?;
        Ok(())
    })
}

/// # Safety
/// The builder must be live and `record_json` must be valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_upsert_record_json(
    builder: *mut VkRetrievalBuilder,
    record_json: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval builder must not be null"))?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: RetrievalRecordBatch = serde_json::from_str(&json).map_err(|error| {
            let code = if error.to_string().contains("missing field `embedding`") {
                VK_STATUS_MISSING_EMBEDDING
            } else {
                VK_STATUS_INVALID_ARGUMENT
            };
            FfiError {
                code,
                message: format!("invalid retrieval record JSON: {error}"),
            }
        })?;
        builder.builder.upsert_record(
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
        Ok(())
    })
}

/// # Safety
/// The builder must be live and is consumed by this call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_build(
    builder: *mut VkRetrievalBuilder,
    status: *mut VkStatus,
) -> *mut VkRetrievalDatabase {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument(
                "retrieval builder must not be null",
            ));
        }
        let builder = unsafe { Box::from_raw(builder) };
        Ok(Box::into_raw(Box::new(VkRetrievalDatabase {
            database: builder.builder.build()?,
        })))
    })
}

/// # Safety
/// The pointer must be null or a live builder not used elsewhere.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_free(builder: *mut VkRetrievalBuilder) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

/// # Safety
/// The directory and status pointers must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_load(
    directory: *const c_char,
    status: *mut VkStatus,
) -> *mut VkRetrievalDatabase {
    ffi_ptr(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        let database = RetrievalDatabase::load_from_dir(directory)?;
        Ok(Box::into_raw(Box::new(VkRetrievalDatabase { database })))
    })
}

/// # Safety
/// The database must be live and string/status pointers valid.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_save(
    database: *const VkRetrievalDatabase,
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
        let directory = unsafe { read_c_string(directory, "directory") }?;
        database.database.save_to_dir(directory)?;
        Ok(())
    })
}

/// # Safety
/// The directory and status pointers must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_validate(
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        RetrievalDatabase::validate_dir(directory)?;
        Ok(())
    })
}

/// # Safety
/// The pointer must be null or a live database not used elsewhere.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_free(database: *mut VkRetrievalDatabase) {
    if !database.is_null() {
        drop(unsafe { Box::from_raw(database) });
    }
}

/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_semantic_search(
    database: *const VkRetrievalDatabase,
    embedding: *const c_float,
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
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
        let mut query = SearchQuery::new(
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        );
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = database.database.semantic_search(&query)?;
        unsafe { *out_results = retrieval_search_result_buffer(&database.database, hits) };
        Ok(())
    })
}

/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_keyword_search(
    database: *const VkRetrievalDatabase,
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
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = database.database.keyword_search(&query)?;
        unsafe { *out_results = retrieval_keyword_result_buffer(&database.database, hits) };
        Ok(())
    })
}

/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_hybrid_search(
    database: *const VkRetrievalDatabase,
    text: *const c_char,
    embedding: *const c_float,
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
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
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
        let hits = database.database.hybrid_search(&query)?;
        unsafe { *out_results = retrieval_hybrid_result_buffer(&database.database, hits) };
        Ok(())
    })
}

/// Performs the public alpha-controlled hybrid query without requiring a
/// language binding to construct engine fusion weights.
///
/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_hybrid_search_alpha(
    database: *const VkRetrievalDatabase,
    text: *const c_char,
    embedding: *const c_float,
    embedding_len: usize,
    top_k: usize,
    filter: *const VkFilter,
    options: VkHybridQueryOptions,
    out_results: *mut VkHybridResultBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k)
        .try_with_alpha(options.alpha)
        .map_err(|error| FfiError::invalid_argument(error.to_string()))?;
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = database.database.hybrid_search(&query)?;
        unsafe { *out_results = retrieval_hybrid_result_buffer(&database.database, hits) };
        Ok(())
    })
}

/// Creates a new local exact/hybrid index.
///
/// # Safety
///
/// `status`, when non-null, must point to a valid `VkStatus`.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_new(
    dimension: usize,
    metric: u32,
    encoding: u32,
    status: *mut VkStatus,
) -> *mut VkIndex {
    ffi_ptr(status, || {
        let metric = parse_metric(metric)?;
        let encoding = parse_encoding_code(encoding)?;
        let config = IndexConfig::new(dimension, metric).with_vector_encoding(encoding);
        let index = ExactVectorIndex::try_with_config(config)?;
        Ok(Box::into_raw(Box::new(VkIndex { index })))
    })
}

/// Loads an index previously saved by RetrievalKit.
///
/// # Safety
///
/// `directory` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_load(
    directory: *const c_char,
    status: *mut VkStatus,
) -> *mut VkIndex {
    ffi_ptr(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        let index = ExactVectorIndex::load_from_dir(directory)?;
        Ok(Box::into_raw(Box::new(VkIndex { index })))
    })
}

/// Verifies a saved index without modifying it.
///
/// # Safety
///
/// `directory` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_validate(
    directory: *const c_char,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        ExactVectorIndex::validate_dir(directory)?;
        Ok(())
    })
}

/// Frees an index created or loaded by RetrievalKit.
///
/// # Safety
///
/// `index` must be null or a pointer returned by `retrievalkit_index_new` or
/// `retrievalkit_index_load` that has not already been freed. No other operation
/// may be using the handle.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_free(index: *mut VkIndex) {
    if !index.is_null() {
        unsafe { drop(Box::from_raw(index)) };
    }
}

/// Saves an index to a local directory.
///
/// # Safety
///
/// `index` must be a valid RetrievalKit index pointer and `directory` must point
/// to a valid null-terminated UTF-8 C string. The caller must provide exclusive
/// access to the index for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_save(
    index: *mut VkIndex,
    directory: *const c_char,
    include_bm25: bool,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        let index = unsafe { index_mut(index) }?;
        let directory = unsafe { read_c_string(directory, "directory") }?;
        index
            .index
            .save_to_dir_with_options(directory, IndexPersistenceOptions { include_bm25 })?;
        Ok(())
    })
}

/// Returns the index embedding dimension, or 0 for a null index pointer.
///
/// # Safety
///
/// `index` must be null or a valid RetrievalKit index pointer.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_dimension(index: *const VkIndex) -> usize {
    if index.is_null() {
        return 0;
    }
    unsafe { &*index }.index.dimension()
}

/// Returns the number of active chunks in the index, or 0 for a null pointer.
///
/// # Safety
///
/// `index` must be null or a valid RetrievalKit index pointer.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_active_chunk_count(index: *const VkIndex) -> usize {
    if index.is_null() {
        return 0;
    }
    unsafe { &*index }.index.active_chunk_count()
}

/// Returns the total number of stored chunks, including tombstones.
///
/// # Safety
///
/// `index` must be null or a valid RetrievalKit index pointer.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_total_chunk_count(index: *const VkIndex) -> usize {
    if index.is_null() {
        return 0;
    }
    unsafe { &*index }.index.len()
}

/// Returns the number of tombstoned chunks.
///
/// # Safety
///
/// `index` must be null or a valid RetrievalKit index pointer.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_tombstoned_chunk_count(index: *const VkIndex) -> usize {
    if index.is_null() {
        return 0;
    }
    unsafe { &*index }.index.tombstoned_chunk_count()
}

/// Splits UTF-8 text using the shared Rust ingestion implementation.
///
/// # Safety
///
/// `text` must point to a valid null-terminated UTF-8 C string. `out_chunks`
/// must point to writable memory. Free successful output with
/// `retrievalkit_text_chunks_free`.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_chunk_text(
    text: *const c_char,
    strategy: u32,
    max_characters: usize,
    overlap_characters: usize,
    out_chunks: *mut VkTextChunkBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_chunks.is_null() {
            return Err(FfiError::invalid_argument("out_chunks must not be null"));
        }
        let strategy = match strategy {
            VK_CHUNKING_FIXED => ChunkingStrategy::Fixed,
            VK_CHUNKING_SENTENCE => ChunkingStrategy::Sentence,
            _ => {
                return Err(FfiError::invalid_argument(format!(
                    "unsupported chunking strategy code {strategy}"
                )))
            }
        };
        let config = ChunkingConfig::new(max_characters, overlap_characters, strategy)
            .map_err(|error| FfiError::invalid_argument(error.to_string()))?;
        let text = unsafe { read_c_string(text, "text") }?;
        let chunks = chunk_text(&text, config)
            .into_iter()
            .map(|chunk| VkTextChunk {
                text: string_to_owned_ptr(&chunk.text),
                start_byte: chunk.start_byte,
                end_byte: chunk.end_byte,
            })
            .collect();
        unsafe { *out_chunks = text_chunk_buffer(chunks) };
        Ok(())
    })
}

/// Adds or replaces all chunks for a caller-owned document ID.
///
/// # Safety
///
/// All string, metadata, and embedding pointers must remain valid for the
/// duration of this call. The caller must provide exclusive access to the index.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_upsert_document(
    index: *mut VkIndex,
    document_id: *const c_char,
    document_text: *const c_char,
    document_metadata: *const VkMetadataEntry,
    document_metadata_len: usize,
    chunks: *const VkChunkInput,
    chunk_count: usize,
    out_chunk_ids: *mut VkChunkIdBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_chunk_ids.is_null() {
            return Err(FfiError::invalid_argument("out_chunk_ids must not be null"));
        }
        let index = unsafe { index_mut(index) }?;
        let document = Document {
            id: unsafe { read_c_string(document_id, "document_id") }?,
            text: unsafe { read_c_string(document_text, "document_text") }?,
            metadata: unsafe { read_metadata(document_metadata, document_metadata_len) }?,
        };
        let chunk_inputs = unsafe { read_chunk_inputs(chunks, chunk_count) }?;
        let chunk_ids = index.index.upsert_document(document, chunk_inputs)?;
        let buffer = chunk_id_buffer(chunk_ids);
        unsafe { *out_chunk_ids = buffer };
        Ok(())
    })
}

/// Deletes active chunks for a caller-owned document ID.
///
/// # Safety
///
/// `index` must be valid and `document_id` must point to a valid
/// null-terminated UTF-8 C string. The caller must provide exclusive access to
/// the index.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_delete_document(
    index: *mut VkIndex,
    document_id: *const c_char,
    deleted_count: *mut usize,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if deleted_count.is_null() {
            return Err(FfiError::invalid_argument("deleted_count must not be null"));
        }
        let index = unsafe { index_mut(index) }?;
        let document_id = unsafe { read_c_string(document_id, "document_id") }?;
        let count = index.index.delete_document(&document_id);
        unsafe { *deleted_count = count };
        Ok(())
    })
}

/// Rebuilds index storage without tombstoned chunks.
///
/// # Safety
///
/// `index` must be valid, `out_report` must point to writable memory, and the
/// caller must provide exclusive access to the index.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_compact(
    index: *mut VkIndex,
    out_report: *mut VkCompactionReport,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_report.is_null() {
            return Err(FfiError::invalid_argument("out_report must not be null"));
        }
        let index = unsafe { index_mut(index) }?;
        let report = index.index.compact()?;
        unsafe { *out_report = report.into() };
        Ok(())
    })
}

/// Performs exact vector search over active chunks.
///
/// # Safety
///
/// `embedding` must point to `embedding_len` contiguous `float` values. This
/// call may run concurrently with other read-only calls on the same index, but
/// not with save, mutation, compaction, or destruction. Each call requires its
/// own filter, status, and output storage.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_search(
    index: *const VkIndex,
    embedding: *const c_float,
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
        let index = unsafe { index_ref(index) }?;
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?;
        let mut query = SearchQuery::new(embedding.to_vec(), top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = index.index.search(&query)?;
        let buffer = search_result_buffer(index, hits);
        unsafe { *out_results = buffer };
        Ok(())
    })
}

/// Performs BM25 keyword search over active chunks.
///
/// # Safety
///
/// `text` must point to a valid null-terminated UTF-8 C string. This call may
/// run concurrently with other read-only calls on the same index, subject to
/// the same independent filter, status, and output-storage requirements.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_keyword_search(
    index: *const VkIndex,
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
        let index = unsafe { index_ref(index) }?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = index.index.keyword_search(&query)?;
        let buffer = keyword_result_buffer(index, hits);
        unsafe { *out_results = buffer };
        Ok(())
    })
}

/// Performs hybrid exact vector + BM25 search.
///
/// # Safety
///
/// `text` must be valid UTF-8 and `embedding` must point to `embedding_len`
/// contiguous `float` values. This call may run concurrently with other
/// read-only calls on the same index, subject to the same independent filter,
/// status, and output-storage requirements.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_hybrid_search(
    index: *const VkIndex,
    text: *const c_char,
    embedding: *const c_float,
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
        let index = unsafe { index_ref(index) }?;
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            embedding.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        query.fusion = parse_hybrid_fusion(options)?;
        let hits = index.index.hybrid_search(&query)?;
        let buffer = hybrid_result_buffer(index, hits);
        unsafe { *out_results = buffer };
        Ok(())
    })
}

/// Performs alpha-controlled hybrid search for language bindings.
///
/// # Safety
/// The safety contract is identical to [`retrievalkit_index_hybrid_search`].
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_hybrid_search_alpha(
    index: *const VkIndex,
    text: *const c_char,
    embedding: *const c_float,
    embedding_len: usize,
    top_k: usize,
    filter: *const VkFilter,
    options: VkHybridQueryOptions,
    out_results: *mut VkHybridResultBuffer,
    status: *mut VkStatus,
) -> bool {
    ffi_bool(status, || {
        if out_results.is_null() {
            return Err(FfiError::invalid_argument("out_results must not be null"));
        }
        let index = unsafe { index_ref(index) }?;
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?;
        let mut query = HybridQuery::new(
            unsafe { read_c_string(text, "text") }?,
            embedding.to_vec(),
            top_k,
        )
        .with_candidate_limits(options.vector_top_k, options.keyword_top_k)
        .try_with_alpha(options.alpha)
        .map_err(|error| FfiError::invalid_argument(error.to_string()))?;
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = index.index.hybrid_search(&query)?;
        let buffer = hybrid_result_buffer(index, hits);
        unsafe { *out_results = buffer };
        Ok(())
    })
}

/// Frees chunk IDs returned by `retrievalkit_index_upsert_document`.
///
/// # Safety
///
/// `buffer.values` must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_chunk_id_buffer_free(buffer: VkChunkIdBuffer) {
    if !buffer.values.is_null() {
        unsafe {
            drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
                buffer.values,
                buffer.count,
            )))
        };
    }
}

/// Frees chunks returned by `retrievalkit_chunk_text`.
///
/// # Safety
///
/// `buffer.chunks` must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_text_chunks_free(buffer: VkTextChunkBuffer) {
    if buffer.chunks.is_null() {
        return;
    }
    let chunks =
        unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.chunks, buffer.count)) };
    for chunk in chunks.iter() {
        unsafe { retrievalkit_string_free(chunk.text) };
    }
}

/// Frees exact search results returned by `retrievalkit_index_search`.
///
/// # Safety
///
/// `buffer.hits` must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_search_results_free(buffer: VkSearchResultBuffer) {
    if buffer.hits.is_null() {
        return;
    }
    let hits = unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.hits, buffer.count)) };
    for hit in hits.iter() {
        unsafe { retrievalkit_string_free(hit.document_id) };
        unsafe { retrievalkit_string_free(hit.record_id) };
        unsafe { retrievalkit_string_free(hit.text) };
    }
}

/// Frees keyword search results returned by `retrievalkit_index_keyword_search`.
///
/// # Safety
///
/// `buffer.hits` must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_keyword_results_free(buffer: VkKeywordResultBuffer) {
    if buffer.hits.is_null() {
        return;
    }
    let hits = unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.hits, buffer.count)) };
    for hit in hits.iter() {
        unsafe { retrievalkit_string_free(hit.document_id) };
        unsafe { retrievalkit_string_free(hit.record_id) };
        unsafe { retrievalkit_string_free(hit.text) };
        unsafe { string_array_free(hit.matched_terms) };
    }
}

/// Frees hybrid search results returned by `retrievalkit_index_hybrid_search`.
///
/// # Safety
///
/// `buffer.hits` must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_hybrid_results_free(buffer: VkHybridResultBuffer) {
    if buffer.hits.is_null() {
        return;
    }
    let hits = unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(buffer.hits, buffer.count)) };
    for hit in hits.iter() {
        unsafe { retrievalkit_string_free(hit.document_id) };
        unsafe { retrievalkit_string_free(hit.record_id) };
        unsafe { retrievalkit_string_free(hit.text) };
        unsafe { string_array_free(hit.matched_terms) };
    }
}

/// Builds an equality metadata filter.
///
/// # Safety
///
/// `field` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_equals(
    field: *const c_char,
    value: VkMetadataValue,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::Equals {
                field: unsafe { read_c_string(field, "field") }?,
                value: unsafe { read_metadata_value(value) }?,
            },
        })))
    })
}

/// Builds a not-equals metadata filter.
///
/// # Safety
///
/// `field` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_not_equals(
    field: *const c_char,
    value: VkMetadataValue,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::NotEquals {
                field: unsafe { read_c_string(field, "field") }?,
                value: unsafe { read_metadata_value(value) }?,
            },
        })))
    })
}

/// Builds an exists metadata filter.
///
/// # Safety
///
/// `field` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_exists(
    field: *const c_char,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::Exists {
                field: unsafe { read_c_string(field, "field") }?,
            },
        })))
    })
}

/// Builds an inclusive range metadata filter.
///
/// # Safety
///
/// `field` must point to a valid null-terminated UTF-8 C string. Lower and
/// upper pointers may be null to create one-sided ranges.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_range(
    field: *const c_char,
    lower: *const VkMetadataValue,
    upper: *const VkMetadataValue,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::Range {
                field: unsafe { read_c_string(field, "field") }?,
                lower: unsafe { optional_metadata_value(lower) }?,
                upper: unsafe { optional_metadata_value(upper) }?,
            },
        })))
    })
}

/// Builds an in-values metadata filter.
///
/// # Safety
///
/// `field` and `values` must remain valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_in_values(
    field: *const c_char,
    values: *const VkMetadataValue,
    value_count: usize,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        let values = unsafe { read_metadata_values(values, value_count) }?;
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::In {
                field: unsafe { read_c_string(field, "field") }?,
                values,
            },
        })))
    })
}

/// Builds an all-of filter from child filters.
///
/// # Safety
///
/// `filters` must point to `filter_count` valid RetrievalKit filter pointers.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_all(
    filters: *const *const VkFilter,
    filter_count: usize,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::All(unsafe { read_filter_list(filters, filter_count) }?),
        })))
    })
}

/// Builds an any-of filter from child filters.
///
/// # Safety
///
/// `filters` must point to `filter_count` valid RetrievalKit filter pointers.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_any(
    filters: *const *const VkFilter,
    filter_count: usize,
    status: *mut VkStatus,
) -> *mut VkFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(VkFilter {
            filter: Filter::Any(unsafe { read_filter_list(filters, filter_count) }?),
        })))
    })
}

/// Frees a filter created by RetrievalKit FFI.
///
/// # Safety
///
/// `filter` must be null or a pointer returned by a `retrievalkit_filter_*`
/// function that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_free(filter: *mut VkFilter) {
    if !filter.is_null() {
        unsafe { drop(Box::from_raw(filter)) };
    }
}

fn ffi_bool<F>(status: *mut VkStatus, operation: F) -> bool
where
    F: FnOnce() -> std::result::Result<(), FfiError>,
{
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => {
            unsafe { set_status_ok(status) };
            true
        }
        Ok(Err(error)) => {
            unsafe { set_status_error(status, error) };
            false
        }
        Err(_) => {
            unsafe { set_status_error(status, FfiError::panic()) };
            false
        }
    }
}

fn ffi_ptr<T, F>(status: *mut VkStatus, operation: F) -> *mut T
where
    F: FnOnce() -> std::result::Result<*mut T, FfiError>,
{
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(pointer)) => {
            unsafe { set_status_ok(status) };
            pointer
        }
        Ok(Err(error)) => {
            unsafe { set_status_error(status, error) };
            ptr::null_mut()
        }
        Err(_) => {
            unsafe { set_status_error(status, FfiError::panic()) };
            ptr::null_mut()
        }
    }
}

unsafe fn set_status_ok(status: *mut VkStatus) {
    unsafe { retrievalkit_status_clear(status) };
}

unsafe fn set_status_error(status: *mut VkStatus, error: FfiError) {
    if status.is_null() {
        return;
    }

    unsafe { retrievalkit_status_clear(status) };
    let status = unsafe { &mut *status };
    status.code = error.code;
    status.message = json_to_c_string(&error.message);
}

#[derive(Debug)]
struct FfiError {
    code: i32,
    message: String,
}

impl FfiError {
    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: VK_STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    fn core(error: retrievalkit_core::RetrievalKitError) -> Self {
        let code = match &error {
            retrievalkit_core::RetrievalKitError::CorruptIndex { .. } => VK_STATUS_CORRUPT_INDEX,
            retrievalkit_core::RetrievalKitError::InvalidDimension { .. } => {
                VK_STATUS_INVALID_DIMENSION
            }
            retrievalkit_core::RetrievalKitError::MissingEmbedding { .. } => {
                VK_STATUS_MISSING_EMBEDDING
            }
            retrievalkit_core::RetrievalKitError::RetrievalCapabilityUnavailable { .. } => {
                VK_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE
            }
            retrievalkit_core::RetrievalKitError::InvalidIdentity { .. }
            | retrievalkit_core::RetrievalKitError::InvalidRecordValue { .. } => {
                VK_STATUS_INVALID_IDENTITY
            }
            _ => VK_STATUS_CORE_ERROR,
        };
        Self {
            code,
            message: error.to_string(),
        }
    }

    fn panic() -> Self {
        Self {
            code: VK_STATUS_PANIC,
            message: "RetrievalKit FFI call panicked".to_owned(),
        }
    }
}

impl From<retrievalkit_core::RetrievalKitError> for FfiError {
    fn from(value: retrievalkit_core::RetrievalKitError) -> Self {
        Self::core(value)
    }
}

unsafe fn index_ref<'a>(index: *const VkIndex) -> std::result::Result<&'a VkIndex, FfiError> {
    if index.is_null() {
        return Err(FfiError::invalid_argument("index must not be null"));
    }
    Ok(unsafe { &*index })
}

unsafe fn index_mut<'a>(index: *mut VkIndex) -> std::result::Result<&'a mut VkIndex, FfiError> {
    if index.is_null() {
        return Err(FfiError::invalid_argument("index must not be null"));
    }
    Ok(unsafe { &mut *index })
}

unsafe fn read_c_string(value: *const c_char, name: &str) -> std::result::Result<String, FfiError> {
    if value.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| FfiError::invalid_argument(format!("{name} must be valid UTF-8")))
}

unsafe fn read_f32_slice<'a>(
    values: *const c_float,
    count: usize,
    name: &str,
) -> std::result::Result<&'a [f32], FfiError> {
    if count == 0 {
        return Ok(&[]);
    }
    if values.is_null() {
        return Err(FfiError::invalid_argument(format!(
            "{name} must not be null"
        )));
    }
    Ok(unsafe { slice::from_raw_parts(values.cast::<f32>(), count) })
}

unsafe fn read_metadata(
    entries: *const VkMetadataEntry,
    count: usize,
) -> std::result::Result<Metadata, FfiError> {
    if count == 0 {
        return Ok(Metadata::new());
    }
    if entries.is_null() {
        return Err(FfiError::invalid_argument(
            "metadata entries must not be null when metadata_len is non-zero",
        ));
    }

    let entries = unsafe { slice::from_raw_parts(entries, count) };
    let mut metadata = Metadata::new();
    for entry in entries {
        metadata.insert(
            unsafe { read_c_string(entry.field, "metadata field") }?,
            unsafe { read_metadata_value(entry.value) }?,
        );
    }
    Ok(metadata)
}

unsafe fn read_metadata_value(
    value: VkMetadataValue,
) -> std::result::Result<MetadataValue, FfiError> {
    match value.value_type {
        VK_METADATA_STRING => Ok(MetadataValue::String(unsafe {
            read_c_string(value.string_value, "metadata string value")
        }?)),
        VK_METADATA_INTEGER => Ok(MetadataValue::Integer(value.integer_value)),
        VK_METADATA_FLOAT => Ok(MetadataValue::Float(value.float_value)),
        VK_METADATA_BOOLEAN => Ok(MetadataValue::Boolean(value.bool_value)),
        VK_METADATA_TIMESTAMP_MILLIS => Ok(MetadataValue::TimestampMillis(value.integer_value)),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported metadata value type {}",
            value.value_type
        ))),
    }
}

unsafe fn optional_metadata_value(
    value: *const VkMetadataValue,
) -> std::result::Result<Option<MetadataValue>, FfiError> {
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(unsafe { read_metadata_value(*value) }?))
}

unsafe fn read_metadata_values(
    values: *const VkMetadataValue,
    count: usize,
) -> std::result::Result<Vec<MetadataValue>, FfiError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if values.is_null() {
        return Err(FfiError::invalid_argument(
            "values must not be null when value_count is non-zero",
        ));
    }
    unsafe { slice::from_raw_parts(values, count) }
        .iter()
        .map(|value| unsafe { read_metadata_value(*value) })
        .collect()
}

unsafe fn read_chunk_inputs(
    chunks: *const VkChunkInput,
    count: usize,
) -> std::result::Result<Vec<ChunkInput>, FfiError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if chunks.is_null() {
        return Err(FfiError::invalid_argument(
            "chunks must not be null when chunk_count is non-zero",
        ));
    }

    unsafe { slice::from_raw_parts(chunks, count) }
        .iter()
        .map(|chunk| {
            Ok(ChunkInput {
                text: unsafe { read_c_string(chunk.text, "chunk text") }?,
                embedding: unsafe {
                    read_f32_slice(chunk.embedding, chunk.embedding_len, "chunk embedding")
                }?
                .to_vec(),
                metadata: unsafe { read_metadata(chunk.metadata, chunk.metadata_len) }?,
            })
        })
        .collect()
}

unsafe fn optional_filter(filter: *const VkFilter) -> Option<Filter> {
    if filter.is_null() {
        None
    } else {
        Some(unsafe { &*filter }.filter.clone())
    }
}

unsafe fn read_filter_list(
    filters: *const *const VkFilter,
    count: usize,
) -> std::result::Result<Vec<Filter>, FfiError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if filters.is_null() {
        return Err(FfiError::invalid_argument(
            "filters must not be null when filter_count is non-zero",
        ));
    }

    unsafe { slice::from_raw_parts(filters, count) }
        .iter()
        .map(|filter| {
            if filter.is_null() {
                Err(FfiError::invalid_argument("child filter must not be null"))
            } else {
                Ok(unsafe { &**filter }.filter.clone())
            }
        })
        .collect()
}

fn parse_metric(metric: u32) -> std::result::Result<VectorMetric, FfiError> {
    match metric {
        VK_METRIC_COSINE => Ok(VectorMetric::Cosine),
        VK_METRIC_DOT_PRODUCT => Ok(VectorMetric::DotProduct),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported vector metric {metric}"
        ))),
    }
}

fn parse_encoding_code(encoding: u32) -> std::result::Result<VectorEncoding, FfiError> {
    match encoding {
        VK_ENCODING_F32 => Ok(VectorEncoding::F32),
        VK_ENCODING_F16 => Ok(VectorEncoding::F16),
        VK_ENCODING_BF16 => Ok(VectorEncoding::BF16),
        VK_ENCODING_I8_SCALAR_QUANTIZED => Ok(VectorEncoding::I8ScalarQuantized),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported vector encoding {encoding}"
        ))),
    }
}

fn parse_hybrid_fusion(options: VkHybridOptions) -> std::result::Result<HybridFusion, FfiError> {
    match options.fusion_type {
        VK_FUSION_WEIGHTED_NORMALIZED_SCORE => Ok(HybridFusion::WeightedNormalizedScore {
            vector_weight: options.vector_weight,
            keyword_weight: options.keyword_weight,
        }),
        VK_FUSION_RECIPROCAL_RANK => Ok(HybridFusion::ReciprocalRank {
            rrf_k: options.rrf_k,
        }),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported hybrid fusion type {}",
            options.fusion_type
        ))),
    }
}

fn chunk_id_buffer(values: Vec<u64>) -> VkChunkIdBuffer {
    let mut values = values.into_boxed_slice();
    let buffer = VkChunkIdBuffer {
        values: values.as_mut_ptr(),
        count: values.len(),
    };
    std::mem::forget(values);
    buffer
}

fn text_chunk_buffer(mut chunks: Vec<VkTextChunk>) -> VkTextChunkBuffer {
    let buffer = VkTextChunkBuffer {
        chunks: chunks.as_mut_ptr(),
        count: chunks.len(),
    };
    std::mem::forget(chunks);
    buffer
}

fn search_result_buffer(index: &VkIndex, hits: Vec<SearchHit>) -> VkSearchResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = index
                .index
                .chunk(hit.chunk_id)
                .map(|chunk| chunk.text.as_str())
                .unwrap_or("");
            VkSearchHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
                score: hit.score,
                vector_score: hit.trace.vector_score,
                filter_matched: hit.trace.filter_matched,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkSearchResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn retrieval_search_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<SearchHit>,
) -> VkSearchResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = database
                .chunk(hit.chunk_id)
                .map_or("", |chunk| chunk.text.as_str());
            VkSearchHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
                score: hit.score,
                vector_score: hit.trace.vector_score,
                filter_matched: hit.trace.filter_matched,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkSearchResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn keyword_result_buffer(index: &VkIndex, hits: Vec<KeywordHit>) -> VkKeywordResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = index
                .index
                .chunk(hit.chunk_id)
                .map(|chunk| chunk.text.as_str())
                .unwrap_or("");
            VkKeywordHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
                score: hit.score,
                matched_terms: string_array(hit.matched_terms),
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkKeywordResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn retrieval_keyword_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<KeywordHit>,
) -> VkKeywordResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = database
                .chunk(hit.chunk_id)
                .map_or("", |chunk| chunk.text.as_str());
            VkKeywordHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
                score: hit.score,
                matched_terms: string_array(hit.matched_terms),
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkKeywordResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn hybrid_result_buffer(index: &VkIndex, hits: Vec<HybridHit>) -> VkHybridResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = index
                .index
                .chunk(hit.chunk_id)
                .map(|chunk| chunk.text.as_str())
                .unwrap_or("");
            VkHybridHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
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
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkHybridResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn retrieval_hybrid_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<HybridHit>,
) -> VkHybridResultBuffer {
    let mut hits = hits
        .into_iter()
        .map(|hit| {
            let text = database
                .chunk(hit.chunk_id)
                .map_or("", |chunk| chunk.text.as_str());
            VkHybridHit {
                chunk_id: hit.chunk_id,
                document_id: string_to_owned_ptr(&hit.document_id),
                record_id: ptr::null_mut(),
                text: string_to_owned_ptr(text),
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
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let buffer = VkHybridResultBuffer {
        hits: hits.as_mut_ptr(),
        count: hits.len(),
    };
    std::mem::forget(hits);
    buffer
}

fn string_to_owned_ptr(value: &str) -> *mut c_char {
    json_to_c_string(value)
}

fn string_array(values: Vec<String>) -> VkStringArray {
    let mut pointers = values
        .into_iter()
        .map(|value| string_to_owned_ptr(&value))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let array = VkStringArray {
        values: pointers.as_mut_ptr(),
        count: pointers.len(),
    };
    std::mem::forget(pointers);
    array
}

unsafe fn string_array_free(array: VkStringArray) {
    if array.values.is_null() {
        return;
    }
    let pointers =
        unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(array.values, array.count)) };
    for pointer in pointers.iter() {
        unsafe { retrievalkit_string_free(*pointer) };
    }
}

fn json_to_c_string(json: &str) -> *mut c_char {
    let sanitized = json.replace('\0', "\\u0000");
    match CString::new(sanitized) {
        Ok(value) => value.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Frees a string returned by `retrievalkit_bench_synthetic_json`.
///
/// # Safety
///
/// `ptr` must be null or a pointer returned by `retrievalkit_bench_synthetic_json`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(test)]
    use crate::bench::{response_json, run_benchmark, BenchmarkConfig};

    #[test]
    fn default_config_includes_f32_f16_and_i8() {
        let config = BenchmarkConfig::default();

        assert_eq!(
            config.encodings,
            vec![
                VectorEncoding::F32,
                VectorEncoding::F16,
                VectorEncoding::I8ScalarQuantized,
            ]
        );
        assert_eq!(config.dimensions, vec![384, 768]);
        assert!(config.include_unfiltered);
        assert!(config.include_filtered);
        assert!(config.include_persistence);
        assert!(config.persist_bm25);
    }

    #[test]
    fn index_handle_contents_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VkIndex>();
    }

    #[test]
    fn chunking_ffi_returns_owned_text_and_offsets() {
        let text = CString::new("abçdef").unwrap();
        let mut status = VkStatus {
            code: -1,
            message: ptr::null_mut(),
        };
        let mut output = VkTextChunkBuffer {
            chunks: ptr::null_mut(),
            count: 0,
        };

        let success = unsafe {
            retrievalkit_chunk_text(
                text.as_ptr(),
                VK_CHUNKING_FIXED,
                4,
                1,
                &mut output,
                &mut status,
            )
        };

        assert!(success);
        assert_eq!(status.code, VK_STATUS_OK);
        let chunks = unsafe { slice::from_raw_parts(output.chunks, output.count) };
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            unsafe { CStr::from_ptr(chunks[0].text) }.to_str().unwrap(),
            "abçd"
        );
        assert_eq!((chunks[0].start_byte, chunks[0].end_byte), (0, 5));
        unsafe {
            retrievalkit_text_chunks_free(output);
            retrievalkit_status_clear(&mut status);
        }
    }

    #[test]
    fn benchmark_json_reports_all_requested_encoding_filter_pairs() {
        let config = BenchmarkConfig {
            chunks: 32,
            dimensions: vec![8],
            queries: 4,
            top_k: 3,
            filter_every: Some(4),
            ..BenchmarkConfig::default()
        };

        let report = run_benchmark(config).unwrap();

        assert_eq!(report.runs.len(), 6);
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "f32" && run.filter_every.is_none()));
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "f16" && run.filter_every == Some(4)));
        assert!(report
            .runs
            .iter()
            .any(|run| run.encoding == "i8-scalar-quantized" && run.filter_every == Some(4)));
        assert!(report.runs.iter().all(|run| run.persistence.is_some()));
    }

    #[test]
    fn benchmark_json_can_skip_persistence_metrics() {
        let report = run_benchmark(BenchmarkConfig {
            chunks: 16,
            dimensions: vec![8],
            queries: 2,
            top_k: 2,
            include_filtered: false,
            include_persistence: false,
            ..BenchmarkConfig::default()
        })
        .unwrap();

        assert!(report.runs.iter().all(|run| run.persistence.is_none()));
    }

    #[test]
    fn benchmark_json_can_skip_bm25_persistence() {
        let report = run_benchmark(BenchmarkConfig {
            chunks: 16,
            dimensions: vec![8],
            queries: 2,
            top_k: 2,
            include_filtered: false,
            persist_bm25: false,
            ..BenchmarkConfig::default()
        })
        .unwrap();

        assert!(report.runs.iter().all(|run| {
            let persistence = run.persistence.as_ref().unwrap();
            !persistence.persist_bm25 && persistence.file_sizes.bm25_bytes == 0
        }));
    }

    #[test]
    fn benchmark_json_can_skip_recall_ground_truth() {
        let report = run_benchmark(BenchmarkConfig {
            chunks: 16,
            dimensions: vec![8],
            queries: 2,
            top_k: 2,
            encodings: vec![VectorEncoding::I8ScalarQuantized],
            include_recall: false,
            ..BenchmarkConfig::default()
        })
        .unwrap();

        assert_eq!(report.runs.len(), 2);
        assert!(report
            .runs
            .iter()
            .all(|run| run.recall_at_k_vs_f32.is_none()));
    }

    #[test]
    fn invalid_config_returns_error_json() {
        let response = response_json(run_benchmark(BenchmarkConfig {
            chunks: 0,
            ..BenchmarkConfig::default()
        }));

        assert!(response.contains("\"ok\":false"));
        assert!(response.contains("chunks must be greater than zero"));
    }

    #[test]
    fn ffi_function_returns_json_and_owned_string_can_be_freed() {
        let config = CString::new(
            r#"{"chunks":16,"dimensions":[8],"queries":2,"top_k":2,"encodings":["f32","f16","i8"],"include_filtered":false}"#,
        )
        .unwrap();

        let ptr = unsafe { retrievalkit_bench_synthetic_json(config.as_ptr()) };

        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        unsafe { retrievalkit_string_free(ptr) };

        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"encoding\":\"f32\""));
        assert!(json.contains("\"encoding\":\"f16\""));
        assert!(json.contains("\"encoding\":\"i8-scalar-quantized\""));
        assert!(json.contains("\"persistence\""));
        assert!(json.contains("\"persist_bm25\":true"));
        assert!(json.contains("\"post_load_search\""));
    }

    #[test]
    fn sdk_ffi_can_upsert_filter_search_and_delete() {
        let mut status = empty_status();
        let index =
            unsafe { retrievalkit_index_new(2, VK_METRIC_COSINE, VK_ENCODING_F32, &mut status) };
        assert!(!index.is_null());
        assert_status_ok(&status);

        let document_id = CString::new("doc-1").unwrap();
        let document_text = CString::new("document text").unwrap();
        let bucket_field = CString::new("bucket").unwrap();
        let chunk_text_keep = CString::new("keep").unwrap();
        let chunk_text_skip = CString::new("skip").unwrap();
        let keep_embedding = [1.0_f32, 0.0];
        let skip_embedding = [0.0_f32, 1.0];
        let keep_metadata = [VkMetadataEntry {
            field: bucket_field.as_ptr(),
            value: VkMetadataValue {
                value_type: VK_METADATA_INTEGER,
                string_value: ptr::null(),
                integer_value: 1,
                float_value: 0.0,
                bool_value: false,
            },
        }];
        let skip_metadata = [VkMetadataEntry {
            field: bucket_field.as_ptr(),
            value: VkMetadataValue {
                value_type: VK_METADATA_INTEGER,
                string_value: ptr::null(),
                integer_value: 2,
                float_value: 0.0,
                bool_value: false,
            },
        }];
        let chunks = [
            VkChunkInput {
                text: chunk_text_keep.as_ptr(),
                embedding: keep_embedding.as_ptr(),
                embedding_len: keep_embedding.len(),
                metadata: keep_metadata.as_ptr(),
                metadata_len: keep_metadata.len(),
            },
            VkChunkInput {
                text: chunk_text_skip.as_ptr(),
                embedding: skip_embedding.as_ptr(),
                embedding_len: skip_embedding.len(),
                metadata: skip_metadata.as_ptr(),
                metadata_len: skip_metadata.len(),
            },
        ];
        let mut chunk_ids = VkChunkIdBuffer {
            values: ptr::null_mut(),
            count: 0,
        };

        let upserted = unsafe {
            retrievalkit_index_upsert_document(
                index,
                document_id.as_ptr(),
                document_text.as_ptr(),
                ptr::null(),
                0,
                chunks.as_ptr(),
                chunks.len(),
                &mut chunk_ids,
                &mut status,
            )
        };
        assert!(upserted);
        assert_status_ok(&status);
        assert_eq!(chunk_ids.count, 2);
        unsafe { retrievalkit_chunk_id_buffer_free(chunk_ids) };

        let filter_value = VkMetadataValue {
            value_type: VK_METADATA_INTEGER,
            string_value: ptr::null(),
            integer_value: 2,
            float_value: 0.0,
            bool_value: false,
        };
        let filter =
            unsafe { retrievalkit_filter_equals(bucket_field.as_ptr(), filter_value, &mut status) };
        assert!(!filter.is_null());
        assert_status_ok(&status);

        let query = [1.0_f32, 0.0];
        let mut results = VkSearchResultBuffer {
            hits: ptr::null_mut(),
            count: 0,
        };
        let searched = unsafe {
            retrievalkit_index_search(
                index,
                query.as_ptr(),
                query.len(),
                2,
                filter,
                &mut results,
                &mut status,
            )
        };
        assert!(searched);
        assert_status_ok(&status);
        assert_eq!(results.count, 1);
        let hit = unsafe { &*results.hits };
        assert_eq!(
            unsafe { CStr::from_ptr(hit.text) }.to_str().unwrap(),
            "skip"
        );
        unsafe { retrievalkit_search_results_free(results) };

        let mut deleted_count = 0;
        let deleted = unsafe {
            retrievalkit_index_delete_document(
                index,
                document_id.as_ptr(),
                &mut deleted_count,
                &mut status,
            )
        };
        assert!(deleted);
        assert_status_ok(&status);
        assert_eq!(deleted_count, 2);
        assert_eq!(unsafe { retrievalkit_index_active_chunk_count(index) }, 0);
        assert_eq!(unsafe { retrievalkit_index_total_chunk_count(index) }, 2);
        assert_eq!(
            unsafe { retrievalkit_index_tombstoned_chunk_count(index) },
            2
        );

        let mut compaction = VkCompactionReport::default();
        let compacted = unsafe { retrievalkit_index_compact(index, &mut compaction, &mut status) };
        assert!(compacted);
        assert_status_ok(&status);
        assert_eq!(compaction.chunks_before, 2);
        assert_eq!(compaction.chunks_after, 0);
        assert_eq!(compaction.chunks_removed, 2);
        assert!(compaction.estimated_bytes_reclaimed > 0);
        assert_eq!(unsafe { retrievalkit_index_total_chunk_count(index) }, 0);
        assert_eq!(
            unsafe { retrievalkit_index_tombstoned_chunk_count(index) },
            0
        );

        unsafe {
            retrievalkit_filter_free(filter);
            retrievalkit_index_free(index);
            retrievalkit_status_clear(&mut status);
        }
    }

    #[test]
    fn sdk_ffi_reports_dimension_errors_through_status() {
        let mut status = empty_status();
        let index =
            unsafe { retrievalkit_index_new(2, VK_METRIC_COSINE, VK_ENCODING_F32, &mut status) };
        assert!(!index.is_null());

        let query = [1.0_f32];
        let mut results = VkSearchResultBuffer {
            hits: ptr::null_mut(),
            count: 0,
        };
        let searched = unsafe {
            retrievalkit_index_search(
                index,
                query.as_ptr(),
                query.len(),
                1,
                ptr::null(),
                &mut results,
                &mut status,
            )
        };

        assert!(!searched);
        assert_eq!(status.code, VK_STATUS_INVALID_DIMENSION);
        let message = unsafe { CStr::from_ptr(status.message) }
            .to_str()
            .unwrap()
            .to_owned();
        assert!(message.contains("invalid vector dimension"));

        unsafe {
            retrievalkit_index_free(index);
            retrievalkit_status_clear(&mut status);
        }
    }

    #[test]
    fn sdk_ffi_save_and_load_round_trips_results() {
        let mut status = empty_status();
        let index =
            unsafe { retrievalkit_index_new(2, VK_METRIC_COSINE, VK_ENCODING_F32, &mut status) };
        assert!(!index.is_null());

        let document_id = CString::new("doc-1").unwrap();
        let document_text = CString::new("").unwrap();
        let chunk_text = CString::new("persisted").unwrap();
        let embedding = [0.0_f32, 1.0];
        let chunks = [VkChunkInput {
            text: chunk_text.as_ptr(),
            embedding: embedding.as_ptr(),
            embedding_len: embedding.len(),
            metadata: ptr::null(),
            metadata_len: 0,
        }];
        let mut chunk_ids = VkChunkIdBuffer {
            values: ptr::null_mut(),
            count: 0,
        };
        assert!(unsafe {
            retrievalkit_index_upsert_document(
                index,
                document_id.as_ptr(),
                document_text.as_ptr(),
                ptr::null(),
                0,
                chunks.as_ptr(),
                chunks.len(),
                &mut chunk_ids,
                &mut status,
            )
        });
        unsafe { retrievalkit_chunk_id_buffer_free(chunk_ids) };

        let directory = std::env::temp_dir().join(format!(
            "retrievalkit-sdk-ffi-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory_string = CString::new(directory.to_string_lossy().as_bytes()).unwrap();
        assert!(unsafe {
            retrievalkit_index_save(index, directory_string.as_ptr(), true, &mut status)
        });

        let loaded = unsafe { retrievalkit_index_load(directory_string.as_ptr(), &mut status) };
        assert!(!loaded.is_null());
        assert_eq!(unsafe { retrievalkit_index_dimension(loaded) }, 2);
        assert_eq!(unsafe { retrievalkit_index_active_chunk_count(loaded) }, 1);

        let mut results = VkSearchResultBuffer {
            hits: ptr::null_mut(),
            count: 0,
        };
        assert!(unsafe {
            retrievalkit_index_search(
                loaded,
                embedding.as_ptr(),
                embedding.len(),
                1,
                ptr::null(),
                &mut results,
                &mut status,
            )
        });
        assert_eq!(results.count, 1);
        let hit = unsafe { &*results.hits };
        assert_eq!(
            unsafe { CStr::from_ptr(hit.text) }.to_str().unwrap(),
            "persisted"
        );

        unsafe {
            retrievalkit_search_results_free(results);
            retrievalkit_index_free(loaded);
            retrievalkit_index_free(index);
            retrievalkit_status_clear(&mut status);
        }
        let _ = fs::remove_dir_all(directory);
    }

    fn empty_status() -> VkStatus {
        VkStatus {
            code: VK_STATUS_OK,
            message: ptr::null_mut(),
        }
    }

    fn assert_status_ok(status: &VkStatus) {
        assert_eq!(status.code, VK_STATUS_OK);
        assert!(status.message.is_null());
    }
}
