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
  "include_recall": true,
  "persist_bm25": true,
  "filter_every": 10
}
```

The returned string is UTF-8 JSON and must be released with
`vectorkit_string_free`.

The report includes runtime SIMD capability flags and one result row for each
dimension, encoding, and filter mode. On Apple platforms, the report also
includes current and peak resident memory snapshots in bytes. When
`include_persistence` is true, each row also includes:

- `save_ms`
- `load_ms`
- persisted file sizes by component
- post-load search latency for the same query set
- memory snapshots before save, after save, after load, and after post-load
  search

Set `include_recall` to `false` for physical-device memory validation runs
where keeping F32 ground-truth indexes alive would inflate RSS.
Set `persist_bm25` to `false` to measure a compact vector-only persisted
profile. Vector search and metadata filters still reload, but keyword and
hybrid search need BM25 to be persisted or rebuilt separately.

## Threading Contract

After construction or loading, exact, keyword, and hybrid search plus the
dimension/count accessors may use one `VkIndex` concurrently. Every call must
own its status, output storage, and filter handle. Save, upsert, delete,
compaction, and free require exclusive access, and the handle must outlive all
calls. The C ABI intentionally adds no hidden locks; callers enforce this
contract. The Swift wrapper supplies a writer-preferring asynchronous gate.
