#ifndef RETRIEVALKIT_GRAPH_FFI_H
#define RETRIEVALKIT_GRAPH_FFI_H

#include "retrievalkit_ffi.h"

#ifdef __cplusplus
extern "C" {
#endif

// Returns the graph aggregate ABI version. This symbol exists only in
// RetrievalKitGraphFFI, which also contains every base RetrievalKit FFI symbol.
uint32_t retrievalkit_graph_ffi_abi_version(void);
char *retrievalkit_phase4_device_query_session_json(const char *config_json);
char *retrievalkit_phase4_device_lifecycle_sample_json(const char *config_json);

#define VK_GRAPH_STATUS_INVALID_SCHEMA 100
#define VK_GRAPH_STATUS_INVALID_IDENTITY 101
#define VK_GRAPH_STATUS_STALE_GENERATION 102
#define VK_GRAPH_STATUS_INCOMPATIBLE_VERSION 103
#define VK_GRAPH_STATUS_GRAPH_UNAVAILABLE 104
#define VK_GRAPH_STATUS_CORRUPT_SNAPSHOT 105
#define VK_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED 106
#define VK_GRAPH_STATUS_CANCELLED 107
#define VK_GRAPH_STATUS_TIMED_OUT 108
#define VK_GRAPH_STATUS_LOCK_UNAVAILABLE 109
#define VK_GRAPH_STATUS_INTERNAL 110
#define VK_GRAPH_STATUS_INVALID_DIMENSION 111
#define VK_GRAPH_STATUS_MISSING_EMBEDDING 112
#define VK_GRAPH_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE 113

typedef struct VkGraphBuilder VkGraphBuilder;
typedef struct VkGraphIndex VkGraphIndex;
typedef struct VkGraphDatabaseBuilder VkGraphDatabaseBuilder;
typedef struct VkGraphDatabase VkGraphDatabase;
typedef struct VkGraphRetrievalBuilder VkGraphRetrievalBuilder;
typedef struct VkGraphRetrievalDatabase VkGraphRetrievalDatabase;

// Graph-only builders accept schema and records, but no vector configuration
// or embeddings.
VkGraphDatabaseBuilder *retrievalkit_graph_database_builder_new(
    const char *corpus_id,
    const char *schema_json,
    VkStatus *status
);
bool retrievalkit_graph_database_builder_upsert_record_json(
    VkGraphDatabaseBuilder *builder,
    const char *record_json,
    VkStatus *status
);
VkGraphDatabase *retrievalkit_graph_database_builder_build(
    VkGraphDatabaseBuilder *builder,
    VkStatus *status
);
void retrievalkit_graph_database_builder_free(VkGraphDatabaseBuilder *builder);

VkGraphRetrievalBuilder *retrievalkit_graph_retrieval_builder_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    const char *schema_json,
    VkStatus *status
);
bool retrievalkit_graph_retrieval_builder_upsert_record_json(
    VkGraphRetrievalBuilder *builder,
    const char *record_json,
    VkStatus *status
);
VkGraphRetrievalDatabase *retrievalkit_graph_retrieval_builder_build(
    VkGraphRetrievalBuilder *builder,
    VkStatus *status
);
void retrievalkit_graph_retrieval_builder_free(VkGraphRetrievalBuilder *builder);

VkGraphDatabase *retrievalkit_graph_database_load(const char *directory, VkStatus *status);
bool retrievalkit_graph_database_save(const VkGraphDatabase *database, const char *directory, VkStatus *status);
bool retrievalkit_graph_database_validate(const char *directory, VkStatus *status);
void retrievalkit_graph_database_free(VkGraphDatabase *database);

VkGraphRetrievalDatabase *retrievalkit_graph_retrieval_database_load(const char *directory, VkStatus *status);
bool retrievalkit_graph_retrieval_database_save(const VkGraphRetrievalDatabase *database, const char *directory, VkStatus *status);
bool retrievalkit_graph_retrieval_database_validate(const char *directory, VkStatus *status);
void retrievalkit_graph_retrieval_database_free(VkGraphRetrievalDatabase *database);

VkGraphBuilder *retrievalkit_graph_builder_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    VkStatus *status
);
bool retrievalkit_graph_builder_upsert_record_json(
    VkGraphBuilder *builder,
    const char *record_json,
    VkStatus *status
);
// Always consumes builder, including when schema validation fails.
VkGraphIndex *retrievalkit_graph_builder_build_json(
    VkGraphBuilder *builder,
    const char *schema_json,
    VkStatus *status
);
void retrievalkit_graph_builder_free(VkGraphBuilder *builder);

