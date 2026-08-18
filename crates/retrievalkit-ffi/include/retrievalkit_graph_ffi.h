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

#define RETRIEVALKIT_GRAPH_STATUS_INVALID_SCHEMA 100
#define RETRIEVALKIT_GRAPH_STATUS_INVALID_IDENTITY 101
#define RETRIEVALKIT_GRAPH_STATUS_STALE_GENERATION 102
#define RETRIEVALKIT_GRAPH_STATUS_INCOMPATIBLE_VERSION 103
#define RETRIEVALKIT_GRAPH_STATUS_GRAPH_UNAVAILABLE 104
#define RETRIEVALKIT_GRAPH_STATUS_CORRUPT_SNAPSHOT 105
#define RETRIEVALKIT_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED 106
#define RETRIEVALKIT_GRAPH_STATUS_CANCELLED 107
#define RETRIEVALKIT_GRAPH_STATUS_TIMED_OUT 108
#define RETRIEVALKIT_GRAPH_STATUS_LOCK_UNAVAILABLE 109
#define RETRIEVALKIT_GRAPH_STATUS_INTERNAL 110
#define RETRIEVALKIT_GRAPH_STATUS_INVALID_DIMENSION 111
#define RETRIEVALKIT_GRAPH_STATUS_MISSING_EMBEDDING 112
#define RETRIEVALKIT_GRAPH_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE 113

typedef struct RetrievalKitGraphBuilder RetrievalKitGraphBuilder;
typedef struct RetrievalKitGraphIndex RetrievalKitGraphIndex;
typedef struct RetrievalKitGraphDatabaseBuilder RetrievalKitGraphDatabaseBuilder;
typedef struct RetrievalKitGraphDatabase RetrievalKitGraphDatabase;
typedef struct RetrievalKitGraphRetrievalBuilder RetrievalKitGraphRetrievalBuilder;
typedef struct RetrievalKitGraphRetrievalDatabase RetrievalKitGraphRetrievalDatabase;

