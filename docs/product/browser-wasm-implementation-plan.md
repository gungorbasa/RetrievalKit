# Browser/WebAssembly Implementation Plan

Status: initial additive implementation and SIMD128 acceleration complete
2026-07-26; cross-browser qualification remains in progress.

This plan is additive. It does not replace, refactor, or relax qualification
for the existing native Swift, Python, Node.js, Kotlin/JVM, Android, Rust, C,
or CLI implementations. Browser package publication, website deployment,
release tagging, and public performance claims require separate owner
authorization.

## Performance Contract

Browser qualification measures four independent budgets:

1. WASM/module and database startup.
2. Embedding generation.
3. Worker transfer and boundary conversion.
4. Retrieval, filtering, graph traversal, and ranking inside Rust.

Every benchmark records corpus size, dimension, encoding, query type, filter
selectivity, browser version, operating system, hardware, build profile,
WASM feature tier, p50, p95, peak memory, and whether the embedding model was
already warm. Retrieval-only numbers never include embedding inference.

The first qualification matrix covers:

- 10K, 25K, and 50K chunks;
- 384d and 768d embeddings;
- top-k 5 and 10;
- vector, BM25, weighted hybrid, graph-only, and graph-scoped retrieval;
- no filter, dense filter, and sparse filter;
- Chrome, Firefox, and Safari;
- named desktop and mobile device classes.

The browser requirement is p95 retrieval-only search latency no greater than
10 ms for every configuration claimed as supported. The preferred operating
range is 8–10 ms. This is an engineering gate, not a public claim. The initial
384d F32 portable baseline passes at 10K and 25K chunks but fails at 50K, so
50K×384d must not be qualified until a browser-only acceleration tier passes
this gate. A 50K×192d F32 diagnostic passes at 7.50 ms vector p95 and 8.11 ms
hybrid p95, but it is diagnostic evidence only and does not replace the
canonical native-parity 384d I8 profile. A change to the gate or qualified
vector profile requires recorded benchmark and quality evidence plus an
explicit product decision.
The 768d and compact-encoding budgets are frozen after the first reproducible
baseline. No end-to-end target may hide embedding or Worker transfer time
inside a retrieval-only measurement.

## Work Breakdown

### B0 — Product authorization

- Amend the product spec and roadmap.
- Freeze the three capability-specific products.
- Freeze native-isolation and no-publication requirements.

Exit: active product documentation authorizes the separate target.

### B1 — Portable WASM compilation slice

- Keep native Cargo defaults unchanged.
- Exclude filesystem persistence and `fs2` from WASM.
- Test SimSIMD's portable fallback for `wasm32-unknown-unknown`. Version 6.5.16
  compiles during `cargo check` but does not link because its build script can
  omit the C archive while the Rust crate still requests `-lsimsimd`.
- Use a WASM-only portable Rust scorer and half conversion after that measured
  upstream link failure. Keep native SimSIMD unchanged.
- Keep native C/NEON/AVX dispatch unchanged.
- Keep the initial WASM database in memory.

Exit: release-mode `wasm32-unknown-unknown` checks pass for core, graph, and the
new binding aggregate; native all-feature tests still pass.

### B2 — Browser binding and conformance

- Add a separate `wasm-bindgen` aggregate.
- Expose retrieval-only, graph-only, and combined databases.
- Accept contiguous embedding batches and typed structured inputs.
- Return compact top-k results with the native trace contract.
- Add checked-in native/WASM fixtures for vector, BM25, hybrid, filtering,
  graph projection, scoped retrieval, lifecycle, and deterministic errors.

Exit: native and WASM results are identical for every fixture.

### B3 — Worker-owned TypeScript API

- Add an asynchronous browser package independent of N-API.
- Own database handles inside one dedicated Worker.
- Use request IDs and deterministic lifecycle state.
- Transfer or bulk-copy embedding buffers once per operation.
- Coalesce or supersede obsolete interactive queries without corrupting
  database state.

Exit: browser integration tests prove no retrieval implementation runs on the
UI thread and operations after close fail deterministically.

### B4 — Portable performance baseline

- Benchmark startup, ingestion, transfer, retrieval, and memory separately.
- Record the scalar portable Rust baseline before optimizing. Retain the
  SimSIMD WASM link result as dependency evidence rather than a benchmark.
