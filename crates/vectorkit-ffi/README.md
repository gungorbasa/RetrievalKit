# VectorKit FFI

This crate exposes a small C ABI for device-side benchmark harnesses.

The first exported entrypoint is:

```c
char *vectorkit_bench_synthetic_json(const char *config_json);
void vectorkit_string_free(char *ptr);
```

Passing `NULL` or an empty string uses the default benchmark config:

```json
{
  "chunks": 24000,
  "dimensions": [384, 768],
  "queries": 200,
  "top_k": 10,
  "encodings": ["f32", "f16", "i8"],
  "metric": "cosine",
  "include_unfiltered": true,
  "include_filtered": true,
  "filter_every": 10
}
```

The returned string is UTF-8 JSON and must be released with
`vectorkit_string_free`.

The report includes runtime SIMD capability flags and one result row for each
dimension, encoding, and filter mode.
