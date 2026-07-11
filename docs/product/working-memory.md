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
  filtering, persistence, and Swift integration. An optional fully local graph
  package is now authorized behind gated M0-M5 milestones; graph-free core hot
  paths and artifacts remain graph-neutral.
- Do not add HNSW/ANN, server mode, sync, dashboards, or distributed database
  behavior unless the product spec changes.
- Retrieval must stay fast on local devices. Avoid hot-path JSON, SQLite,
  network calls, avoidable allocation, and broad string lookups.

## Optional Graph Roadmap Status

- M0 product authorization and the customer fixture contract/template are in
  place. The template deliberately contains no invented customer data.
- M1 implements canonical `RecordStore` values/identities, persisted corpus and
  generation identity, adaptive `CandidateScope`, scoped exact/BM25/hybrid
  retrieval, and ordered bulk hydration in `vectorkit-core`.
- The 10K x 384d local development comparison measured graph-free p95 changes
  of exact +1.32%, BM25 +2.46%, and hybrid +2.66% versus pre-M1 using the median
  of three final p95 runs. Repeat the <=3% release gate on pinned hardware.
- M2 is authorized as a generic schema-driven package using domain-neutral
  synthetic conformance fixtures. Customer data is deferred acceptance evidence
  and must never become hard-coded schema behavior. Real-workload capacity and
  device claims remain provisional until private customer validation occurs.
- `vectorkit-graph` M2 now provides record/chunk node schemas, explicit typed
  references and collections, validation policies, deterministic CSR adjacency,
  exact property seeds, bounded multi-step traversal, cycles, canonical paths,
  limits, cancellation, edge provenance, and graph-result projection into exact,
  BM25, and hybrid candidate scopes. It consumes one core index and does not
  expose a peer mutable handle.
- M3.1 now defines deterministic canonical schema JSON with a BLAKE3 identity
  and a bounded versioned graph snapshot payload tied to the core corpus and
  generation. Round-trip, deterministic encoding, corruption, truncation,
  trailing-byte, schema-hash, and stale-generation tests pass. M3.2 still needs
  to compose this payload with core persistence through atomic staging,
  validation, and activation.
- M3.2 now persists one composite immutable generation containing a complete
  core database, `schema.json`, and `graph.bin`. It checks sizes and BLAKE3
  digests, reopens and validates the staged core/graph pair, syncs it, and only
  then atomically activates it through the graph manifest. Read-only validation,
  repeat saves, abandoned staging, unsafe manifest paths, truncation, appended
  data, and same-size corruption have conformance coverage.
- M3.3 now serializes writers with an OS-released exclusive database lock.
  Loaders use a short shared open lock and retain a shared per-generation lease,
  preventing cleanup while a loaded index uses that generation. Locked saves
  remove abandoned staging and unleased superseded generations; an invalid
  existing manifest stops recovery without deleting snapshots.
- M3.4 injects failure at every pre-activation composite-save checkpoint and
  proves the previous manifest remains byte-identical and loadable. The M1 Max
  local persistence fixture measured save p95 98 ms and open/validate p95 10 ms
  for 2K records/8K edges. Five interleaved current/pre-M1 graph-free runs
  measured exact +0.44%, BM25 +1.01%, and hybrid +0.73%, passing the <=3% local
  gate. See `docs/product/reports/graph-m3-benchmark-report.md`; pinned-device
  release qualification is still required.
- M4.1 selected and proved the Swift aggregate packaging topology. The existing
  `vectorkit-ffi` crate has an off-by-default `graph` feature; base
  `VectorKitFFI` remains graph-free, while `VectorKitGraphFFI` is built with the
  feature and contains the base retrieval symbols plus graph ABI symbols in one
  static library. Graph-enabled apps select the aggregate instead of linking
  both artifacts.
- M4.2a adds the aggregate native lifecycle boundary: a corpus-bound builder
  ingests generic canonical record/chunk batches, Rust decodes and validates the
  canonical schema, finalization consumes the builder into one graph handle,
  and that handle supports composite save/load/validation. JSON is limited to
  cold schema and ingestion paths; M4.3 query hot paths remain typed C ABI.
- M4.2b adds the `VectorKitGraph` Swift product with generic `Encodable` schema,
  canonical record, metadata, and chunk types. Actor-owned builder/index handles
  preserve native ownership; finalization consumes the builder. Swift integration
  tests cover schema marshaling, record ingestion, consumed-builder rejection,
  and composite save/validate/load.

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
- With compact vector-only persistence (`persist_bm25=false`) and compact chunk
  metadata encoding, `24K x 768d I8ScalarQuantized` filtered measured locally at
  `19.581 MiB`, with `chunks.bin` down to about `1.888 MiB`.
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

