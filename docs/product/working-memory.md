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
- `24K x 384d` with `I8ScalarQuantized` is the first practical compact target.
  With current binary persistence, it saves at `12.772 MiB` including vectors,
  chunks, BM25, tombstones, and manifest.
- `24K x 384d` with `F16` is close but over budget at `21.470 MiB`.
- `24K x 768d` with `I8ScalarQuantized` is also close but over budget at
  `21.561 MiB`; vector payload alone is about `17.7 MiB`.
- `24K x 1536d` cannot fit under `20 MB` with current I8 storage.
- Full chunk text should likely stay outside the hot index if the `20 MB` target
  includes user-visible data.
- Current size/speed report: `docs/product/size-speed-report.md`.

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
- I8 synthetic recall passed the current gate. Exact full-scan latency is now
  faster than F32/F16 on the current Apple M1 Max development machine because
  VectorKit uses an AArch64 `dotprod` C shim for I8 dot products when runtime
  feature detection reports support. SimSIMD still reports
  `neon,neon_f16,dynamic`, not `neon_i8`, because this machine has
  `FEAT_DotProd=1` but `FEAT_I8MM=0`; keep VectorKit's guarded fallback path.
- `BinaryQuantized` is a future size-constrained candidate retrieval option.
  It may be necessary for `768d + data <20 MB`, but needs recall benchmarking
  before use.

## Recent Benchmark Takeaways

For `24K` vectors, `top_k=10`, `200` synthetic queries after the AArch64 I8
dotprod backend, late result materialization, active-offset scans, and the
unfiltered I8 fast path:

| dim | F32 avg | F16 avg | I8 avg | I8 recall@10 vs F32 |
|---:|---:|---:|---:|---:|
| 384 | ~2.5 ms | ~2.6 ms | ~0.8 ms | 0.9895 |
| 768 | ~5.4 ms | ~5.6 ms | ~1.0 ms | 0.9920 |

For isolated scoring kernels, `50K x 768d I8` measured about `1.25 ms` average
versus about `9.1 ms` for F32 on the same machine.

With an indexed equality filter at roughly `1/10` selectivity, `24K` vectors,
`top_k=10`, and `200` synthetic queries, filtered `I8ScalarQuantized` measured
about `0.51 ms` average for `384d` and `0.81 ms` average for `768d`.

## Deferred Exploration

- `docs/research/turbovec-notes.md` captures ideas from `RyanCodrai/turbovec`.
  Useful ideas include filter-aware scoring loops, explicit cache warmup,
  strict binary format validation, and benchmark/debug counters. Do not adopt
  its approximate TurboQuant index as the V1 primary retrieval engine.
- Optional rerank vector store for compressed encodings is deferred.
- Revisit reranking only after real-data benchmarks show I8 needs higher final
  quality.
- If explored later, measure disk size, memory, recall, and latency for:
  `I8`, `I8 + F16 rerank`, and `I8 + F32 rerank`.
- Reranking adds small CPU cost for `top_k * overfetch` candidates but
  significant memory/disk cost from the second vector store.

## Completed Optimizations

- Late result materialization is implemented for exact vector search. The hot
  loop keeps only `chunk_id`, `offset`, and score, then builds final `SearchHit`
  values after sorting the winning candidates.
- Active-offset scanning is implemented for unfiltered exact vector search. The
  index keeps a derived `active_offsets` list so tombstoned rows are not scanned
  after upserts, deletes, or persistence reload.
- A specialized unfiltered `I8ScalarQuantized` search path is implemented. It
  borrows the contiguous I8 values and scale arrays directly, skips generic
  filter checks, and keeps final result materialization after top-k selection.
- The specialized I8 search path now also handles filtered vector searches. It
  uses metadata candidate offsets when available, still verifies the actual
  filter predicate for correctness, and falls back to active-offset scans for
  filter shapes that cannot be fully narrowed by the metadata index.
- A `vectorkit-ffi` crate now exposes `vectorkit_bench_synthetic_json` and
  `vectorkit_string_free` for Swift/macOS/iOS benchmark harnesses. The default
  benchmark runs `24K` chunks, `384d` and `768d`, `f32`/`f16`/`i8`, and both
  unfiltered and `filter_every=10` filtered searches. FFI benchmark rows now
  also include persistence save time, load time, persisted file sizes, and
  post-load search latency by default.
- A SwiftPM macOS command-line harness exists at
  `wrappers/swift/VectorKitBench`. It links `vectorkit-ffi`, supports
  `--small-smoke`, `--config`, and `--config-file`, and successfully ran the
  full default FFI benchmark locally.
- `scripts/build-xcframework.sh` packages `vectorkit-ffi` as
  `target/apple/VectorKitFFI.xcframework`. The full Apple package is verified
  locally with `ios-arm64`, `ios-arm64-simulator`, and `macos-arm64` slices.
  The iOS simulator slice is arm64-only; `x86_64-apple-ios` is intentionally
  not used.
- A minimal SwiftUI iOS benchmark app exists at
  `wrappers/swift/VectorKitIOSBench`. It links the local XCFramework, exposes
  smoke and full default benchmark buttons, and the generic iOS Simulator build
  succeeds locally.

## Likely Next Tasks

- Run the iOS benchmark app on physical iPhone/iPad hardware and compare device
  latency against the macOS SwiftPM and Rust benchmark reports.
- Add a fixture-backed benchmark with realistic chunk text, metadata, and BM25
  distributions, then persist and report actual file sizes.
- Benchmark the I8 dotprod path on target iPhone/iPad/Mac hardware, especially
  older devices that may not report `dotprod`.
- Explore parallel exact scan only after target-device benchmarks show remaining
  CPU pressure.
- Add payload/RSS memory reporting to the benchmark output.
- Validate the compact target on a target Apple device through the Swift
  wrapper once the wrapper can load persisted indexes.
