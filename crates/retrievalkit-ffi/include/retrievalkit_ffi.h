#ifndef RETRIEVALKIT_FFI_H
#define RETRIEVALKIT_FFI_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RETRIEVALKIT_STATUS_OK 0
#define RETRIEVALKIT_STATUS_INVALID_ARGUMENT 1
#define RETRIEVALKIT_STATUS_CORE_ERROR 2
#define RETRIEVALKIT_STATUS_PANIC 3
#define RETRIEVALKIT_STATUS_CORRUPT_INDEX 4

#define RETRIEVALKIT_METRIC_COSINE 0
#define RETRIEVALKIT_METRIC_DOT_PRODUCT 1

#define RETRIEVALKIT_ENCODING_F32 0
#define RETRIEVALKIT_ENCODING_F16 1
#define RETRIEVALKIT_ENCODING_BF16 2
#define RETRIEVALKIT_ENCODING_I8_SCALAR_QUANTIZED 3

#define RETRIEVALKIT_METADATA_STRING 0
#define RETRIEVALKIT_METADATA_INTEGER 1
#define RETRIEVALKIT_METADATA_FLOAT 2
#define RETRIEVALKIT_METADATA_BOOLEAN 3
#define RETRIEVALKIT_METADATA_TIMESTAMP_MILLIS 4

#define RETRIEVALKIT_CHUNKING_FIXED 0
#define RETRIEVALKIT_CHUNKING_SENTENCE 1

typedef struct RetrievalKitIndex RetrievalKitIndex;
typedef struct RetrievalKitFilter RetrievalKitFilter;

typedef struct RetrievalKitStatus {
  int32_t code;
  char *message;
} RetrievalKitStatus;

typedef struct RetrievalKitCompactionReport {
  size_t chunks_before;
  size_t chunks_after;
  size_t chunks_removed;
  size_t estimated_bytes_before;
  size_t estimated_bytes_after;
  size_t estimated_bytes_reclaimed;
} RetrievalKitCompactionReport;

typedef struct RetrievalKitMetadataValue {
  uint32_t value_type;
  const char *string_value;
  int64_t integer_value;
  double float_value;
  bool bool_value;
} RetrievalKitMetadataValue;

typedef struct RetrievalKitMetadataEntry {
  const char *field;
  RetrievalKitMetadataValue value;
} RetrievalKitMetadataEntry;

typedef struct RetrievalKitChunkInput {
  const char *text;
  const float *embedding;
  size_t embedding_len;
  const RetrievalKitMetadataEntry *metadata;
  size_t metadata_len;
} RetrievalKitChunkInput;

typedef struct RetrievalKitChunkIdBuffer {
  uint64_t *values;
  size_t count;
} RetrievalKitChunkIdBuffer;

typedef struct RetrievalKitTextChunk {
  char *text;
  size_t start_byte;
  size_t end_byte;
} RetrievalKitTextChunk;

typedef struct RetrievalKitTextChunkBuffer {
  RetrievalKitTextChunk *chunks;
  size_t count;
} RetrievalKitTextChunkBuffer;

typedef struct RetrievalKitUtf8Range {
  size_t offset;
  size_t length;
} RetrievalKitUtf8Range;

typedef struct RetrievalKitPackedMetadataEntry {
  RetrievalKitUtf8Range key;
  uint32_t value_type;
  RetrievalKitUtf8Range string_value;
  int64_t integer_value;
  double float_value;
  bool bool_value;
} RetrievalKitPackedMetadataEntry;

typedef struct RetrievalKitSearchHit {
  uint64_t chunk_id;
  RetrievalKitUtf8Range document_id;
  bool has_record_id;
  RetrievalKitUtf8Range record_id;
  RetrievalKitUtf8Range text;
  float score;
  float vector_score;
  size_t metadata_start;
  size_t metadata_count;
} RetrievalKitSearchHit;

typedef struct RetrievalKitSearchResultBuffer {
  const RetrievalKitSearchHit *hits;
  size_t count;
  const uint8_t *utf8;
  size_t utf8_len;
  const RetrievalKitPackedMetadataEntry *metadata;
  size_t metadata_count;
} RetrievalKitSearchResultBuffer;

typedef struct RetrievalKitStringArray {
  char **values;
  size_t count;
} RetrievalKitStringArray;

typedef struct RetrievalKitKeywordHit {
  uint64_t chunk_id;
  RetrievalKitUtf8Range document_id;
  bool has_record_id;
  RetrievalKitUtf8Range record_id;
  RetrievalKitUtf8Range text;
  float score;
  size_t matched_terms_start;
  size_t matched_terms_count;
  size_t metadata_start;
  size_t metadata_count;
} RetrievalKitKeywordHit;

typedef struct RetrievalKitKeywordResultBuffer {
  const RetrievalKitKeywordHit *hits;
  size_t count;
  const uint8_t *utf8;
  size_t utf8_len;
  const RetrievalKitUtf8Range *matched_terms;
  size_t matched_terms_count;
  const RetrievalKitPackedMetadataEntry *metadata;
  size_t metadata_count;
} RetrievalKitKeywordResultBuffer;