// Graph-only builders accept schema and records, but no vector configuration
// or embeddings.
RetrievalKitGraphDatabaseBuilder *retrievalkit_graph_database_builder_new(
    const char *corpus_id,
    const char *schema_json,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_database_builder_upsert_record_json(
    RetrievalKitGraphDatabaseBuilder *builder,
    const char *record_json,
    RetrievalKitStatus *status
);
RetrievalKitGraphDatabase *retrievalkit_graph_database_builder_build(
    RetrievalKitGraphDatabaseBuilder *builder,
    RetrievalKitStatus *status
);
void retrievalkit_graph_database_builder_free(RetrievalKitGraphDatabaseBuilder *builder);

RetrievalKitGraphRetrievalBuilder *retrievalkit_graph_retrieval_builder_new(
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    const char *schema_json,
    RetrievalKitStatus *status
);
RetrievalKitGraphRetrievalBuilder *retrievalkit_graph_retrieval_builder_new_with_bm25(
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    const char *schema_json,
    float bm25_k1,
    float bm25_b,
    const char *stop_words_json,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_retrieval_builder_upsert_record_json(
    RetrievalKitGraphRetrievalBuilder *builder,
    const char *record_json,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_retrieval_builder_upsert_record_with_embedding_json(
    RetrievalKitGraphRetrievalBuilder *builder,
    const char *record_json,
    const float *embedding,
    size_t embedding_len,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_retrieval_builder_upsert_documents_json(
    RetrievalKitGraphRetrievalBuilder *builder,
    const char *batch_json,
    RetrievalKitStatus *status
);
RetrievalKitGraphRetrievalDatabase *retrievalkit_graph_retrieval_builder_build(
    RetrievalKitGraphRetrievalBuilder *builder,
    RetrievalKitStatus *status
);
void retrievalkit_graph_retrieval_builder_free(RetrievalKitGraphRetrievalBuilder *builder);

RetrievalKitGraphDatabase *retrievalkit_graph_database_load(const char *directory, RetrievalKitStatus *status);
bool retrievalkit_graph_database_save(const RetrievalKitGraphDatabase *database, const char *directory, RetrievalKitStatus *status);
bool retrievalkit_graph_database_validate(const char *directory, RetrievalKitStatus *status);
void retrievalkit_graph_database_free(RetrievalKitGraphDatabase *database);

RetrievalKitGraphRetrievalDatabase *retrievalkit_graph_retrieval_database_load(const char *directory, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_database_save(const RetrievalKitGraphRetrievalDatabase *database, const char *directory, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_database_validate(const char *directory, RetrievalKitStatus *status);
void retrievalkit_graph_retrieval_database_free(RetrievalKitGraphRetrievalDatabase *database);

RetrievalKitGraphBuilder *retrievalkit_graph_builder_new(
    size_t dimension,
    uint32_t metric,
    uint32_t encoding,
    const char *corpus_id,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_builder_upsert_record_json(
    RetrievalKitGraphBuilder *builder,
    const char *record_json,
    RetrievalKitStatus *status
);
// Always consumes builder, including when schema validation fails.
RetrievalKitGraphIndex *retrievalkit_graph_builder_build_json(
    RetrievalKitGraphBuilder *builder,
    const char *schema_json,
    RetrievalKitStatus *status
);
void retrievalkit_graph_builder_free(RetrievalKitGraphBuilder *builder);

RetrievalKitGraphIndex *retrievalkit_graph_index_load(const char *directory, RetrievalKitStatus *status);
bool retrievalkit_graph_index_save(
    const RetrievalKitGraphIndex *index,
    const char *directory,
    RetrievalKitStatus *status
);
bool retrievalkit_graph_index_validate(const char *directory, RetrievalKitStatus *status);
void retrievalkit_graph_index_free(RetrievalKitGraphIndex *index);

typedef struct RetrievalKitGraphResult RetrievalKitGraphResult;
typedef struct RetrievalKitGraphCancellation RetrievalKitGraphCancellation;
typedef struct RetrievalKitGraphScope RetrievalKitGraphScope;
typedef struct { const char *node_type; uint32_t source_type; const char *record_id; const char *chunk_key; } RetrievalKitGraphNodeRef;
typedef struct { uint32_t value_type; const char *string_value; int64_t integer_value; bool bool_value; } RetrievalKitGraphScalar;
typedef struct { const char *relationship; uint32_t direction; size_t min_hops; size_t max_hops; } RetrievalKitGraphStep;
typedef struct { size_t max_hops; size_t max_visited; size_t max_results; size_t max_working_bytes; } RetrievalKitGraphLimits;
typedef struct {
    uint32_t seed_type;
    const RetrievalKitGraphNodeRef *node_ids; size_t node_id_count;
    const char *seed_node_type;
    const char *const *field_segments; size_t field_segment_count;
    const RetrievalKitGraphScalar *values; size_t value_count;
    const RetrievalKitGraphStep *steps; size_t step_count;
    RetrievalKitGraphLimits limits;
} RetrievalKitGraphQuery;
typedef struct { char *node_type; uint32_t source_type; char *record_id; char *chunk_key; size_t depth; size_t path_length; } RetrievalKitGraphMatch;
typedef struct { char *node_type; uint32_t source_type; char *record_id; char *chunk_key; } RetrievalKitGraphOwnedNode;
// Every pointer in this value is owned by the caller after a successful access.
// Release the complete value with retrievalkit_graph_path_edge_clear.
typedef struct {
    char *relationship_type;
    RetrievalKitGraphOwnedNode source;
    RetrievalKitGraphOwnedNode target;
    uint32_t occurrence_ordinal;
    uint32_t schema_rule_index;
    char *source_record_id;
    RetrievalKitStringArray source_field_segments;
    bool derived_inverse;
    bool built_in;
} RetrievalKitGraphPathEdge;
typedef struct { size_t seed_count; size_t visited_states; size_t traversed_edges; size_t result_count; size_t diagnostics; uint32_t truncation_reason; } RetrievalKitGraphTrace;
typedef struct { char *record_id; char *chunk_key; } RetrievalKitGraphChunkIdentity;
typedef struct {
    RetrievalKitGraphChunkIdentity *candidates;
    size_t count;
    size_t source_nodes;
    size_t projected_chunks_before_filter;
    size_t projected_chunks_after_filter;
} RetrievalKitGraphCandidateProjection;

RetrievalKitGraphResult *retrievalkit_graph_query(const RetrievalKitGraphIndex *index, RetrievalKitGraphQuery query, const RetrievalKitGraphCancellation *cancellation, RetrievalKitStatus *status);
RetrievalKitGraphResult *retrievalkit_graph_database_query(const RetrievalKitGraphDatabase *database, RetrievalKitGraphQuery query, const RetrievalKitGraphCancellation *cancellation, RetrievalKitStatus *status);
RetrievalKitGraphResult *retrievalkit_graph_retrieval_database_query(const RetrievalKitGraphRetrievalDatabase *database, RetrievalKitGraphQuery query, const RetrievalKitGraphCancellation *cancellation, RetrievalKitStatus *status);
// out_projection must be zero-initialized or previously cleared. Candidate
// identities are returned in lexical (record_id, chunk_key) order.
bool retrievalkit_graph_database_project_candidates(const RetrievalKitGraphDatabase *database, const RetrievalKitGraphResult *result, const RetrievalKitFilter *filter, RetrievalKitGraphCandidateProjection *out_projection, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_database_project_candidates(const RetrievalKitGraphRetrievalDatabase *database, const RetrievalKitGraphResult *result, const RetrievalKitFilter *filter, RetrievalKitGraphCandidateProjection *out_projection, RetrievalKitStatus *status);
void retrievalkit_graph_candidate_projection_free(RetrievalKitGraphCandidateProjection projection);
void retrievalkit_graph_candidate_projection_clear(RetrievalKitGraphCandidateProjection *projection);
size_t retrievalkit_graph_result_count(const RetrievalKitGraphResult *result);
bool retrievalkit_graph_result_match(const RetrievalKitGraphResult *result, size_t index, RetrievalKitGraphMatch *out_match, RetrievalKitStatus *status);
void retrievalkit_graph_match_clear(RetrievalKitGraphMatch *value);
// Materializes one edge from the canonical path of a previously materialized
// match. The graph result must remain alive for this call only.
bool retrievalkit_graph_result_path_edge(const RetrievalKitGraphResult *result, size_t match_index, size_t edge_index, RetrievalKitGraphPathEdge *out_edge, RetrievalKitStatus *status);
void retrievalkit_graph_path_edge_clear(RetrievalKitGraphPathEdge *value);
RetrievalKitGraphTrace retrievalkit_graph_result_trace(const RetrievalKitGraphResult *result);
void retrievalkit_graph_result_free(RetrievalKitGraphResult *result);
RetrievalKitGraphScope *retrievalkit_graph_result_project(const RetrievalKitGraphIndex *index, const RetrievalKitGraphResult *result, RetrievalKitStatus *status);
size_t retrievalkit_graph_scope_source_nodes(const RetrievalKitGraphScope *scope);
size_t retrievalkit_graph_scope_resolved_chunks(const RetrievalKitGraphScope *scope);
void retrievalkit_graph_scope_free(RetrievalKitGraphScope *scope);
bool retrievalkit_graph_scope_search(const RetrievalKitGraphIndex *index, const RetrievalKitGraphScope *scope, const float *embedding, size_t embedding_len, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitSearchResultBuffer *out_results, RetrievalKitStatus *status);
bool retrievalkit_graph_scope_keyword_search(const RetrievalKitGraphIndex *index, const RetrievalKitGraphScope *scope, const char *text, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitKeywordResultBuffer *out_results, RetrievalKitStatus *status);
bool retrievalkit_graph_scope_hybrid_search_alpha(const RetrievalKitGraphIndex *index, const RetrievalKitGraphScope *scope, const char *text, const float *embedding, size_t embedding_len, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitHybridQueryOptions options, RetrievalKitHybridResultBuffer *out_results, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_semantic_search(const RetrievalKitGraphRetrievalDatabase *database, const RetrievalKitGraphResult *within, const float *embedding, size_t embedding_len, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitSearchResultBuffer *out_results, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_keyword_search(const RetrievalKitGraphRetrievalDatabase *database, const RetrievalKitGraphResult *within, const char *text, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitKeywordResultBuffer *out_results, RetrievalKitStatus *status);
bool retrievalkit_graph_retrieval_hybrid_search_alpha(const RetrievalKitGraphRetrievalDatabase *database, const RetrievalKitGraphResult *within, const char *text, const float *embedding, size_t embedding_len, size_t top_k, const RetrievalKitFilter *filter, RetrievalKitHybridQueryOptions options, RetrievalKitHybridResultBuffer *out_results, RetrievalKitStatus *status);
RetrievalKitGraphCancellation *retrievalkit_graph_cancellation_new(void);
void retrievalkit_graph_cancellation_cancel(const RetrievalKitGraphCancellation *value);
void retrievalkit_graph_cancellation_free(RetrievalKitGraphCancellation *value);

#ifdef __cplusplus
}
#endif

#endif
