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
  "include_persistence": true,
  "persist_bm25": true,
  "filter_every": 10
}
```

The returned string is UTF-8 JSON and must be released with
`vectorkit_string_free`.

The report includes runtime SIMD capability flags and one result row for each
dimension, encoding, and filter mode. When `include_persistence` is true, each
row also includes:

- `save_ms`
- `load_ms`
- persisted file sizes by component
- post-load search latency for the same query set

Set `persist_bm25` to `false` to measure a compact vector-only persisted
profile. Vector search and metadata filters still reload, but keyword and
hybrid search need BM25 to be persisted or rebuilt separately.
