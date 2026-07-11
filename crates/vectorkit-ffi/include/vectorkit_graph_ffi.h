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

#ifdef __cplusplus
}
#endif

#endif