typedef struct RetrievalKitHybridHit {
  uint64_t chunk_id;
  RetrievalKitUtf8Range document_id;
  bool has_record_id;
  RetrievalKitUtf8Range record_id;
  RetrievalKitUtf8Range text;
  float score;
  bool has_vector_score;
  float vector_score;
  bool has_keyword_score;
  float keyword_score;
  bool has_vector_rank;
  size_t vector_rank;
  bool has_keyword_rank;
  size_t keyword_rank;
  bool has_normalized_vector_score;
  float normalized_vector_score;
  bool has_normalized_keyword_score;
  float normalized_keyword_score;
  size_t matched_terms_start;
  size_t matched_terms_count;
  size_t metadata_start;
  size_t metadata_count;
} RetrievalKitHybridHit;

typedef struct RetrievalKitHybridResultBuffer {
  const RetrievalKitHybridHit *hits;
  size_t count;
  const uint8_t *utf8;
  size_t utf8_len;
  const RetrievalKitUtf8Range *matched_terms;
  size_t matched_terms_count;
  const RetrievalKitPackedMetadataEntry *metadata;
  size_t metadata_count;
  float alpha;
} RetrievalKitHybridResultBuffer;

typedef struct RetrievalKitHybridQueryOptions {
  size_t vector_top_k;
  size_t keyword_top_k;
  float alpha;
} RetrievalKitHybridQueryOptions;

#define RETRIEVALKIT_STATUS_INVALID_DIMENSION 5
#define RETRIEVALKIT_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE 6
#define RETRIEVALKIT_STATUS_INVALID_IDENTITY 7
#define RETRIEVALKIT_STATUS_MISSING_EMBEDDING 8

typedef struct RetrievalKitRetrievalBuilder RetrievalKitRetrievalBuilder;
typedef struct RetrievalKitRetrievalDatabase RetrievalKitRetrievalDatabase;

void retrievalkit_status_clear(RetrievalKitStatus *status);

RetrievalKitRetrievalBuilder *retrievalkit_retrieval_builder_new(
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    RetrievalKitStatus *status);
RetrievalKitRetrievalBuilder *retrievalkit_retrieval_builder_new_with_bm25(
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    float bm25_k1,
    float bm25_b,
    const char *stop_words_json,
    RetrievalKitStatus *status);
bool retrievalkit_retrieval_builder_upsert_document(
    RetrievalKitRetrievalBuilder *builder,
    const char *document_id,
    const char *text,
    const RetrievalKitMetadataEntry *metadata,
    size_t metadata_len,
    const float *embedding,
    size_t embedding_len,
    RetrievalKitStatus *status);
bool retrievalkit_retrieval_builder_upsert_record_json(
    RetrievalKitRetrievalBuilder *builder,
    const char *record_json,
    RetrievalKitStatus *status);
RetrievalKitRetrievalDatabase *retrievalkit_retrieval_builder_build(
    RetrievalKitRetrievalBuilder *builder,
    RetrievalKitStatus *status);
void retrievalkit_retrieval_builder_free(RetrievalKitRetrievalBuilder *builder);

RetrievalKitRetrievalDatabase *retrievalkit_retrieval_database_load(const char *directory, RetrievalKitStatus *status);
bool retrievalkit_retrieval_database_save(const RetrievalKitRetrievalDatabase *database, const char *directory, RetrievalKitStatus *status);
bool retrievalkit_retrieval_database_validate(const char *directory, RetrievalKitStatus *status);
void retrievalkit_retrieval_database_free(RetrievalKitRetrievalDatabase *database);
bool retrievalkit_retrieval_semantic_search(
    const RetrievalKitRetrievalDatabase *database,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitSearchResultBuffer *out_results,
    RetrievalKitStatus *status);
bool retrievalkit_retrieval_keyword_search(
    const RetrievalKitRetrievalDatabase *database,
    const char *text,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitKeywordResultBuffer *out_results,
    RetrievalKitStatus *status);
bool retrievalkit_retrieval_hybrid_search_alpha(
    const RetrievalKitRetrievalDatabase *database,
    const char *text,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitHybridQueryOptions options,
    RetrievalKitHybridResultBuffer *out_results,
    RetrievalKitStatus *status);

/*
 * Threading contract for one RetrievalKitIndex handle:
 *
 * - dimension/count accessors and exact, keyword, and hybrid search may run
 *   concurrently after construction or loading.
 * - save, upsert, delete, compaction, and free require exclusive access.
 * - callers must not start new reads while an exclusive operation is waiting.
 * - every concurrent call must use independent RetrievalKitStatus, output buffers, and
 *   RetrievalKitFilter handles. Output buffers may be freed independently.
 * - the handle must remain alive until every call using it has returned.
 *
 * The C ABI does not add locks. Callers are responsible for enforcing this
 * contract; the Swift wrapper provides a writer-preferring asynchronous gate.
 */

RetrievalKitIndex *retrievalkit_index_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    RetrievalKitStatus *status);

RetrievalKitIndex *retrievalkit_index_load(const char *directory, RetrievalKitStatus *status);