Social Network real-data MiniLM benchmark:

- Built `target/examples/social-network-index-minilm` from the real Social
  Network fixture using Core ML `sentence-transformers/all-MiniLM-L6-v2`,
  `seq=256`, `384d`, cosine, and `I8ScalarQuantized`.
- Persisted index: `28,650` chunks, `31.346 MiB`.
- Query fixture: `target/examples/social-network-minilm-queries.json`.
- Swift exact vector search over the MiniLM index measured `0.470 ms` average,
  `0.466 ms` p50, `0.497 ms` p95, and `0.535 ms` p99 over `750` measured
  queries with `top_k=5`.
- Core ML MiniLM query embedding measured separately at `3.057 ms` average,
  `2.973 ms` p50, `3.545 ms` p95, and `5.493 ms` p99.
- Approximate embedding + Swift search: `3.527 ms` average, `3.439 ms` p50,
  `4.042 ms` p95, and `6.028 ms` p99. This is close to Moss's published
  `4.3 ms` p95, but should be replaced by a single Swift end-to-end benchmark
  once Swift-side tokenization/model execution is wired in.
- Source reports:
  `docs/product/reports/social-network-end-to-end-benchmark-report.md` and
  `docs/product/reports/social-network-minilm-swift-search-report.md`.

Local Rust CLI hybrid work after BM25/runtime optimizations:

- BM25 keyword search is no longer the main hybrid bottleneck for the current
  synthetic V1 benchmark. Runtime BM25 uses hash-backed postings, bounded
  top-k selection, metadata allowlists for filtered search, and cached active
  document frequency per term.
- Filtered hybrid search is fast when metadata filters narrow candidate sets.
  On `10K x 384d I8ScalarQuantized`, `top_k=10`, `100` queries:
  `filter_every=100` measured around `0.03-0.05 ms`, `filter_every=10`
  around `0.10-0.34 ms`, and `filter_every=2` around `0.49-1.58 ms`
  depending on candidate limits and fusion mode.
- Unfiltered hybrid latency is dominated by vector candidate count, not BM25.
  In the same benchmark, `10` vector candidates produced roughly
  `0.44-0.57 ms`; `25` vector candidates roughly `1.17-1.30 ms`; `50`
  vector candidates roughly `2.5 ms+`.
- Keyword candidate count is comparatively cheap after the BM25 changes.
- Hybrid candidate limits are intentionally public and per-query:
  `HybridQuery::with_candidate_limits(vector_top_k, keyword_top_k)`.
  The V1 fixture compares each pair with both a same-encoding `100/100`
  reference and F32 `100/100`. Keep `50/50` as the default because it is the
  smallest tested pair that meets the `0.95` I8-versus-F32 recall gate; callers
  can still choose smaller limits when latency matters more than overlap.

Physical-device validation using the iOS `Device` mode has now run on an
iPhone with iOS 26.5. Source report:
`docs/product/reports/iphone-device-validation-i8-report.md`.

| dim | filter | avg | p95 | load | persisted | observed RSS after load |
|---:|:---|---:|---:|---:|---:|---:|
| 384 | none | 0.471 ms | 0.506 ms | 22.54 ms | 12.772 MiB | 180.33 MiB |
| 384 | 1/10 | 0.111 ms | 0.173 ms | 28.68 ms | 12.910 MiB | 214.89 MiB |
| 768 | none | 0.640 ms | 0.658 ms | 25.62 ms | 21.561 MiB | 255.34 MiB |
| 768 | 1/10 | 0.167 ms | 0.256 ms | 27.83 ms | 21.699 MiB | 249.27 MiB |

Treat the RSS values as process-level sequential-run observations, not isolated
per-index memory footprints. Add one-scenario-per-launch device presets before
making memory-budget decisions from RSS.

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

## Python Wrapper Context

- `docs/agents/python.md` defines Python wrapper guidance. Python is currently
  an internal developer wrapper before the public Swift wrapper is finalized.
- `crates/vectorkit-python` exposes a thin PyO3 module that calls
  `vectorkit-core` directly.
- `wrappers/python` contains the maturin package. The public API is Pythonic:
  `Index.add(documents=[...])`, `Index.search(embedding, limit=10, where=...)`,
  `Index.keyword_search(...)`, `Index.save(...)`, `Index.load(...)`, and
  `delete_document(...)`.