- Inspect WASM size and verify there is one retained vector representation.
- Profile filtered and unfiltered exact scans before changing algorithms.
- The first direct Node/WASM diagnostic is recorded in
  `reports/browser-wasm-portable-baseline-2026-07-26.md`. The portable 384d
  F32 scorer meets the retrieval-only gate at 10K and 25K chunks but misses it
  at 50K. Bulk ingestion is not yet qualified and currently scales poorly
  enough to require a separate additive optimization before browser release.

Exit: a reproducible report identifies the binding cost and hot scoring path.

### B5 — Measured acceleration

- Add WASM SIMD128 because B4 misses the frozen 50K×384d gate. A
  compiler-only `target-feature=+simd128` experiment did not improve the hot
  path, so the acceleration must use an explicit target-specific scoring
  implementation rather than relying on auto-vectorization.
- Prioritize the same signed-I8 dot-product path used by the compact native
  profile: 384 I8 values plus one F32 per-vector scale. Produce separate
  SIMD128 and portable artifacts and select the fastest supported artifact in
  the Worker before constructing a database.
- Feature-detect and retain a portable fallback.
- Test optional WASM threads only above a measured corpus-size threshold.
- Keep threaded deployment optional and document cross-origin isolation.
- Do not add ANN, quantization changes, or WebGPU retrieval in this phase.
- The explicit signed-I8 SIMD128 tier measures 1.80 ms vector p95 and 2.20 ms
  hybrid p95 at 50K×384d on the reference machine. Portable/SIMD result
  conformance passes at both the canonical 384d profile and a 396d scalar-tail
  case. Cross-browser qualification is still required before compatibility or
  public speed claims.

Exit: the simplest tier that meets the gates passes correctness, latency,
memory, and cross-browser checks.

### B6 — Optional byte snapshots

- Design a versioned, checksummed, sectioned binary snapshot.
- Add `save_to_bytes` and `load_from_bytes` in Rust.
- Store bytes through IndexedDB or OPFS in the TypeScript layer.
- Measure restore time, copy count, decompression time, and peak memory.
- Enable `zstd` only if the measured startup/size tradeoff is favorable.

Exit: restored results are conformant and warm startup meets its frozen gate.

### B7 — Browser qualification

- Run the complete device/browser matrix.
- Add performance-regression thresholds to CI at an appropriate stable tier.
- Record unsupported browser features and fallback selection.
- Perform package-content, license, and local-install checks.

Exit: a qualification report supports any proposed compatibility or speed
claim. Publication remains a separate owner decision.

### B8 — Optional browser embedding provider

- Keep embedding in an independent `wrappers/browser-embedding` distribution;
  do not add it to `retrievalkit-core`, `retrievalkit-wasm`, or the browser
  retrieval package.
- Own verified artifact acquisition, the tokenizer, ONNX Runtime Web session,
  warmup, and inference in a dedicated module Worker.
- Freeze direct FP32 MiniLM inference to the same immutable artifact revision,
  fixed 256-token behavior, and 384-value normalized F32 output contract used
  by the qualified native providers.
- Prefer WebGPU with same-model WASM operator fallback, and retain a
  deterministic WASM-only provider. Verify every shipped runtime asset and
  every downloaded model file by exact size and SHA-256.
- Qualify correctness against the frozen Rust FP32 vectors and report WebGPU
  and WASM startup/inference separately. Portable WASM is a correctness and
  compatibility fallback; its current 32-token p95 is about 19.8 ms and does
  not satisfy a 10 ms combined embedding-plus-retrieval budget.

Exit: artifact/cache failure cases, Worker lifecycle, fixed-token behavior,
actual provider inference, vector/ranking conformance, package contents, and
latency are recorded. Cross-browser WebGPU compatibility and package
publication remain separate gates.

## Hot-Path Rules

- Normalize cosine vectors during ingestion and the query once.
- Keep stored vectors contiguous in WASM memory.
- Use internal numeric IDs, compact active/filter membership, and bounded top-k
  selection inside Rust.
- Cross JS/Worker/WASM boundaries once per operation, never once per item.
- Keep Worker results proportional to top-k.
- Preallocate batch capacity where sizes are known and avoid avoidable
  `WebAssembly.Memory` growth.
- Preserve exact ranking, filtering, trace, generation, and tie-break semantics
  while optimizing.

## Explicit Non-Goals

- Changing native dependencies, APIs, persistence, performance paths, or
  packaging.
- Reusing the Node N-API addon in browsers.
- Filesystem emulation in the first browser artifact.
- ANN/HNSW, WebGPU retrieval, server mode, synchronization, or automatic graph
  construction.
- Publishing a browser package, deploying a site, tagging a release, or
  claiming browser performance before qualification.
