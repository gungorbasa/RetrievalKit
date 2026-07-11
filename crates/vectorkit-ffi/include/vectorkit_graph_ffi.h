#ifndef VECTORKIT_GRAPH_FFI_H
#define VECTORKIT_GRAPH_FFI_H

#include "vectorkit_ffi.h"

#ifdef __cplusplus
extern "C" {
#endif

// Returns the graph aggregate ABI version. This symbol exists only in
// VectorKitGraphFFI, which also contains every base VectorKit FFI symbol.
uint32_t vectorkit_graph_ffi_abi_version(void);

typedef struct VkGraphBuilder VkGraphBuilder;
typedef struct VkGraphIndex VkGraphIndex;

VkGraphBuilder *vectorkit_graph_builder_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    VkStatus *status
);
bool vectorkit_graph_builder_upsert_record_json(
    VkGraphBuilder *builder,
    const char *record_json,
    VkStatus *status
);
// Always consumes builder, including when schema validation fails.
VkGraphIndex *vectorkit_graph_builder_build_json(
    VkGraphBuilder *builder,
    const char *schema_json,
    VkStatus *status
);
void vectorkit_graph_builder_free(VkGraphBuilder *builder);

VkGraphIndex *vectorkit_graph_index_load(const char *directory, VkStatus *status);
bool vectorkit_graph_index_save(
    const VkGraphIndex *index,
    const char *directory,
    VkStatus *status
);
bool vectorkit_graph_index_validate(const char *directory, VkStatus *status);
void vectorkit_graph_index_free(VkGraphIndex *index);

typedef struct VkGraphResult VkGraphResult;
typedef struct VkGraphCancellation VkGraphCancellation;
typedef struct VkGraphScope VkGraphScope;
typedef struct { const char *node_type; uint32_t source_type; const char *record_id; const char *chunk_key; } VkGraphNodeRef;
typedef struct { uint32_t value_type; const char *string_value; int64_t integer_value; bool bool_value; } VkGraphScalar;
typedef struct { const char *relationship; uint32_t direction; size_t min_hops; size_t max_hops; } VkGraphStep;
typedef struct { size_t max_hops; size_t max_visited; size_t max_results; size_t max_working_bytes; } VkGraphLimits;
typedef struct {
    uint32_t seed_type;
    const VkGraphNodeRef *node_ids; size_t node_id_count;
    const char *seed_node_type;
    const char *const *field_segments; size_t field_segment_count;
    const VkGraphScalar *values; size_t value_count;
    const VkGraphStep *steps; size_t step_count;
    VkGraphLimits limits;
} VkGraphQuery;
typedef struct { char *node_type; uint32_t source_type; char *record_id; char *chunk_key; size_t depth; size_t path_length; } VkGraphMatch;
typedef struct { size_t seed_count; size_t visited_states; size_t traversed_edges; size_t result_count; size_t diagnostics; uint32_t truncation_reason; } VkGraphTrace;

VkGraphResult *vectorkit_graph_query(const VkGraphIndex *index, VkGraphQuery query, const VkGraphCancellation *cancellation, VkStatus *status);
size_t vectorkit_graph_result_count(const VkGraphResult *result);
bool vectorkit_graph_result_match(const VkGraphResult *result, size_t index, VkGraphMatch *out_match, VkStatus *status);
void vectorkit_graph_match_clear(VkGraphMatch *value);
VkGraphTrace vectorkit_graph_result_trace(const VkGraphResult *result);
void vectorkit_graph_result_free(VkGraphResult *result);
VkGraphScope *vectorkit_graph_result_project(const VkGraphIndex *index, const VkGraphResult *result, VkStatus *status);
size_t vectorkit_graph_scope_source_nodes(const VkGraphScope *scope);
size_t vectorkit_graph_scope_resolved_chunks(const VkGraphScope *scope);
void vectorkit_graph_scope_free(VkGraphScope *scope);
bool vectorkit_graph_scope_search(const VkGraphIndex *index, const VkGraphScope *scope, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkSearchResultBuffer *out_results, VkStatus *status);
bool vectorkit_graph_scope_keyword_search(const VkGraphIndex *index, const VkGraphScope *scope, const char *text, size_t top_k, const VkFilter *filter, VkKeywordResultBuffer *out_results, VkStatus *status);
bool vectorkit_graph_scope_hybrid_search(const VkGraphIndex *index, const VkGraphScope *scope, const char *text, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkHybridOptions options, VkHybridResultBuffer *out_results, VkStatus *status);
VkGraphCancellation *vectorkit_graph_cancellation_new(void);
void vectorkit_graph_cancellation_cancel(const VkGraphCancellation *value);
void vectorkit_graph_cancellation_free(VkGraphCancellation *value);

#ifdef __cplusplus
}
#endif

#endif
