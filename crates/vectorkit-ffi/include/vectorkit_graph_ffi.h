#ifndef VECTORKIT_GRAPH_FFI_H
#define VECTORKIT_GRAPH_FFI_H

#include "vectorkit_ffi.h"

#ifdef __cplusplus
extern "C" {
#endif

// Returns the graph aggregate ABI version. This symbol exists only in
// VectorKitGraphFFI, which also contains every base VectorKit FFI symbol.
uint32_t vectorkit_graph_ffi_abi_version(void);

#ifdef __cplusplus
}
#endif

#endif
