#ifndef VECTORKIT_FFI_H
#define VECTORKIT_FFI_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VK_STATUS_OK 0
#define VK_STATUS_INVALID_ARGUMENT 1
#define VK_STATUS_CORE_ERROR 2
#define VK_STATUS_PANIC 3
#define VK_STATUS_CORRUPT_INDEX 4

#define VK_METRIC_COSINE 0
#define VK_METRIC_DOT_PRODUCT 1

#define VK_ENCODING_F32 0
#define VK_ENCODING_F16 1
#define VK_ENCODING_BF16 2
#define VK_ENCODING_I8_SCALAR_QUANTIZED 3

#define VK_METADATA_STRING 0
#define VK_METADATA_INTEGER 1
#define VK_METADATA_FLOAT 2
#define VK_METADATA_BOOLEAN 3
#define VK_METADATA_TIMESTAMP_MILLIS 4

#define VK_FUSION_WEIGHTED_NORMALIZED_SCORE 0
#define VK_FUSION_RECIPROCAL_RANK 1

#define VK_CHUNKING_FIXED 0
#define VK_CHUNKING_SENTENCE 1

typedef struct VkIndex VkIndex;
typedef struct VkFilter VkFilter;

typedef struct VkStatus {
  int32_t code;
  char *message;
} VkStatus;

typedef struct VkCompactionReport {
  size_t chunks_before;
  size_t chunks_after;
  size_t chunks_removed;
  size_t estimated_bytes_before;
  size_t estimated_bytes_after;
  size_t estimated_bytes_reclaimed;
} VkCompactionReport;

typedef struct VkMetadataValue {
  uint32_t value_type;
  const char *string_value;
  int64_t integer_value;
  double float_value;
  bool bool_value;
} VkMetadataValue;

typedef struct VkMetadataEntry {
  const char *field;
  VkMetadataValue value;
} VkMetadataEntry;

typedef struct VkChunkInput {
  const char *text;
  const float *embedding;
  size_t embedding_len;
  const VkMetadataEntry *metadata;
  size_t metadata_len;
} VkChunkInput;

typedef struct VkChunkIdBuffer {
  uint64_t *values;
  size_t count;
} VkChunkIdBuffer;

typedef struct VkTextChunk {
  char *text;
  size_t start_byte;
  size_t end_byte;
} VkTextChunk;

typedef struct VkTextChunkBuffer {
  VkTextChunk *chunks;
  size_t count;
} VkTextChunkBuffer;

typedef struct VkSearchHit {
  uint64_t chunk_id;
  char *document_id;
  char *text;
  float score;
  float vector_score;
  bool filter_matched;
} VkSearchHit;

typedef struct VkSearchResultBuffer {
  VkSearchHit *hits;
  size_t count;
} VkSearchResultBuffer;

typedef struct VkStringArray {
  char **values;
  size_t count;
} VkStringArray;

typedef struct VkKeywordHit {
  uint64_t chunk_id;
  char *document_id;
  char *text;
  float score;
  VkStringArray matched_terms;
} VkKeywordHit;

typedef struct VkKeywordResultBuffer {
  VkKeywordHit *hits;
  size_t count;
} VkKeywordResultBuffer;

typedef struct VkHybridHit {
  uint64_t chunk_id;
  char *document_id;
  char *text;
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
  VkStringArray matched_terms;
  bool filter_matched;
} VkHybridHit;

typedef struct VkHybridResultBuffer {
  VkHybridHit *hits;
  size_t count;
} VkHybridResultBuffer;

typedef struct VkHybridOptions {
  size_t vector_top_k;
  size_t keyword_top_k;
  uint32_t fusion_type;
  float vector_weight;
  float keyword_weight;
  float rrf_k;
} VkHybridOptions;

#define VK_STATUS_INVALID_DIMENSION 5
#define VK_STATUS_RETRIEVAL_MODE_UNAVAILABLE 6
#define VK_STATUS_INVALID_IDENTITY 7
#define VK_STATUS_MISSING_EMBEDDING 8

typedef struct VkRetrievalBuilder VkRetrievalBuilder;
typedef struct VkRetrievalDatabase VkRetrievalDatabase;

void vectorkit_status_clear(VkStatus *status);

// retrieval_mode: 0 semantic, 1 hybrid.
VkRetrievalBuilder *vectorkit_retrieval_builder_new(
    uint32_t retrieval_mode,
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    VkStatus *status);
bool vectorkit_retrieval_builder_upsert_record_json(
    VkRetrievalBuilder *builder,
    const char *record_json,
    VkStatus *status);
VkRetrievalDatabase *vectorkit_retrieval_builder_build(
    VkRetrievalBuilder *builder,
    VkStatus *status);
void vectorkit_retrieval_builder_free(VkRetrievalBuilder *builder);

VkRetrievalDatabase *vectorkit_retrieval_database_load(const char *directory, VkStatus *status);
bool vectorkit_retrieval_database_save(const VkRetrievalDatabase *database, const char *directory, VkStatus *status);
bool vectorkit_retrieval_database_validate(const char *directory, VkStatus *status);
void vectorkit_retrieval_database_free(VkRetrievalDatabase *database);
bool vectorkit_retrieval_semantic_search(
    const VkRetrievalDatabase *database,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const VkFilter *filter,
    VkSearchResultBuffer *out_results,
    VkStatus *status);