VkGraphIndex *retrievalkit_graph_index_load(const char *directory, VkStatus *status);
bool retrievalkit_graph_index_save(
    const VkGraphIndex *index,
    const char *directory,
    VkStatus *status
);
bool retrievalkit_graph_index_validate(const char *directory, VkStatus *status);
void retrievalkit_graph_index_free(VkGraphIndex *index);

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
typedef struct { char *node_type; uint32_t source_type; char *record_id; char *chunk_key; } VkGraphOwnedNode;
// Every pointer in this value is owned by the caller after a successful access.
// Release the complete value with retrievalkit_graph_path_edge_clear.
typedef struct {
    char *relationship_type;
    VkGraphOwnedNode source;
    VkGraphOwnedNode target;
    uint32_t occurrence_ordinal;
    uint32_t schema_rule_index;
    char *source_record_id;
    VkStringArray source_field_segments;
    bool derived_inverse;
    bool built_in;
} VkGraphPathEdge;
typedef struct { size_t seed_count; size_t visited_states; size_t traversed_edges; size_t result_count; size_t diagnostics; uint32_t truncation_reason; } VkGraphTrace;
typedef struct { char *record_id; char *chunk_key; } VkGraphChunkIdentity;
typedef struct {
    VkGraphChunkIdentity *candidates;
    size_t count;
    size_t source_nodes;
    size_t projected_chunks_before_filter;
    size_t projected_chunks_after_filter;
} VkGraphCandidateProjection;

VkGraphResult *retrievalkit_graph_query(const VkGraphIndex *index, VkGraphQuery query, const VkGraphCancellation *cancellation, VkStatus *status);
VkGraphResult *retrievalkit_graph_database_query(const VkGraphDatabase *database, VkGraphQuery query, const VkGraphCancellation *cancellation, VkStatus *status);
VkGraphResult *retrievalkit_graph_retrieval_database_query(const VkGraphRetrievalDatabase *database, VkGraphQuery query, const VkGraphCancellation *cancellation, VkStatus *status);
// out_projection must be zero-initialized or previously cleared. Candidate
// identities are returned in lexical (record_id, chunk_key) order.
bool retrievalkit_graph_database_project_candidates(const VkGraphDatabase *database, const VkGraphResult *result, const VkFilter *filter, VkGraphCandidateProjection *out_projection, VkStatus *status);
bool retrievalkit_graph_retrieval_database_project_candidates(const VkGraphRetrievalDatabase *database, const VkGraphResult *result, const VkFilter *filter, VkGraphCandidateProjection *out_projection, VkStatus *status);
void retrievalkit_graph_candidate_projection_free(VkGraphCandidateProjection projection);
void retrievalkit_graph_candidate_projection_clear(VkGraphCandidateProjection *projection);
size_t retrievalkit_graph_result_count(const VkGraphResult *result);
bool retrievalkit_graph_result_match(const VkGraphResult *result, size_t index, VkGraphMatch *out_match, VkStatus *status);
void retrievalkit_graph_match_clear(VkGraphMatch *value);
// Materializes one edge from the canonical path of a previously materialized
// match. The graph result must remain alive for this call only.
bool retrievalkit_graph_result_path_edge(const VkGraphResult *result, size_t match_index, size_t edge_index, VkGraphPathEdge *out_edge, VkStatus *status);
void retrievalkit_graph_path_edge_clear(VkGraphPathEdge *value);
VkGraphTrace retrievalkit_graph_result_trace(const VkGraphResult *result);
void retrievalkit_graph_result_free(VkGraphResult *result);
VkGraphScope *retrievalkit_graph_result_project(const VkGraphIndex *index, const VkGraphResult *result, VkStatus *status);
size_t retrievalkit_graph_scope_source_nodes(const VkGraphScope *scope);
size_t retrievalkit_graph_scope_resolved_chunks(const VkGraphScope *scope);
void retrievalkit_graph_scope_free(VkGraphScope *scope);
bool retrievalkit_graph_scope_search(const VkGraphIndex *index, const VkGraphScope *scope, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkSearchResultBuffer *out_results, VkStatus *status);
bool retrievalkit_graph_scope_keyword_search(const VkGraphIndex *index, const VkGraphScope *scope, const char *text, size_t top_k, const VkFilter *filter, VkKeywordResultBuffer *out_results, VkStatus *status);
bool retrievalkit_graph_scope_hybrid_search(const VkGraphIndex *index, const VkGraphScope *scope, const char *text, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkHybridOptions options, VkHybridResultBuffer *out_results, VkStatus *status);
bool retrievalkit_graph_retrieval_semantic_search(const VkGraphRetrievalDatabase *database, const VkGraphResult *within, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkSearchResultBuffer *out_results, VkStatus *status);
bool retrievalkit_graph_retrieval_keyword_search(const VkGraphRetrievalDatabase *database, const VkGraphResult *within, const char *text, size_t top_k, const VkFilter *filter, VkKeywordResultBuffer *out_results, VkStatus *status);
bool retrievalkit_graph_retrieval_hybrid_search(const VkGraphRetrievalDatabase *database, const VkGraphResult *within, const char *text, const float *embedding, size_t embedding_len, size_t top_k, const VkFilter *filter, VkHybridOptions options, VkHybridResultBuffer *out_results, VkStatus *status);
VkGraphCancellation *retrievalkit_graph_cancellation_new(void);
void retrievalkit_graph_cancellation_cancel(const VkGraphCancellation *value);
void retrievalkit_graph_cancellation_free(VkGraphCancellation *value);

#ifdef __cplusplus
}
#endif

#endif