- Embeddings are caller-provided. `search_text(index, text, embed=...)` is only
  a convenience helper that calls the supplied provider, validates one returned
  query vector, then calls vector search.

## EmbeddingKit Context

- EmbeddingKit lives separately from VectorKit under `wrappers/swift/EmbeddingKit`.
  VectorKit still accepts caller-provided embeddings and does not depend on an
  embedding runtime.
- Core ML model conversion is intentionally outside the Swift package. The
  generic conversion script is `scripts/embedding/convert-embedding-coreml.py`
  with a BGE compatibility wrapper at
  `scripts/embedding/convert-bge-small-coreml.py`. The process is documented in
  `docs/product/embedding-model-conversion.md`.
- Generated model artifacts should stay under
  `target/embedding-models/bge-small-en-v1.5/` and should not be committed by
  default.

## Ingestion Context

- Generic text chunking lives in the separate Rust `vectorkit-ingest` crate so
  retrieval remains isolated in `vectorkit-core`.
- Fixed and sentence-aware strategies use Unicode-character limits and overlap;
  returned ranges are UTF-8 byte offsets into the original text.
- Swift exposes chunking through the separate `VectorKitIngest` product and
  Python through `vectorkit.ingest`. Both call the same Rust implementation.
  Tokenizers differ by model, so exact token counting remains provider-owned.
- The optional Swift `VectorKitPipeline` package and Python
  `vectorkit.pipeline` module compose chunking, embedding, document upsert, and
  hybrid text search. They validate all embeddings before upsert, so provider
  failures leave the previous document version unchanged.
- The pipeline layer owns `DocumentChunker` in Swift and Python. Its default is
  sentence-aware Rust chunking at 500 characters with 50 characters of overlap;
  applications can override it for document-aware splitting. Pipeline validates
  custom text, ordering, and UTF-8 source ranges before embedding, so overrides
  do not weaken downstream index metadata.
- Token-aware pipeline chunking is implemented. Swift automatically uses an
  embedder's `TextTokenCounter` and `maxInputTokens`; Python accepts
  `count_tokens` and `max_tokens`. Oversized chunks are recursively subdivided
  while preserving original UTF-8 offsets. Providers without tokenizer access
  keep the character-based default.

## Completed Optimizations

- Crash-safe transactional persistence is implemented in the Rust core. Format
  V2 stores immutable generations under `.snapshots`, syncs and validates a new
  generation before atomically publishing `manifest.json`, and cleans stale
  generations after success. V1 root-file indexes remain readable and migrate
  on their next save. An OS-released file lock serializes cross-process saves.
  Failure-injection tests cover every pre-publication stage and lock contention.
- Explicit compaction is implemented in Rust and exposed through Swift and
  Python. It atomically rebuilds vectors, chunks, BM25, metadata filters, and ID
  lookups using only active chunks, preserves active IDs and monotonic ID
  allocation, reports estimated memory reclaimed, and is a cheap no-op without
  tombstones. Saving afterward publishes the compacted snapshot.

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
- BM25 runtime search is optimized for V1 hybrid retrieval. The in-memory index
  uses hash-backed term lookups and postings, bounded top-k selection, filtered
  metadata allowlists, O(1) active average document length, and cached active
  document frequency. Persistence remains deterministic through the existing
  sorted binary format.
- Hybrid search defaults to RRF with `rrf_k=60`; weighted normalized score
  fusion remains explicit. Result traces expose vector/keyword ranks, raw
  scores, normalized scores when applicable, matched terms, and fusion config.
- Hybrid candidate limits are exposed through the Rust public API with
  `HybridQuery::with_candidate_limits(vector_top_k, keyword_top_k)`.
- The CLI matrix benchmark can now vary filter selectivity and hybrid candidate
  limits with `--filter-every-values`, `--vector-candidates`, and
  `--keyword-candidates`.
- A `vectorkit-ffi` crate now exposes `vectorkit_bench_synthetic_json` and
  `vectorkit_string_free` for Swift/macOS/iOS benchmark harnesses. The default
  benchmark runs `24K` chunks, `384d` and `768d`, `f32`/`f16`/`i8`, and both
  unfiltered and `filter_every=10` filtered searches. FFI benchmark rows now
  also include persistence save time, load time, persisted file sizes, and
  post-load search latency by default. `persist_bm25=false` measures a compact
  vector-only persistence profile.
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
  smoke, device-validation, full default, and compact vector-only benchmark
  buttons, and the generic iOS Simulator build succeeds locally. The
  device-validation mode runs `24K` chunks, `384d`/`768d`, `i8`, filtered and
  unfiltered, persistence enabled, and `include_recall=false` so F32
  ground-truth indexes do not inflate RSS.