bool vectorkit_retrieval_hybrid_search(
    const VkRetrievalDatabase *database,
    const char *text,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const VkFilter *filter,
    VkHybridOptions options,
    VkHybridResultBuffer *out_results,
    VkStatus *status);

/*
 * Threading contract for one VkIndex handle:
 *
 * - dimension/count accessors and exact, keyword, and hybrid search may run
 *   concurrently after construction or loading.
 * - save, upsert, delete, compaction, and free require exclusive access.
 * - callers must not start new reads while an exclusive operation is waiting.
 * - every concurrent call must use independent VkStatus, output buffers, and
 *   VkFilter handles. Output buffers may be freed independently.
 * - the handle must remain alive until every call using it has returned.
 *
 * The C ABI does not add locks. Callers are responsible for enforcing this
 * contract; the Swift wrapper provides a writer-preferring asynchronous gate.
 */

VkIndex *vectorkit_index_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    VkStatus *status);

VkIndex *vectorkit_index_load(const char *directory, VkStatus *status);

bool vectorkit_index_validate(const char *directory, VkStatus *status);
void vectorkit_index_free(VkIndex *index);

bool vectorkit_index_save(
    VkIndex *index,
    const char *directory,
    bool include_bm25,
    VkStatus *status);

size_t vectorkit_index_dimension(const VkIndex *index);
size_t vectorkit_index_active_chunk_count(const VkIndex *index);
size_t vectorkit_index_total_chunk_count(const VkIndex *index);
size_t vectorkit_index_tombstoned_chunk_count(const VkIndex *index);

bool vectorkit_chunk_text(
    const char *text,
    uint32_t strategy,
    size_t max_characters,
    size_t overlap_characters,
    VkTextChunkBuffer *out_chunks,
    VkStatus *status);

bool vectorkit_index_upsert_document(
    VkIndex *index,
    const char *document_id,
    const char *document_text,
    const VkMetadataEntry *document_metadata,
    size_t document_metadata_len,
    const VkChunkInput *chunks,
    size_t chunk_count,
    VkChunkIdBuffer *out_chunk_ids,
    VkStatus *status);

bool vectorkit_index_delete_document(
    VkIndex *index,
    const char *document_id,
    size_t *deleted_count,
    VkStatus *status);

bool vectorkit_index_compact(
    VkIndex *index,
    VkCompactionReport *out_report,
    VkStatus *status);

bool vectorkit_index_search(
    const VkIndex *index,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const VkFilter *filter,
    VkSearchResultBuffer *out_results,
    VkStatus *status);

bool vectorkit_index_keyword_search(
    const VkIndex *index,
    const char *text,
    size_t top_k,
    const VkFilter *filter,
    VkKeywordResultBuffer *out_results,
    VkStatus *status);

bool vectorkit_index_hybrid_search(
    const VkIndex *index,
    const char *text,
    const float *embedding,
    size_t embedding_len,
    size_t top_k,
    const VkFilter *filter,
    VkHybridOptions options,
    VkHybridResultBuffer *out_results,
    VkStatus *status);

void vectorkit_chunk_id_buffer_free(VkChunkIdBuffer buffer);
void vectorkit_text_chunks_free(VkTextChunkBuffer buffer);
void vectorkit_search_results_free(VkSearchResultBuffer buffer);
void vectorkit_keyword_results_free(VkKeywordResultBuffer buffer);
void vectorkit_hybrid_results_free(VkHybridResultBuffer buffer);

VkFilter *vectorkit_filter_equals(
    const char *field,
    VkMetadataValue value,
    VkStatus *status);

VkFilter *vectorkit_filter_not_equals(
    const char *field,
    VkMetadataValue value,
    VkStatus *status);

VkFilter *vectorkit_filter_exists(const char *field, VkStatus *status);

VkFilter *vectorkit_filter_range(
    const char *field,
    const VkMetadataValue *lower,
    const VkMetadataValue *upper,
    VkStatus *status);

VkFilter *vectorkit_filter_in_values(
    const char *field,
    const VkMetadataValue *values,
    size_t value_count,
    VkStatus *status);

VkFilter *vectorkit_filter_all(
    const VkFilter *const *filters,
    size_t filter_count,
    VkStatus *status);

VkFilter *vectorkit_filter_any(
    const VkFilter *const *filters,
    size_t filter_count,
    VkStatus *status);

void vectorkit_filter_free(VkFilter *filter);

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
// vectorkit_string_free.
char *vectorkit_bench_synthetic_json(const char *config_json);

/**
 * Runs one isolated memory benchmark scenario and returns a JSON report.
 * Run each scenario in a fresh process. Free the result with
 * `vectorkit_string_free`.
 */
char *vectorkit_bench_memory_json(const char *config_json);

// Frees a string returned by VectorKit FFI.
void vectorkit_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
