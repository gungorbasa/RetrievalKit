#ifndef VECTORKIT_FFI_H
#define VECTORKIT_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

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
//   "filter_every": 10
// }
//
// The caller owns the returned pointer and must free it with
// vectorkit_string_free.
char *vectorkit_bench_synthetic_json(const char *config_json);

// Frees a string returned by vectorkit_bench_synthetic_json.
void vectorkit_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