- `chunks.bin` now writes a v2 payload with a metadata field dictionary and
  compact varint integer/timestamp metadata values. The loader still accepts the
  older v1 chunk payload.
- `chunks.bin` and `bm25.bin` are now zstd-compressed at rest by default.
  Manifest fields record compression type and uncompressed byte counts, and load
  transparently decompresses before rebuilding in-memory structures.
- The FFI benchmark report is now schema version `2`. On Apple platforms it
  includes Mach task RSS snapshots for the whole report, per-run build/search
  phases, and persistence save/load/post-load-search phases.

## Likely Next Tasks

The consolidated execution order is maintained in
`docs/product/implementation-roadmap.md`. Checksummed V3 persistence and the
read-only validation API and the thread-safety/lifecycle contract are complete.
The active production slice is physical-device memory-budget validation. The
isolated Rust/FFI and iOS harness is implemented, and iPhone 17 Pro Max budgets
are checked in for 24K × 384d/768d I8 hybrid plus the 50K × 384d extended tier.
Rust/FFI explicitly permit
parallel immutable reads with exclusive mutation. Swift uses a
writer-preferring asynchronous gate plus detached native calls; Python releases
the GIL for Rust work, permits parallel shared searches, and rejects conflicting
exclusive operations through PyO3 borrowing.

- iPhone 17 Pro Max release results are in
  `docs/product/reports/iphone-17-pro-max-memory-budget-report.md`. The compact
  24K target stayed at or below 124.89 MiB peak RSS and 6.187 ms post-load P95
  across five runs. The 50K profile stayed near 198 MiB but reached 14.479 ms
  P95 and is classified as an extended-capacity tier.
- Run F16/F32 presets next, then repeat the compact target on older supported
  iPhone/iPad classes before generalizing budgets.
- `24K × 768d I8` passed at 18.015 MiB persisted and at most 7.827 ms P95;
  its observed peak RSS ranged from 133.84 to 162.42 MiB. A diagnostic
  `50K × 768d I8` run reached 37.519 MiB persisted and 220.02 MiB peak RSS, so
  it is not a compact profile.
- The 24K encoding comparison kept P95 below 10 ms in every case. Persisted
  size was 17.923/35.501 MiB for 384d F16/F32 and 35.501/70.658 MiB for 768d
  F16/F32. I8 remains the universal compact encoding; F32 remains the
  correctness reference.
- Retrieval-quality V2 is checked in under `benchmarks/retrieval-quality/v2`.
  It expands to 306 documents and 42 graded queries with ambiguity and competing
  documents. I8 `50/50` hybrid overlap versus F32 `100/100` is `0.9762`; human
  relevance recall is `0.9028`, MRR is `1.0`, and NDCG is `0.9272`.
- BM25-free I8 vector search retains `1.0` of F32 top 5 and `0.9976` of F32 top
  10, with `1.0` top-result agreement. Do not add an F16/F32 rerank store without
  production-derived evidence that it is needed.
- Smaller hybrid pools have slightly better V2 human relevance than `50/50`,
  despite lower reference overlap. Keep `50/50` provisional and revisit `25/25`
  when anonymized application queries and relevance feedback exist.
- `retrieval-quality-evaluation-standard.md` records the V3 gold-standard plan:
  TREC-compatible qrels/runs, pooled blind judgments, standard metric
  cross-checks, BEIR adapters, per-category gates, and paired confidence
  intervals. Moss uses qrels with Hit Rate, MRR, and NDCG on SciFact, NFCorpus,
  and mini MS MARCO; VectorKit should retain its additional fidelity and
  lifecycle checks.
- NIST TREC evaluation is a committed future milestone, but it does not block
  Phase 5 release distribution. First add TREC-compatible qrels/run output and
  standard metric cross-checks; after packaging is stable, run an appropriate
  official collection and evaluate TREC RAG participation.
- A manual-only GitHub Actions workflow can run Rust format/Clippy/tests, V2
  quality gates, Python lint/type/tests plus installed-wheel smoke, and a full
  Apple XCFramework build followed by Swift tests. It has no push or pull-request
  trigger and does not upload artifacts, create releases, or change Package.swift.
- Benchmark the I8 dotprod path on target iPhone/iPad/Mac hardware, especially
  older devices that may not report `dotprod`.
- Explore parallel exact scan only after target-device benchmarks show remaining
  CPU pressure.
- Validate the compact target and compaction headroom on target Apple devices.
