# VectorKit Working Memory

This file captures active implementation context that should survive chat
changes. Keep it short. Delete or move notes once they become irrelevant,
implemented, or superseded by the product spec.

## Current Workflow

- Work through tasks one by one.
- Explain the next task and wait for approval before implementing it.
- After approval, implement only that task, run checks, commit, push, then
  present the next task.
- Prefer mature fast crates for performance-sensitive work when they clearly
  help. Avoid dependencies for simple local logic.

## Active Product Constraints

- VectorKit is local-first retrieval for mobile/desktop, with iOS/macOS as the
  first wrapper target.
- V1 remains exact vector search, BM25 keyword search, hybrid ranking,
  filtering, persistence, and Swift integration.
- Do not add HNSW/ANN, server mode, sync, dashboards, or distributed database
  behavior unless the product spec changes.
- Retrieval must stay fast on local devices. Avoid hot-path JSON, SQLite,
  network calls, avoidable allocation, and broad string lookups.

## Current Size, RAM, and Speed Goal

- Hard goal: roughly `24K` vectors plus required local data must stay under
  `20 MB` total persisted size. This means vectors plus chunk metadata, required
  display/retrieval data, BM25 data if enabled, headers, tombstones, and version
  data. Do not treat the target as vector-only.
- RAM matters too. Prefer compact in-memory layouts and avoid loading full text
  or broad string-heavy structures on the hot retrieval path unless benchmarks
  prove the cost is acceptable.
- Speed remains a first-class requirement. Size reductions are not useful if
  they push target-device retrieval outside the low-latency budget.
- `24K x 384d` with `I8ScalarQuantized` is the most practical near-term target.
- `24K x 768d` with `I8ScalarQuantized` is tight because vector payload alone is
  about `17.7 MiB`, leaving little room for chunks, metadata, BM25, headers, and
  tombstones.
- `24K x 1536d` cannot fit under `20 MB` with current I8 storage.
- Full chunk text should likely stay outside the hot index if the `20 MB` target
  includes user-visible data.

Approximate vector-only sizes for `24K` vectors:

| dim | F32 | F16 | I8 | Binary |
|---:|---:|---:|---:|---:|
| 384 | ~35.2 MiB | ~17.6 MiB | ~8.9 MiB | ~1.1 MiB |
| 768 | ~70.3 MiB | ~35.2 MiB | ~17.7 MiB | ~2.2 MiB |
| 1536 | ~140.6 MiB | ~70.3 MiB | ~35.3 MiB | ~4.4 MiB |

## Encoding Decisions

- `F32` remains correctness ground truth.
- `F16` is a good quality-preserving size reduction and currently has full
  synthetic recall against F32 in the benchmark runs.
- `BF16` is supported but was much slower than F32/F16 on the current machine.
  Do not choose it for speed without device-specific benchmarks.
- `I8ScalarQuantized` is implemented as symmetric per-vector quantization:
  `i8` values plus one `f32` scale per vector.
- I8 synthetic recall passed the current gate, but exact full-scan latency was
  slightly slower than F32/F16. Treat it as a memory feature, not a speed
  feature, unless later benchmarks prove otherwise.
- `BinaryQuantized` is a future size-constrained candidate retrieval option.
  It may be necessary for `768d + data <20 MB`, but needs recall benchmarking
  before use.

## Recent Benchmark Takeaways

For `50K` vectors, `top_k=5`, `100` synthetic queries:

| dim | F32 avg | F16 avg | I8 avg | I8 recall@5 vs F32 |
|---:|---:|---:|---:|---:|
| 384 | ~7.4 ms | ~7.5 ms | ~8.0 ms | 0.9900 |
| 768 | ~13.6 ms | ~13.7 ms | ~14.3 ms | 0.9920 |
| 1536 | ~25.9 ms | ~26.0 ms | ~27.1 ms | 0.9860 |

For `24K`, expected exact I8 retrieval is roughly:

| dim | expected I8 latency |
|---:|---:|
| 384 | ~3.8-4.2 ms |
| 768 | ~7.0-7.5 ms |
| 1536 | ~13 ms+, but too large for the current size target |

## Deferred Exploration

- Optional rerank vector store for compressed encodings is deferred.
- Revisit reranking only after real-data benchmarks show I8 needs higher final
  quality.
- If explored later, measure disk size, memory, recall, and latency for:
  `I8`, `I8 + F16 rerank`, and `I8 + F32 rerank`.
- Reranking adds small CPU cost for `top_k * overfetch` candidates but
  significant memory/disk cost from the second vector store.

## Likely Next Tasks

- Add a storage-size estimator or disk-size benchmark output that separates:
  vector bytes, chunk metadata bytes, BM25 bytes, tombstone/version bytes, and
  total estimated index bytes.
- After persistence exists, replace estimates with actual file-size reporting
  using saved index files.
- Benchmark the real target explicitly: `24K`, `384d/768d`, `top_k=5/10`,
  `I8ScalarQuantized`, with recall and size estimates.