bool retrievalkit_index_validate(const char *directory, RetrievalKitStatus *status);
void retrievalkit_index_free(RetrievalKitIndex *index);

bool retrievalkit_index_save(
    RetrievalKitIndex *index,
    const char *directory,
    bool include_bm25,
    RetrievalKitStatus *status);

size_t retrievalkit_index_dimension(const RetrievalKitIndex *index);
size_t retrievalkit_index_active_chunk_count(const RetrievalKitIndex *index);
size_t retrievalkit_index_total_chunk_count(const RetrievalKitIndex *index);
size_t retrievalkit_index_tombstoned_chunk_count(const RetrievalKitIndex *index);

bool retrievalkit_chunk_text(
    const char *text,
    uint32_t strategy,
    size_t max_characters,
    size_t overlap_characters,
    RetrievalKitTextChunkBuffer *out_chunks,
    RetrievalKitStatus *status);

bool retrievalkit_index_upsert_document(
    RetrievalKitIndex *index,
    const char *document_id,
    const char *document_text,
    const RetrievalKitMetadataEntry *document_metadata,
    size_t document_metadata_len,
    const RetrievalKitChunkInput *chunks,
    size_t chunk_count,
    RetrievalKitChunkIdBuffer *out_chunk_ids,
    RetrievalKitStatus *status);

bool retrievalkit_index_delete_document(
    RetrievalKitIndex *index,
    const char *document_id,
    size_t *deleted_count,
    RetrievalKitStatus *status);

bool retrievalkit_index_compact(
    RetrievalKitIndex *index,
    RetrievalKitCompactionReport *out_report,
    RetrievalKitStatus *status);

bool retrievalkit_index_search(
    const RetrievalKitIndex *index,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitSearchResultBuffer *out_results,
    RetrievalKitStatus *status);

bool retrievalkit_index_keyword_search(
    const RetrievalKitIndex *index,
    const char *text,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitKeywordResultBuffer *out_results,
    RetrievalKitStatus *status);

bool retrievalkit_index_hybrid_search_alpha(
    const RetrievalKitIndex *index,
    const char *text,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const RetrievalKitFilter *filter,
    RetrievalKitHybridQueryOptions options,
    RetrievalKitHybridResultBuffer *out_results,
    RetrievalKitStatus *status);

void retrievalkit_chunk_id_buffer_free(RetrievalKitChunkIdBuffer buffer);
void retrievalkit_text_chunks_free(RetrievalKitTextChunkBuffer buffer);
void retrievalkit_search_results_free(RetrievalKitSearchResultBuffer buffer);
void retrievalkit_keyword_results_free(RetrievalKitKeywordResultBuffer buffer);
void retrievalkit_hybrid_results_free(RetrievalKitHybridResultBuffer buffer);

RetrievalKitFilter *retrievalkit_filter_equals(
    const char *field,
    RetrievalKitMetadataValue value,
    RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_not_equals(
    const char *field,
    RetrievalKitMetadataValue value,
    RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_exists(const char *field, RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_range(
    const char *field,
    const RetrievalKitMetadataValue *lower,
    const RetrievalKitMetadataValue *upper,
    RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_in_values(
    const char *field,
    const RetrievalKitMetadataValue *values,
    size_t value_count,
    RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_all(
    const RetrievalKitFilter *const *filters,
    size_t filter_count,
    RetrievalKitStatus *status);

RetrievalKitFilter *retrievalkit_filter_any(
    const RetrievalKitFilter *const *filters,
    size_t filter_count,
    RetrievalKitStatus *status);

void retrievalkit_filter_free(RetrievalKitFilter *filter);

// Returns the active native vector runtime capabilities as UTF-8 JSON. The
// caller owns the result and must free it with retrievalkit_string_free.
char *retrievalkit_runtime_capabilities_json(void);

// Runs the synthetic benchmark and returns a heap-allocated UTF-8 JSON string.
//
// Pass NULL or "" for the default config:
// {
//   "chunks": 24000,
//   "dimensions": [384, 768],
//   "queries": 200,
//   "top_k": 10,
//   "encodings": ["f32", "f16", "i8"],
//   "metric": "cosine",
//   "include_unfiltered": true,
//   "include_filtered": true,
//   "include_persistence": true,
//   "include_recall": true,
//   "persist_bm25": true,
//   "filter_every": 10
// }
//
// The caller owns the returned pointer and must free it with
// retrievalkit_string_free.
char *retrievalkit_bench_synthetic_json(const char *config_json);

/**
 * Runs one isolated memory benchmark scenario and returns a JSON report.
 * Run each scenario in a fresh process. Free the result with
 * `retrievalkit_string_free`.
 */
char *retrievalkit_bench_memory_json(const char *config_json);

/**
 * Runs one isolated Phase 4b graph-free regression session against the base
 * retrieval APIs. This symbol intentionally has no graph dependency and is
 * present in both the base and graph aggregate products.
 */
char *retrievalkit_phase4_graph_free_regression_json(const char *config_json);

// Frees a string returned by RetrievalKit FFI.
void retrievalkit_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
