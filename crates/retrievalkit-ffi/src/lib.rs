use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

use retrievalkit_core::{
    Bm25Config, ChunkInput, ChunkKey, CompactionReport, CorpusId, Document, ExactVectorIndex,
    Filter, HybridHit, HybridQuery, IndexConfig, IndexPersistenceOptions, KeywordHit, KeywordQuery,
    Metadata, MetadataValue, Record, RecordChunkInput, RetrievalDatabase, RetrievalDatabaseBuilder,
    SearchHit, SearchQuery, VectorEncoding, VectorMetric,
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

/// Returns the active native vector runtime capabilities as UTF-8 JSON.
/// Free the returned string with `retrievalkit_string_free`.
#[no_mangle]
pub extern "C" fn retrievalkit_runtime_capabilities_json() -> *mut c_char {
    json_to_c_string(&bench::runtime_capabilities_json())
}

const RETRIEVALKIT_STATUS_OK: i32 = 0;
const RETRIEVALKIT_STATUS_INVALID_ARGUMENT: i32 = 1;
const RETRIEVALKIT_STATUS_CORE_ERROR: i32 = 2;
const RETRIEVALKIT_STATUS_PANIC: i32 = 3;
const RETRIEVALKIT_STATUS_CORRUPT_INDEX: i32 = 4;
const RETRIEVALKIT_STATUS_INVALID_DIMENSION: i32 = 5;
const RETRIEVALKIT_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE: i32 = 6;
const RETRIEVALKIT_STATUS_INVALID_IDENTITY: i32 = 7;
const RETRIEVALKIT_STATUS_MISSING_EMBEDDING: i32 = 8;

const RETRIEVALKIT_METRIC_COSINE: u32 = 0;
const RETRIEVALKIT_METRIC_DOT_PRODUCT: u32 = 1;

const RETRIEVALKIT_ENCODING_F32: u32 = 0;
const RETRIEVALKIT_ENCODING_F16: u32 = 1;
const RETRIEVALKIT_ENCODING_BF16: u32 = 2;
const RETRIEVALKIT_ENCODING_I8_SCALAR_QUANTIZED: u32 = 3;

const RETRIEVALKIT_METADATA_STRING: u32 = 0;
const RETRIEVALKIT_METADATA_INTEGER: u32 = 1;
const RETRIEVALKIT_METADATA_FLOAT: u32 = 2;
const RETRIEVALKIT_METADATA_BOOLEAN: u32 = 3;
const RETRIEVALKIT_METADATA_TIMESTAMP_MILLIS: u32 = 4;

const RETRIEVALKIT_CHUNKING_FIXED: u32 = 0;
const RETRIEVALKIT_CHUNKING_SENTENCE: u32 = 1;

#[repr(C)]
pub struct RetrievalKitStatus {
    pub code: i32,
    pub message: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RetrievalKitCompactionReport {
    pub chunks_before: usize,
    pub chunks_after: usize,
    pub chunks_removed: usize,
    pub estimated_bytes_before: usize,
    pub estimated_bytes_after: usize,
    pub estimated_bytes_reclaimed: usize,
}

impl From<CompactionReport> for RetrievalKitCompactionReport {
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
pub struct RetrievalKitMetadataValue {
    pub value_type: u32,
    pub string_value: *const c_char,
    pub integer_value: i64,
    pub float_value: f64,
    pub bool_value: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitMetadataEntry {
    pub field: *const c_char,
    pub value: RetrievalKitMetadataValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitChunkInput {
    pub text: *const c_char,
    pub embedding: *const c_float,
    pub embedding_len: usize,
    pub metadata: *const RetrievalKitMetadataEntry,
    pub metadata_len: usize,
}

#[repr(C)]
pub struct RetrievalKitChunkIdBuffer {
    pub values: *mut u64,
    pub count: usize,
}

#[repr(C)]
pub struct RetrievalKitTextChunk {
    pub text: *mut c_char,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[repr(C)]
pub struct RetrievalKitTextChunkBuffer {
    pub chunks: *mut RetrievalKitTextChunk,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RetrievalKitUtf8Range {
    pub offset: usize,
    pub length: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RetrievalKitPackedMetadataEntry {
    pub key: RetrievalKitUtf8Range,
    pub value_type: u32,
    pub string_value: RetrievalKitUtf8Range,
    pub integer_value: i64,
    pub float_value: f64,
    pub bool_value: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitSearchHit {
    pub chunk_id: u64,
    pub document_id: RetrievalKitUtf8Range,
    pub has_record_id: bool,
    pub record_id: RetrievalKitUtf8Range,
    pub text: RetrievalKitUtf8Range,
    pub score: c_float,
    pub vector_score: c_float,
    pub metadata_start: usize,
    pub metadata_count: usize,
}

#[repr(C)]
pub struct RetrievalKitSearchResultBuffer {
    pub hits: *const RetrievalKitSearchHit,
    pub count: usize,
    pub utf8: *const u8,
    pub utf8_len: usize,
    pub metadata: *const RetrievalKitPackedMetadataEntry,
    pub metadata_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitStringArray {
    pub values: *mut *mut c_char,
    pub count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitKeywordHit {
    pub chunk_id: u64,
    pub document_id: RetrievalKitUtf8Range,
    pub has_record_id: bool,
    pub record_id: RetrievalKitUtf8Range,
    pub text: RetrievalKitUtf8Range,
    pub score: c_float,
    pub matched_terms_start: usize,
    pub matched_terms_count: usize,
    pub metadata_start: usize,
    pub metadata_count: usize,
}

#[repr(C)]
pub struct RetrievalKitKeywordResultBuffer {
    pub hits: *const RetrievalKitKeywordHit,
    pub count: usize,
    pub utf8: *const u8,
    pub utf8_len: usize,
    pub matched_terms: *const RetrievalKitUtf8Range,
    pub matched_terms_count: usize,
    pub metadata: *const RetrievalKitPackedMetadataEntry,
    pub metadata_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitHybridHit {
    pub chunk_id: u64,
    pub document_id: RetrievalKitUtf8Range,
    pub has_record_id: bool,
    pub record_id: RetrievalKitUtf8Range,
    pub text: RetrievalKitUtf8Range,
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
    pub matched_terms_start: usize,
    pub matched_terms_count: usize,
    pub metadata_start: usize,
    pub metadata_count: usize,
}

#[repr(C)]
pub struct RetrievalKitHybridResultBuffer {
    pub hits: *const RetrievalKitHybridHit,
    pub count: usize,
    pub utf8: *const u8,
    pub utf8_len: usize,
    pub matched_terms: *const RetrievalKitUtf8Range,
    pub matched_terms_count: usize,
    pub metadata: *const RetrievalKitPackedMetadataEntry,
    pub metadata_count: usize,
    pub alpha: c_float,
}

/// Public hybrid controls shared by high-level language bindings.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RetrievalKitHybridQueryOptions {
    pub vector_top_k: usize,
    pub keyword_top_k: usize,
    pub alpha: c_float,
}

pub struct RetrievalKitIndex {
    index: ExactVectorIndex,
}

pub struct RetrievalKitRetrievalBuilder {
    builder: RetrievalDatabaseBuilder,
}

pub struct RetrievalKitRetrievalDatabase {
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

pub struct RetrievalKitFilter {
    filter: Filter,
}

/// Clears a status value populated by any RetrievalKit FFI function.
///
/// # Safety
///
/// `status`, when non-null, must point to a valid `RetrievalKitStatus`. Its `message`
/// field must be null or a pointer allocated by RetrievalKit FFI.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_status_clear(status: *mut RetrievalKitStatus) {
    if status.is_null() {
        return;
    }

    let status = unsafe { &mut *status };
    if !status.message.is_null() {
        unsafe { retrievalkit_string_free(status.message) };
    }
    status.code = RETRIEVALKIT_STATUS_OK;
    status.message = ptr::null_mut();
}

/// # Safety
/// String and status pointers must be valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_new(
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitRetrievalBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let builder = RetrievalDatabaseBuilder::new(
            corpus_id,
            parse_metric(metric)?,
            parse_encoding_code(encoding)?,
        );
        Ok(Box::into_raw(Box::new(RetrievalKitRetrievalBuilder {
            builder,
        })))
    })
}

/// Creates a retrieval builder with explicit BM25 scoring and stop-word configuration.
///
/// # Safety
/// String and status pointers must be valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_new_with_bm25(
    metric: u32,
    encoding: u32,
    corpus_id: *const c_char,
    bm25_k1: c_float,
    bm25_b: c_float,
    stop_words_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitRetrievalBuilder {
    ffi_ptr(status, || {
        let corpus_id = CorpusId::new(unsafe { read_c_string(corpus_id, "corpus_id") }?)?;
        let stop_words_json = unsafe { read_c_string(stop_words_json, "stop_words_json")? };
        let stop_words =
            serde_json::from_str::<Vec<String>>(&stop_words_json).map_err(|error| {
                FfiError::invalid_argument(format!("invalid stop_words_json: {error}"))
            })?;
        let builder = RetrievalDatabaseBuilder::new(
            corpus_id,
            parse_metric(metric)?,
            parse_encoding_code(encoding)?,
        )
        .try_with_bm25_config(Bm25Config::try_new(bm25_k1, bm25_b, stop_words)?)?;
        Ok(Box::into_raw(Box::new(RetrievalKitRetrievalBuilder {
            builder,
        })))
    })
}

/// Adds one public document without exposing its canonical record/chunk
/// projection to the caller.
///
/// # Safety
/// Every pointer must remain valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_upsert_document(
    builder: *mut RetrievalKitRetrievalBuilder,
    document_id: *const c_char,
    text: *const c_char,
    metadata: *const RetrievalKitMetadataEntry,
    metadata_len: usize,
    embedding: *const c_float,
    embedding_len: usize,
    status: *mut RetrievalKitStatus,
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
    builder: *mut RetrievalKitRetrievalBuilder,
    record_json: *const c_char,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        let builder = unsafe { builder.as_mut() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval builder must not be null"))?;
        let json = unsafe { read_c_string(record_json, "record_json") }?;
        let batch: RetrievalRecordBatch = serde_json::from_str(&json).map_err(|error| {
            let code = if error.to_string().contains("missing field `embedding`") {
                RETRIEVALKIT_STATUS_MISSING_EMBEDDING
            } else {
                RETRIEVALKIT_STATUS_INVALID_ARGUMENT
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
    builder: *mut RetrievalKitRetrievalBuilder,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitRetrievalDatabase {
    ffi_ptr(status, || {
        if builder.is_null() {
            return Err(FfiError::invalid_argument(
                "retrieval builder must not be null",
            ));
        }
        let builder = unsafe { Box::from_raw(builder) };
        Ok(Box::into_raw(Box::new(RetrievalKitRetrievalDatabase {
            database: builder.builder.build()?,
        })))
    })
}

/// # Safety
/// The pointer must be null or a live builder not used elsewhere.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_builder_free(
    builder: *mut RetrievalKitRetrievalBuilder,
) {
    if !builder.is_null() {
        drop(unsafe { Box::from_raw(builder) });
    }
}

/// # Safety
/// The directory and status pointers must be valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_load(
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitRetrievalDatabase {
    ffi_ptr(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        let database = RetrievalDatabase::load_from_dir(directory)?;
        Ok(Box::into_raw(Box::new(RetrievalKitRetrievalDatabase {
            database,
        })))
    })
}

/// # Safety
/// The database must be live and string/status pointers valid.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_database_save(
    database: *const RetrievalKitRetrievalDatabase,
    directory: *const c_char,
    status: *mut RetrievalKitStatus,
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
    status: *mut RetrievalKitStatus,
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
pub unsafe extern "C" fn retrievalkit_retrieval_database_free(
    database: *mut RetrievalKitRetrievalDatabase,
) {
    if !database.is_null() {
        drop(unsafe { Box::from_raw(database) });
    }
}

/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_semantic_search(
    database: *const RetrievalKitRetrievalDatabase,
    embedding: *const c_float,
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
        unsafe { *out_results = retrieval_search_result_buffer(&database.database, hits)? };
        Ok(())
    })
}

/// # Safety
/// All input and output pointers must remain valid for the call.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_retrieval_keyword_search(
    database: *const RetrievalKitRetrievalDatabase,
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
        let database = unsafe { database.as_ref() }
            .ok_or_else(|| FfiError::invalid_argument("retrieval database must not be null"))?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = database.database.keyword_search(&query)?;
        unsafe { *out_results = retrieval_keyword_result_buffer(&database.database, hits)? };
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
    database: *const RetrievalKitRetrievalDatabase,
    text: *const c_char,
    embedding: *const c_float,
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
        unsafe {
            *out_results = retrieval_hybrid_result_buffer(&database.database, hits, options.alpha)?
        };
        Ok(())
    })
}

/// Creates a new local exact/hybrid index.
///
/// # Safety
///
/// `status`, when non-null, must point to a valid `RetrievalKitStatus`.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_new(
    dimension: usize,
    metric: u32,
    encoding: u32,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitIndex {
    ffi_ptr(status, || {
        let metric = parse_metric(metric)?;
        let encoding = parse_encoding_code(encoding)?;
        let config = IndexConfig::new(dimension, metric).with_vector_encoding(encoding);
        let index = ExactVectorIndex::try_with_config(config)?;
        Ok(Box::into_raw(Box::new(RetrievalKitIndex { index })))
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
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitIndex {
    ffi_ptr(status, || {
        let directory = unsafe { read_c_string(directory, "directory") }?;
        let index = ExactVectorIndex::load_from_dir(directory)?;
        Ok(Box::into_raw(Box::new(RetrievalKitIndex { index })))
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
    status: *mut RetrievalKitStatus,
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
pub unsafe extern "C" fn retrievalkit_index_free(index: *mut RetrievalKitIndex) {
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
    index: *mut RetrievalKitIndex,
    directory: *const c_char,
    include_bm25: bool,
    status: *mut RetrievalKitStatus,
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
pub unsafe extern "C" fn retrievalkit_index_dimension(index: *const RetrievalKitIndex) -> usize {
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
pub unsafe extern "C" fn retrievalkit_index_active_chunk_count(
    index: *const RetrievalKitIndex,
) -> usize {
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
pub unsafe extern "C" fn retrievalkit_index_total_chunk_count(
    index: *const RetrievalKitIndex,
) -> usize {
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
pub unsafe extern "C" fn retrievalkit_index_tombstoned_chunk_count(
    index: *const RetrievalKitIndex,
) -> usize {
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
    out_chunks: *mut RetrievalKitTextChunkBuffer,
    status: *mut RetrievalKitStatus,
) -> bool {
    ffi_bool(status, || {
        if out_chunks.is_null() {
            return Err(FfiError::invalid_argument("out_chunks must not be null"));
        }
        let strategy = match strategy {
            RETRIEVALKIT_CHUNKING_FIXED => ChunkingStrategy::Fixed,
            RETRIEVALKIT_CHUNKING_SENTENCE => ChunkingStrategy::Sentence,
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
            .map(|chunk| RetrievalKitTextChunk {
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
    index: *mut RetrievalKitIndex,
    document_id: *const c_char,
    document_text: *const c_char,
    document_metadata: *const RetrievalKitMetadataEntry,
    document_metadata_len: usize,
    chunks: *const RetrievalKitChunkInput,
    chunk_count: usize,
    out_chunk_ids: *mut RetrievalKitChunkIdBuffer,
    status: *mut RetrievalKitStatus,
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
    index: *mut RetrievalKitIndex,
    document_id: *const c_char,
    deleted_count: *mut usize,
    status: *mut RetrievalKitStatus,
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
    index: *mut RetrievalKitIndex,
    out_report: *mut RetrievalKitCompactionReport,
    status: *mut RetrievalKitStatus,
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
    index: *const RetrievalKitIndex,
    embedding: *const c_float,
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
        let index = unsafe { index_ref(index) }?;
        let embedding = unsafe { read_f32_slice(embedding, embedding_len, "embedding") }?;
        let mut query = SearchQuery::new(embedding.to_vec(), top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = index.index.search(&query)?;
        let buffer = search_result_buffer(index, hits)?;
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
    index: *const RetrievalKitIndex,
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
        let index = unsafe { index_ref(index) }?;
        let mut query = KeywordQuery::new(unsafe { read_c_string(text, "text") }?, top_k);
        if let Some(filter) = unsafe { optional_filter(filter) } {
            query = query.with_filter(filter);
        }
        let hits = index.index.keyword_search(&query)?;
        let buffer = keyword_result_buffer(index, hits)?;
        unsafe { *out_results = buffer };
        Ok(())
    })
}

/// Performs alpha-controlled hybrid search for language bindings.
///
/// # Safety
/// `text` must be valid UTF-8 and `embedding` must point to `embedding_len`
/// contiguous `float` values. This call may run concurrently with other
/// read-only calls on the same index, subject to independent filter, status,
/// and output-storage requirements.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_index_hybrid_search_alpha(
    index: *const RetrievalKitIndex,
    text: *const c_char,
    embedding: *const c_float,
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
        let buffer = hybrid_result_buffer(index, hits, options.alpha)?;
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
pub unsafe extern "C" fn retrievalkit_chunk_id_buffer_free(buffer: RetrievalKitChunkIdBuffer) {
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
pub unsafe extern "C" fn retrievalkit_text_chunks_free(buffer: RetrievalKitTextChunkBuffer) {
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
/// Every non-null pointer in `buffer` must have been allocated by RetrievalKit
/// FFI and the buffer must not have been freed before.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_search_results_free(buffer: RetrievalKitSearchResultBuffer) {
    unsafe { drop_ffi_slice(buffer.hits, buffer.count) };
    unsafe { drop_ffi_slice(buffer.utf8, buffer.utf8_len) };
    unsafe { drop_ffi_slice(buffer.metadata, buffer.metadata_count) };
}

/// Frees keyword search results returned by `retrievalkit_index_keyword_search`.
///
/// # Safety
///
/// Every non-null pointer in `buffer` must have been allocated by RetrievalKit
/// FFI and the buffer must not have been freed before.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_keyword_results_free(
    buffer: RetrievalKitKeywordResultBuffer,
) {
    unsafe { drop_ffi_slice(buffer.hits, buffer.count) };
    unsafe { drop_ffi_slice(buffer.utf8, buffer.utf8_len) };
    unsafe { drop_ffi_slice(buffer.matched_terms, buffer.matched_terms_count) };
    unsafe { drop_ffi_slice(buffer.metadata, buffer.metadata_count) };
}

/// Frees hybrid search results returned by any `*_hybrid_search_alpha` function.
///
/// # Safety
///
/// Every non-null pointer in `buffer` must have been allocated by RetrievalKit
/// FFI and the buffer must not have been freed before.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_hybrid_results_free(buffer: RetrievalKitHybridResultBuffer) {
    unsafe { drop_ffi_slice(buffer.hits, buffer.count) };
    unsafe { drop_ffi_slice(buffer.utf8, buffer.utf8_len) };
    unsafe { drop_ffi_slice(buffer.matched_terms, buffer.matched_terms_count) };
    unsafe { drop_ffi_slice(buffer.metadata, buffer.metadata_count) };
}

/// Builds an equality metadata filter.
///
/// # Safety
///
/// `field` must point to a valid null-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn retrievalkit_filter_equals(
    field: *const c_char,
    value: RetrievalKitMetadataValue,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    value: RetrievalKitMetadataValue,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    lower: *const RetrievalKitMetadataValue,
    upper: *const RetrievalKitMetadataValue,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    values: *const RetrievalKitMetadataValue,
    value_count: usize,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        let values = unsafe { read_metadata_values(values, value_count) }?;
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    filters: *const *const RetrievalKitFilter,
    filter_count: usize,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
    filters: *const *const RetrievalKitFilter,
    filter_count: usize,
    status: *mut RetrievalKitStatus,
) -> *mut RetrievalKitFilter {
    ffi_ptr(status, || {
        Ok(Box::into_raw(Box::new(RetrievalKitFilter {
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
pub unsafe extern "C" fn retrievalkit_filter_free(filter: *mut RetrievalKitFilter) {
    if !filter.is_null() {
        unsafe { drop(Box::from_raw(filter)) };
    }
}

fn ffi_bool<F>(status: *mut RetrievalKitStatus, operation: F) -> bool
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

fn ffi_ptr<T, F>(status: *mut RetrievalKitStatus, operation: F) -> *mut T
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

unsafe fn set_status_ok(status: *mut RetrievalKitStatus) {
    unsafe { retrievalkit_status_clear(status) };
}

unsafe fn set_status_error(status: *mut RetrievalKitStatus, error: FfiError) {
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
            code: RETRIEVALKIT_STATUS_INVALID_ARGUMENT,
            message: message.into(),
        }
    }

    fn core(error: retrievalkit_core::RetrievalKitError) -> Self {
        let code = match &error {
            retrievalkit_core::RetrievalKitError::CorruptIndex { .. } => {
                RETRIEVALKIT_STATUS_CORRUPT_INDEX
            }
            retrievalkit_core::RetrievalKitError::InvalidDimension { .. } => {
                RETRIEVALKIT_STATUS_INVALID_DIMENSION
            }
            retrievalkit_core::RetrievalKitError::MissingEmbedding { .. } => {
                RETRIEVALKIT_STATUS_MISSING_EMBEDDING
            }
            retrievalkit_core::RetrievalKitError::RetrievalCapabilityUnavailable { .. } => {
                RETRIEVALKIT_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE
            }
            retrievalkit_core::RetrievalKitError::InvalidIdentity { .. }
            | retrievalkit_core::RetrievalKitError::InvalidRecordValue { .. } => {
                RETRIEVALKIT_STATUS_INVALID_IDENTITY
            }
            retrievalkit_core::RetrievalKitError::InvalidQuery { .. } => {
                RETRIEVALKIT_STATUS_INVALID_ARGUMENT
            }
            _ => RETRIEVALKIT_STATUS_CORE_ERROR,
        };
        Self {
            code,
            message: error.to_string(),
        }
    }

    fn panic() -> Self {
        Self {
            code: RETRIEVALKIT_STATUS_PANIC,
            message: "RetrievalKit FFI call panicked".to_owned(),
        }
    }

    fn result_buffer_overflow() -> Self {
        Self {
            code: RETRIEVALKIT_STATUS_CORE_ERROR,
            message: "packed result buffer size overflow".to_owned(),
        }
    }

    fn missing_result_payload(result_kind: &str, chunk_id: u64, missing: &str) -> Self {
        Self {
            code: RETRIEVALKIT_STATUS_CORE_ERROR,
            message: format!(
                "{result_kind} referenced internal chunk ID {chunk_id} with no {missing}; reload the database from its last valid snapshot"
            ),
        }
    }
}

impl From<retrievalkit_core::RetrievalKitError> for FfiError {
    fn from(value: retrievalkit_core::RetrievalKitError) -> Self {
        Self::core(value)
    }
}

unsafe fn index_ref<'a>(
    index: *const RetrievalKitIndex,
) -> std::result::Result<&'a RetrievalKitIndex, FfiError> {
    if index.is_null() {
        return Err(FfiError::invalid_argument("index must not be null"));
    }
    Ok(unsafe { &*index })
}

unsafe fn index_mut<'a>(
    index: *mut RetrievalKitIndex,
) -> std::result::Result<&'a mut RetrievalKitIndex, FfiError> {
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
    entries: *const RetrievalKitMetadataEntry,
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
    value: RetrievalKitMetadataValue,
) -> std::result::Result<MetadataValue, FfiError> {
    match value.value_type {
        RETRIEVALKIT_METADATA_STRING => Ok(MetadataValue::String(unsafe {
            read_c_string(value.string_value, "metadata string value")
        }?)),
        RETRIEVALKIT_METADATA_INTEGER => Ok(MetadataValue::Integer(value.integer_value)),
        RETRIEVALKIT_METADATA_FLOAT => Ok(MetadataValue::Float(value.float_value)),
        RETRIEVALKIT_METADATA_BOOLEAN => Ok(MetadataValue::Boolean(value.bool_value)),
        RETRIEVALKIT_METADATA_TIMESTAMP_MILLIS => {
            Ok(MetadataValue::TimestampMillis(value.integer_value))
        }
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported metadata value type {}",
            value.value_type
        ))),
    }
}

unsafe fn optional_metadata_value(
    value: *const RetrievalKitMetadataValue,
) -> std::result::Result<Option<MetadataValue>, FfiError> {
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(unsafe { read_metadata_value(*value) }?))
}

unsafe fn read_metadata_values(
    values: *const RetrievalKitMetadataValue,
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
    chunks: *const RetrievalKitChunkInput,
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

unsafe fn optional_filter(filter: *const RetrievalKitFilter) -> Option<Filter> {
    if filter.is_null() {
        None
    } else {
        Some(unsafe { &*filter }.filter.clone())
    }
}

unsafe fn read_filter_list(
    filters: *const *const RetrievalKitFilter,
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
        RETRIEVALKIT_METRIC_COSINE => Ok(VectorMetric::Cosine),
        RETRIEVALKIT_METRIC_DOT_PRODUCT => Ok(VectorMetric::DotProduct),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported vector metric {metric}"
        ))),
    }
}

fn parse_encoding_code(encoding: u32) -> std::result::Result<VectorEncoding, FfiError> {
    match encoding {
        RETRIEVALKIT_ENCODING_F32 => Ok(VectorEncoding::F32),
        RETRIEVALKIT_ENCODING_F16 => Ok(VectorEncoding::F16),
        RETRIEVALKIT_ENCODING_BF16 => Ok(VectorEncoding::BF16),
        RETRIEVALKIT_ENCODING_I8_SCALAR_QUANTIZED => Ok(VectorEncoding::I8ScalarQuantized),
        _ => Err(FfiError::invalid_argument(format!(
            "unsupported vector encoding {encoding}"
        ))),
    }
}

fn chunk_id_buffer(values: Vec<u64>) -> RetrievalKitChunkIdBuffer {
    let mut values = values.into_boxed_slice();
    let buffer = RetrievalKitChunkIdBuffer {
        values: values.as_mut_ptr(),
        count: values.len(),
    };
    std::mem::forget(values);
    buffer
}

fn text_chunk_buffer(mut chunks: Vec<RetrievalKitTextChunk>) -> RetrievalKitTextChunkBuffer {
    let buffer = RetrievalKitTextChunkBuffer {
        chunks: chunks.as_mut_ptr(),
        count: chunks.len(),
    };
    std::mem::forget(chunks);
    buffer
}

#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "graph"), allow(dead_code))]
pub(crate) enum PackedRecordId<'a> {
    None,
    DocumentId,
    Value(&'a str),
}

#[derive(Clone, Copy)]
pub(crate) struct PackedResultPayload<'a> {
    pub document_id: Option<&'a str>,
    pub record_id: PackedRecordId<'a>,
    pub text: &'a str,
    pub metadata: &'a Metadata,
}

struct PackedUtf8Arena {
    bytes: Vec<u8>,
}

impl PackedUtf8Arena {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, value: &str) -> RetrievalKitUtf8Range {
        let range = RetrievalKitUtf8Range {
            offset: self.bytes.len(),
            length: value.len(),
        };
        self.bytes.extend_from_slice(value.as_bytes());
        range
    }
}

fn checked_result_bytes(total: usize, value: &str) -> std::result::Result<usize, FfiError> {
    total
        .checked_add(value.len())
        .ok_or_else(FfiError::result_buffer_overflow)
}

fn packed_text_size(
    total: usize,
    fallback_document_id: &str,
    value: PackedResultPayload<'_>,
) -> std::result::Result<usize, FfiError> {
    let document_id = value.document_id.unwrap_or(fallback_document_id);
    let total = checked_result_bytes(total, document_id)?;
    let total = match value.record_id {
        PackedRecordId::None => total,
        PackedRecordId::DocumentId => checked_result_bytes(total, document_id)?,
        PackedRecordId::Value(record_id) => checked_result_bytes(total, record_id)?,
    };
    let mut total = checked_result_bytes(total, value.text)?;
    for (key, metadata_value) in value.metadata {
        total = checked_result_bytes(total, key)?;
        if let MetadataValue::String(string_value) = metadata_value {
            total = checked_result_bytes(total, string_value)?;
        }
    }
    Ok(total)
}

fn pack_payload(
    arena: &mut PackedUtf8Arena,
    metadata: &mut Vec<RetrievalKitPackedMetadataEntry>,
    fallback_document_id: &str,
    value: PackedResultPayload<'_>,
) -> (
    RetrievalKitUtf8Range,
    bool,
    RetrievalKitUtf8Range,
    RetrievalKitUtf8Range,
    usize,
    usize,
) {
    let document_id_value = value.document_id.unwrap_or(fallback_document_id);
    let document_id = arena.push(document_id_value);
    let (has_record_id, record_id) = match value.record_id {
        PackedRecordId::None => (false, RetrievalKitUtf8Range::default()),
        PackedRecordId::DocumentId => (true, arena.push(document_id_value)),
        PackedRecordId::Value(record_id) => (true, arena.push(record_id)),
    };
    let text = arena.push(value.text);
    let metadata_start = metadata.len();
    metadata.extend(value.metadata.iter().map(|(key, value)| {
        let mut packed = RetrievalKitPackedMetadataEntry {
            key: arena.push(key),
            ..RetrievalKitPackedMetadataEntry::default()
        };
        match value {
            MetadataValue::String(value) => {
                packed.value_type = RETRIEVALKIT_METADATA_STRING;
                packed.string_value = arena.push(value);
            }
            MetadataValue::Integer(value) => {
                packed.value_type = RETRIEVALKIT_METADATA_INTEGER;
                packed.integer_value = *value;
            }
            MetadataValue::Float(value) => {
                packed.value_type = RETRIEVALKIT_METADATA_FLOAT;
                packed.float_value = *value;
            }
            MetadataValue::Boolean(value) => {
                packed.value_type = RETRIEVALKIT_METADATA_BOOLEAN;
                packed.bool_value = *value;
            }
            MetadataValue::TimestampMillis(value) => {
                packed.value_type = RETRIEVALKIT_METADATA_TIMESTAMP_MILLIS;
                packed.integer_value = *value;
            }
        }
        packed
    }));
    (
        document_id,
        has_record_id,
        record_id,
        text,
        metadata_start,
        value.metadata.len(),
    )
}

fn into_ffi_slice<T>(values: Vec<T>) -> (*const T, usize) {
    if values.is_empty() {
        return (ptr::null(), 0);
    }
    let values = values.into_boxed_slice();
    let count = values.len();
    let values = Box::into_raw(values);
    (values.cast::<T>() as *const T, count)
}

unsafe fn drop_ffi_slice<T>(values: *const T, count: usize) {
    if values.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            values as *mut T,
            count,
        )));
    }
}

pub(crate) fn empty_search_result_buffer() -> RetrievalKitSearchResultBuffer {
    RetrievalKitSearchResultBuffer {
        hits: ptr::null(),
        count: 0,
        utf8: ptr::null(),
        utf8_len: 0,
        metadata: ptr::null(),
        metadata_count: 0,
    }
}

pub(crate) fn empty_keyword_result_buffer() -> RetrievalKitKeywordResultBuffer {
    RetrievalKitKeywordResultBuffer {
        hits: ptr::null(),
        count: 0,
        utf8: ptr::null(),
        utf8_len: 0,
        matched_terms: ptr::null(),
        matched_terms_count: 0,
        metadata: ptr::null(),
        metadata_count: 0,
    }
}

pub(crate) fn empty_hybrid_result_buffer() -> RetrievalKitHybridResultBuffer {
    RetrievalKitHybridResultBuffer {
        hits: ptr::null(),
        count: 0,
        utf8: ptr::null(),
        utf8_len: 0,
        matched_terms: ptr::null(),
        matched_terms_count: 0,
        metadata: ptr::null(),
        metadata_count: 0,
        alpha: 0.0,
    }
}

pub(crate) fn packed_search_result_buffer<'a, F>(
    hits: Vec<SearchHit>,
    resolve: F,
) -> std::result::Result<RetrievalKitSearchResultBuffer, FfiError>
where
    F: Fn(u64) -> std::result::Result<PackedResultPayload<'a>, FfiError>,
{
    let payloads = hits
        .iter()
        .map(|hit| resolve(hit.chunk_id))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let utf8_capacity = hits
        .iter()
        .zip(&payloads)
        .try_fold(0, |total, (hit, payload)| {
            packed_text_size(total, &hit.document_id, *payload)
        })?;
    let metadata_capacity = payloads.iter().try_fold(0usize, |total, payload| {
        total
            .checked_add(payload.metadata.len())
            .ok_or_else(FfiError::result_buffer_overflow)
    })?;
    let mut arena = PackedUtf8Arena::with_capacity(utf8_capacity);
    let mut metadata = Vec::with_capacity(metadata_capacity);
    let mut packed_hits = Vec::with_capacity(hits.len());
    for (hit, payload) in hits.iter().zip(payloads) {
        let (document_id, has_record_id, record_id, text, metadata_start, metadata_count) =
            pack_payload(&mut arena, &mut metadata, &hit.document_id, payload);
        packed_hits.push(RetrievalKitSearchHit {
            chunk_id: hit.chunk_id,
            document_id,
            has_record_id,
            record_id,
            text,
            score: hit.score,
            vector_score: hit.trace.vector_score,
            metadata_start,
            metadata_count,
        });
    }
    debug_assert_eq!(arena.bytes.len(), utf8_capacity);
    debug_assert_eq!(metadata.len(), metadata_capacity);
    let (hits, count) = into_ffi_slice(packed_hits);
    let (utf8, utf8_len) = into_ffi_slice(arena.bytes);
    let (metadata, metadata_count) = into_ffi_slice(metadata);
    Ok(RetrievalKitSearchResultBuffer {
        hits,
        count,
        utf8,
        utf8_len,
        metadata,
        metadata_count,
    })
}

pub(crate) fn packed_keyword_result_buffer<'a, F>(
    hits: Vec<KeywordHit>,
    resolve: F,
) -> std::result::Result<RetrievalKitKeywordResultBuffer, FfiError>
where
    F: Fn(u64) -> std::result::Result<PackedResultPayload<'a>, FfiError>,
{
    let payloads = hits
        .iter()
        .map(|hit| resolve(hit.chunk_id))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut utf8_capacity = 0;
    let mut term_capacity: usize = 0;
    let mut metadata_capacity: usize = 0;
    for (hit, payload) in hits.iter().zip(&payloads) {
        utf8_capacity = packed_text_size(utf8_capacity, &hit.document_id, *payload)?;
        term_capacity = term_capacity
            .checked_add(hit.matched_terms.len())
            .ok_or_else(FfiError::result_buffer_overflow)?;
        metadata_capacity = metadata_capacity
            .checked_add(payload.metadata.len())
            .ok_or_else(FfiError::result_buffer_overflow)?;
        for term in &hit.matched_terms {
            utf8_capacity = checked_result_bytes(utf8_capacity, term)?;
        }
    }

    let mut arena = PackedUtf8Arena::with_capacity(utf8_capacity);
    let mut matched_terms = Vec::with_capacity(term_capacity);
    let mut metadata = Vec::with_capacity(metadata_capacity);
    let mut packed_hits = Vec::with_capacity(hits.len());
    for (hit, payload) in hits.iter().zip(payloads) {
        let (document_id, has_record_id, record_id, text, metadata_start, metadata_count) =
            pack_payload(&mut arena, &mut metadata, &hit.document_id, payload);
        let matched_terms_start = matched_terms.len();
        matched_terms.extend(hit.matched_terms.iter().map(|term| arena.push(term)));
        packed_hits.push(RetrievalKitKeywordHit {
            chunk_id: hit.chunk_id,
            document_id,
            has_record_id,
            record_id,
            text,
            score: hit.score,
            matched_terms_start,
            matched_terms_count: hit.matched_terms.len(),
            metadata_start,
            metadata_count,
        });
    }
    debug_assert_eq!(arena.bytes.len(), utf8_capacity);
    debug_assert_eq!(matched_terms.len(), term_capacity);
    debug_assert_eq!(metadata.len(), metadata_capacity);
    let (hits, count) = into_ffi_slice(packed_hits);
    let (utf8, utf8_len) = into_ffi_slice(arena.bytes);
    let (matched_terms, matched_terms_count) = into_ffi_slice(matched_terms);
    let (metadata, metadata_count) = into_ffi_slice(metadata);
    Ok(RetrievalKitKeywordResultBuffer {
        hits,
        count,
        utf8,
        utf8_len,
        matched_terms,
        matched_terms_count,
        metadata,
        metadata_count,
    })
}

pub(crate) fn packed_hybrid_result_buffer<'a, F>(
    hits: Vec<HybridHit>,
    alpha: f32,
    resolve: F,
) -> std::result::Result<RetrievalKitHybridResultBuffer, FfiError>
where
    F: Fn(u64) -> std::result::Result<PackedResultPayload<'a>, FfiError>,
{
    let payloads = hits
        .iter()
        .map(|hit| resolve(hit.chunk_id))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut utf8_capacity = 0;
    let mut term_capacity: usize = 0;
    let mut metadata_capacity: usize = 0;
    for (hit, payload) in hits.iter().zip(&payloads) {
        utf8_capacity = packed_text_size(utf8_capacity, &hit.document_id, *payload)?;
        term_capacity = term_capacity
            .checked_add(hit.trace.matched_terms.len())
            .ok_or_else(FfiError::result_buffer_overflow)?;
        metadata_capacity = metadata_capacity
            .checked_add(payload.metadata.len())
            .ok_or_else(FfiError::result_buffer_overflow)?;
        for term in &hit.trace.matched_terms {
            utf8_capacity = checked_result_bytes(utf8_capacity, term)?;
        }
    }

    let mut arena = PackedUtf8Arena::with_capacity(utf8_capacity);
    let mut matched_terms = Vec::with_capacity(term_capacity);
    let mut metadata = Vec::with_capacity(metadata_capacity);
    let mut packed_hits = Vec::with_capacity(hits.len());
    for (hit, payload) in hits.iter().zip(payloads) {
        let (document_id, has_record_id, record_id, text, metadata_start, metadata_count) =
            pack_payload(&mut arena, &mut metadata, &hit.document_id, payload);
        let matched_terms_start = matched_terms.len();
        matched_terms.extend(hit.trace.matched_terms.iter().map(|term| arena.push(term)));
        packed_hits.push(RetrievalKitHybridHit {
            chunk_id: hit.chunk_id,
            document_id,
            has_record_id,
            record_id,
            text,
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
            matched_terms_start,
            matched_terms_count: hit.trace.matched_terms.len(),
            metadata_start,
            metadata_count,
        });
    }
    debug_assert_eq!(arena.bytes.len(), utf8_capacity);
    debug_assert_eq!(matched_terms.len(), term_capacity);
    debug_assert_eq!(metadata.len(), metadata_capacity);
    let (hits, count) = into_ffi_slice(packed_hits);
    let (utf8, utf8_len) = into_ffi_slice(arena.bytes);
    let (matched_terms, matched_terms_count) = into_ffi_slice(matched_terms);
    let (metadata, metadata_count) = into_ffi_slice(metadata);
    Ok(RetrievalKitHybridResultBuffer {
        hits,
        count,
        utf8,
        utf8_len,
        matched_terms,
        matched_terms_count,
        metadata,
        metadata_count,
        alpha,
    })
}

fn search_result_buffer(
    index: &RetrievalKitIndex,
    hits: Vec<SearchHit>,
) -> std::result::Result<RetrievalKitSearchResultBuffer, FfiError> {
    packed_search_result_buffer(hits, |chunk_id| {
        let chunk = index
            .index
            .chunk(chunk_id)
            .ok_or_else(|| FfiError::missing_result_payload("search result", chunk_id, "chunk"))?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn retrieval_search_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<SearchHit>,
) -> std::result::Result<RetrievalKitSearchResultBuffer, FfiError> {
    packed_search_result_buffer(hits, |chunk_id| {
        let chunk = database.chunk(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("retrieval result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn keyword_result_buffer(
    index: &RetrievalKitIndex,
    hits: Vec<KeywordHit>,
) -> std::result::Result<RetrievalKitKeywordResultBuffer, FfiError> {
    packed_keyword_result_buffer(hits, |chunk_id| {
        let chunk = index
            .index
            .chunk(chunk_id)
            .ok_or_else(|| FfiError::missing_result_payload("keyword result", chunk_id, "chunk"))?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn retrieval_keyword_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<KeywordHit>,
) -> std::result::Result<RetrievalKitKeywordResultBuffer, FfiError> {
    packed_keyword_result_buffer(hits, |chunk_id| {
        let chunk = database.chunk(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("retrieval keyword result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn hybrid_result_buffer(
    index: &RetrievalKitIndex,
    hits: Vec<HybridHit>,
    alpha: f32,
) -> std::result::Result<RetrievalKitHybridResultBuffer, FfiError> {
    packed_hybrid_result_buffer(hits, alpha, |chunk_id| {
        let chunk = index
            .index
            .chunk(chunk_id)
            .ok_or_else(|| FfiError::missing_result_payload("hybrid result", chunk_id, "chunk"))?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn retrieval_hybrid_result_buffer(
    database: &RetrievalDatabase,
    hits: Vec<HybridHit>,
    alpha: f32,
) -> std::result::Result<RetrievalKitHybridResultBuffer, FfiError> {
    packed_hybrid_result_buffer(hits, alpha, |chunk_id| {
        let chunk = database.chunk(chunk_id).ok_or_else(|| {
            FfiError::missing_result_payload("retrieval hybrid result", chunk_id, "chunk")
        })?;
        Ok(PackedResultPayload {
            document_id: None,
            record_id: PackedRecordId::None,
            text: &chunk.text,
            metadata: &chunk.metadata,
        })
    })
}

fn string_to_owned_ptr(value: &str) -> *mut c_char {
    json_to_c_string(value)
}

#[cfg(feature = "graph")]
fn string_array(values: Vec<String>) -> RetrievalKitStringArray {
    let mut pointers = values
        .into_iter()
        .map(|value| string_to_owned_ptr(&value))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let array = RetrievalKitStringArray {
        values: pointers.as_mut_ptr(),
        count: pointers.len(),
    };
    std::mem::forget(pointers);
    array
}

#[cfg(feature = "graph")]
unsafe fn string_array_free(array: RetrievalKitStringArray) {
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
    fn runtime_capability_json_reports_dispatch_state() {
        let pointer = retrievalkit_runtime_capabilities_json();
        assert!(!pointer.is_null());
        let raw = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .unwrap()
            .to_owned();
        unsafe { retrievalkit_string_free(pointer) };
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(value["simsimd"].is_string());
        assert!(value["aarch64_dotprod"].is_boolean());
    }

    #[test]
    fn index_handle_contents_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RetrievalKitIndex>();
    }

    #[test]
    fn chunking_ffi_returns_owned_text_and_offsets() {
        let text = CString::new("abçdef").unwrap();
        let mut status = RetrievalKitStatus {
            code: -1,
            message: ptr::null_mut(),
        };
        let mut output = RetrievalKitTextChunkBuffer {
            chunks: ptr::null_mut(),
            count: 0,
        };

        let success = unsafe {
            retrievalkit_chunk_text(
                text.as_ptr(),
                RETRIEVALKIT_CHUNKING_FIXED,
                4,
                1,
                &mut output,
                &mut status,
            )
        };

        assert!(success);
        assert_eq!(status.code, RETRIEVALKIT_STATUS_OK);
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
        let index = unsafe {
            retrievalkit_index_new(
                2,
                RETRIEVALKIT_METRIC_COSINE,
                RETRIEVALKIT_ENCODING_F32,
                &mut status,
            )
        };
        assert!(!index.is_null());
        assert_status_ok(&status);

        let document_id = CString::new("doc-1").unwrap();
        let document_text = CString::new("document text").unwrap();
        let bucket_field = CString::new("bucket").unwrap();
        let chunk_text_keep = CString::new("keep").unwrap();
        let chunk_text_skip = CString::new("skip").unwrap();
        let keep_embedding = [1.0_f32, 0.0];
        let skip_embedding = [0.0_f32, 1.0];
        let keep_metadata = [RetrievalKitMetadataEntry {
            field: bucket_field.as_ptr(),
            value: RetrievalKitMetadataValue {
                value_type: RETRIEVALKIT_METADATA_INTEGER,
                string_value: ptr::null(),
                integer_value: 1,
                float_value: 0.0,
                bool_value: false,
            },
        }];
        let skip_metadata = [RetrievalKitMetadataEntry {
            field: bucket_field.as_ptr(),
            value: RetrievalKitMetadataValue {
                value_type: RETRIEVALKIT_METADATA_INTEGER,
                string_value: ptr::null(),
                integer_value: 2,
                float_value: 0.0,
                bool_value: false,
            },
        }];
        let chunks = [
            RetrievalKitChunkInput {
                text: chunk_text_keep.as_ptr(),
                embedding: keep_embedding.as_ptr(),
                embedding_len: keep_embedding.len(),
                metadata: keep_metadata.as_ptr(),
                metadata_len: keep_metadata.len(),
            },
            RetrievalKitChunkInput {
                text: chunk_text_skip.as_ptr(),
                embedding: skip_embedding.as_ptr(),
                embedding_len: skip_embedding.len(),
                metadata: skip_metadata.as_ptr(),
                metadata_len: skip_metadata.len(),
            },
        ];
        let mut chunk_ids = RetrievalKitChunkIdBuffer {
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

        let filter_value = RetrievalKitMetadataValue {
            value_type: RETRIEVALKIT_METADATA_INTEGER,
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
        let mut results = empty_search_result_buffer();
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
        assert_eq!(unsafe { packed_string(&results, hit.text) }, "skip");
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

        let mut compaction = RetrievalKitCompactionReport::default();
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
        let index = unsafe {
            retrievalkit_index_new(
                2,
                RETRIEVALKIT_METRIC_COSINE,
                RETRIEVALKIT_ENCODING_F32,
                &mut status,
            )
        };
        assert!(!index.is_null());

        let query = [1.0_f32];
        let mut results = empty_search_result_buffer();
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
        assert_eq!(status.code, RETRIEVALKIT_STATUS_INVALID_DIMENSION);
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
        let index = unsafe {
            retrievalkit_index_new(
                2,
                RETRIEVALKIT_METRIC_COSINE,
                RETRIEVALKIT_ENCODING_F32,
                &mut status,
            )
        };
        assert!(!index.is_null());

        let document_id = CString::new("doc-1").unwrap();
        let document_text = CString::new("").unwrap();
        let chunk_text = CString::new("persisted").unwrap();
        let embedding = [0.0_f32, 1.0];
        let chunks = [RetrievalKitChunkInput {
            text: chunk_text.as_ptr(),
            embedding: embedding.as_ptr(),
            embedding_len: embedding.len(),
            metadata: ptr::null(),
            metadata_len: 0,
        }];
        let mut chunk_ids = RetrievalKitChunkIdBuffer {
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

        let mut results = empty_search_result_buffer();
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
        assert_eq!(unsafe { packed_string(&results, hit.text) }, "persisted");

        unsafe {
            retrievalkit_search_results_free(results);
            retrievalkit_index_free(loaded);
            retrievalkit_index_free(index);
            retrievalkit_status_clear(&mut status);
        }
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn packed_result_buffers_round_trip_utf8_identities_terms_and_traces() {
        let long_text = "ö".repeat(32 * 1024);
        let mut first_metadata = Metadata::new();
        first_metadata.insert(
            "başlık".to_owned(),
            MetadataValue::String("Swift için".to_owned()),
        );
        first_metadata.insert("count".to_owned(), MetadataValue::Integer(7));
        first_metadata.insert("weight".to_owned(), MetadataValue::Float(2.5));
        first_metadata.insert("active".to_owned(), MetadataValue::Boolean(true));
        first_metadata.insert(
            "created_at".to_owned(),
            MetadataValue::TimestampMillis(1_700_000_000_000),
        );
        let empty_metadata = Metadata::new();
        let search = packed_search_result_buffer(
            vec![
                SearchHit {
                    chunk_id: 1,
                    document_id: "fallback-1".to_owned(),
                    score: 0.9,
                    trace: retrievalkit_core::SearchTrace { vector_score: 0.8 },
                },
                SearchHit {
                    chunk_id: 2,
                    document_id: "fallback-2".to_owned(),
                    score: 0.7,
                    trace: retrievalkit_core::SearchTrace { vector_score: 0.6 },
                },
            ],
            |chunk_id| {
                Ok(if chunk_id == 1 {
                    PackedResultPayload {
                        document_id: Some("belge-ğ"),
                        record_id: PackedRecordId::Value("kayıt-1"),
                        text: "Swift için özel metin",
                        metadata: &first_metadata,
                    }
                } else {
                    PackedResultPayload {
                        document_id: None,
                        record_id: PackedRecordId::None,
                        text: &long_text,
                        metadata: &empty_metadata,
                    }
                })
            },
        )
        .unwrap();
        assert_eq!(search.count, 2);
        let search_hits = unsafe { slice::from_raw_parts(search.hits, search.count) };
        assert_eq!(
            unsafe { packed_utf8(search.utf8, search.utf8_len, search_hits[0].document_id) },
            "belge-ğ"
        );
        assert!(search_hits[0].has_record_id);
        assert_eq!(
            unsafe { packed_utf8(search.utf8, search.utf8_len, search_hits[0].record_id) },
            "kayıt-1"
        );
        assert_eq!(
            unsafe { packed_utf8(search.utf8, search.utf8_len, search_hits[0].text) },
            "Swift için özel metin"
        );
        assert!(!search_hits[1].has_record_id);
        assert_eq!(
            unsafe { packed_utf8(search.utf8, search.utf8_len, search_hits[1].document_id) },
            "fallback-2"
        );
        assert_eq!(
            unsafe { packed_utf8(search.utf8, search.utf8_len, search_hits[1].text) },
            long_text
        );
        assert_eq!(search_hits[0].metadata_start, 0);
        assert_eq!(search_hits[0].metadata_count, 5);
        assert_eq!(search_hits[1].metadata_count, 0);
        assert_eq!(search.metadata_count, 5);
        let metadata = unsafe { slice::from_raw_parts(search.metadata, search.metadata_count) };
        for entry in metadata {
            let key = unsafe { packed_utf8(search.utf8, search.utf8_len, entry.key) };
            match key.as_str() {
                "başlık" => {
                    assert_eq!(entry.value_type, RETRIEVALKIT_METADATA_STRING);
                    assert_eq!(
                        unsafe { packed_utf8(search.utf8, search.utf8_len, entry.string_value) },
                        "Swift için"
                    );
                }
                "count" => {
                    assert_eq!(entry.value_type, RETRIEVALKIT_METADATA_INTEGER);
                    assert_eq!(entry.integer_value, 7);
                }
                "weight" => {
                    assert_eq!(entry.value_type, RETRIEVALKIT_METADATA_FLOAT);
                    assert_eq!(entry.float_value, 2.5);
                }
                "active" => {
                    assert_eq!(entry.value_type, RETRIEVALKIT_METADATA_BOOLEAN);
                    assert!(entry.bool_value);
                }
                "created_at" => {
                    assert_eq!(entry.value_type, RETRIEVALKIT_METADATA_TIMESTAMP_MILLIS);
                    assert_eq!(entry.integer_value, 1_700_000_000_000);
                }
                unexpected => panic!("unexpected metadata key {unexpected}"),
            }
        }
        unsafe { retrievalkit_search_results_free(search) };

        let keyword = packed_keyword_result_buffer(
            vec![KeywordHit {
                chunk_id: 3,
                document_id: "note-3".to_owned(),
                score: 4.2,
                matched_terms: vec!["swift".to_owned(), "özel".to_owned()],
            }],
            |_| {
                Ok(PackedResultPayload {
                    document_id: None,
                    record_id: PackedRecordId::None,
                    text: "Swift özel arama",
                    metadata: &first_metadata,
                })
            },
        )
        .unwrap();
        assert_eq!(keyword.matched_terms_count, 2);
        let keyword_hit = unsafe { &*keyword.hits };
        assert_eq!(keyword_hit.matched_terms_start, 0);
        assert_eq!(keyword_hit.matched_terms_count, 2);
        let terms =
            unsafe { slice::from_raw_parts(keyword.matched_terms, keyword.matched_terms_count) };
        assert_eq!(
            unsafe { packed_utf8(keyword.utf8, keyword.utf8_len, terms[0]) },
            "swift"
        );
        assert_eq!(
            unsafe { packed_utf8(keyword.utf8, keyword.utf8_len, terms[1]) },
            "özel"
        );
        unsafe { retrievalkit_keyword_results_free(keyword) };

        let hybrid = packed_hybrid_result_buffer(
            vec![HybridHit {
                chunk_id: 4,
                document_id: "note-4".to_owned(),
                score: 0.75,
                vector_score: Some(0.8),
                keyword_score: Some(3.0),
                trace: retrievalkit_core::HybridTrace {
                    vector_rank: Some(1),
                    keyword_rank: Some(2),
                    normalized_vector_score: Some(1.0),
                    normalized_keyword_score: Some(0.5),
                    matched_terms: vec!["arama".to_owned()],
                    fusion: retrievalkit_core::HybridFusionTrace::WeightedNormalizedScore {
                        vector_weight: 0.6,
                        keyword_weight: 0.4,
                    },
                },
            }],
            0.6,
            |_| {
                Ok(PackedResultPayload {
                    document_id: None,
                    record_id: PackedRecordId::DocumentId,
                    text: "hybrid arama",
                    metadata: &first_metadata,
                })
            },
        )
        .unwrap();
        let hybrid_hit = unsafe { &*hybrid.hits };
        assert!(hybrid_hit.has_record_id);
        assert!(hybrid_hit.has_vector_score);
        assert!(hybrid_hit.has_keyword_score);
        assert_eq!(hybrid_hit.vector_rank, 1);
        assert_eq!(hybrid_hit.keyword_rank, 2);
        assert_eq!(hybrid_hit.matched_terms_count, 1);
        assert_eq!(
            unsafe { packed_utf8(hybrid.utf8, hybrid.utf8_len, hybrid_hit.record_id) },
            "note-4"
        );
        assert_eq!(hybrid.alpha, 0.6);
        assert_eq!(hybrid_hit.metadata_count, 5);
        unsafe { retrievalkit_hybrid_results_free(hybrid) };
    }

    #[test]
    fn empty_packed_result_buffers_are_null_and_freeable() {
        let search = empty_search_result_buffer();
        assert!(search.hits.is_null());
        assert!(search.utf8.is_null());
        assert!(search.metadata.is_null());
        unsafe { retrievalkit_search_results_free(search) };

        let keyword = empty_keyword_result_buffer();
        assert!(keyword.hits.is_null());
        assert!(keyword.utf8.is_null());
        assert!(keyword.matched_terms.is_null());
        assert!(keyword.metadata.is_null());
        unsafe { retrievalkit_keyword_results_free(keyword) };

        let hybrid = empty_hybrid_result_buffer();
        assert!(hybrid.hits.is_null());
        assert!(hybrid.utf8.is_null());
        assert!(hybrid.matched_terms.is_null());
        assert!(hybrid.metadata.is_null());
        assert_eq!(hybrid.alpha, 0.0);
        unsafe { retrievalkit_hybrid_results_free(hybrid) };
    }

    fn empty_status() -> RetrievalKitStatus {
        RetrievalKitStatus {
            code: RETRIEVALKIT_STATUS_OK,
            message: ptr::null_mut(),
        }
    }

    fn assert_status_ok(status: &RetrievalKitStatus) {
        assert_eq!(status.code, RETRIEVALKIT_STATUS_OK);
        assert!(status.message.is_null());
    }

    unsafe fn packed_string(
        buffer: &RetrievalKitSearchResultBuffer,
        range: RetrievalKitUtf8Range,
    ) -> String {
        unsafe { packed_utf8(buffer.utf8, buffer.utf8_len, range) }
    }

    unsafe fn packed_utf8(
        utf8: *const u8,
        utf8_len: usize,
        range: RetrievalKitUtf8Range,
    ) -> String {
        assert!(range.offset <= utf8_len);
        assert!(range.length <= utf8_len - range.offset);
        let bytes = unsafe { slice::from_raw_parts(utf8.add(range.offset), range.length) };
        std::str::from_utf8(bytes).unwrap().to_owned()
    }
}
