# RetrievalKit FFI

This crate exposes a small C ABI for device-side benchmark harnesses.

The first exported entrypoint is:

```c
char *retrievalkit_bench_synthetic_json(const char *config_json);
char *retrievalkit_bench_memory_json(const char *config_json);
void retrievalkit_string_free(char *ptr);
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
`retrievalkit_string_free`.

The optional graph aggregate also provides typed candidate projection for
`VkGraphDatabase` and `VkGraphRetrievalDatabase`. These calls accept a native
`VkGraphResult` plus an optional `VkFilter` and return lexical stable chunk
identities and source/before/after counts in `VkGraphCandidateProjection`; no
JSON or internal chunk IDs cross this boundary. Initialize the output to zero,
then release it exactly once with
`retrievalkit_graph_candidate_projection_free`, or use the idempotent
`retrievalkit_graph_candidate_projection_clear` on its address. Failed calls leave
the output unchanged, and stale or cross-corpus results use the graph
stale-generation status.

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

`retrievalkit_bench_memory_json` runs one isolated lifecycle scenario. It samples
phase RSS on Apple platforms, exercises persistence and compaction, and checks
optional memory, disk, and latency budgets. Run each invocation in a fresh
process. See `docs/product/memory-benchmark.md` for the schema and commands.

## Threading Contract

After construction or loading, exact, keyword, and hybrid search plus the
dimension/count accessors may use one `VkIndex` concurrently. Every call must
own its status, output storage, and filter handle. Save, upsert, delete,
compaction, and free require exclusive access, and the handle must outlive all
calls. The C ABI intentionally adds no hidden locks; callers enforce this
contract. The Swift wrapper supplies a writer-preferring asynchronous gate.
