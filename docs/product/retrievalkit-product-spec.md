# RetrievalKit Product Spec

## Product Summary

Build a local-first retrieval SDK for mobile, desktop, and browser apps. The
first native public targets are iOS/macOS through Swift, macOS arm64 through
Python, Node.js LTS initially on macOS arm64 through TypeScript, and Kotlin/JVM
with Android arm64-v8a native packaging, backed by one Rust core. The Node.js
wrapper remains a native N-API target. Browser/WebAssembly is an additive,
separately packaged compile target and does not replace or alter any native
wrapper. The initial Kotlin target does not include Kotlin Multiplatform.

The SDK must provide fast, correct local retrieval over app-owned documents without requiring a vector database server.

Primary positioning:

```text
Fast, private retrieval for edge AI.
```

RetrievalKit is not a general vector database. It is a local retrieval engine with a developer-friendly SDK.

## Product Priorities

Priority order:

1. Fast local query path.
2. Correct retrieval behavior.
3. Simple APIs.
4. Predictable persistence and reload behavior.
5. Small-index optimization before ANN complexity.
6. Server mode and sync later.

Correctness means:

- The same input index and query produce stable, explainable results.
- Exact search can be used as the ground-truth baseline.
- Hybrid results expose enough trace data to debug why a document ranked.
- Deleted or outdated documents never appear in final results.
- Metadata filters are applied correctly.

Speed means:

- The hot path avoids JSON parsing, SQLite queries, network calls, and broad allocations.
- The index is already loaded before search.
- Query embedding latency is measured separately from retrieval latency.
- Result metadata needed for display is available through direct lookup.

## Target User

Initial target:

- iOS/macOS developers building AI apps.
- Node.js developers shipping local desktop or command-line AI features on
  macOS arm64.
- Web developers shipping private local retrieval and grounded document
  question-answering in supported desktop browsers.
- Android developers integrating through Kotlin/JVM on arm64-v8a devices.
- Apps with private local data.
- Apps that need offline semantic search or RAG.
- Apps with small local indexes: 1K to fewer than 50K chunks.

Example apps:

- Note apps.
- Transcript search.
- Personal knowledge bases.
- Local document assistants.
- Meeting/audio/video summarizers.
- Enterprise apps with private local data.

## V1 Scope

V1 must include:

- Rust retrieval core.
- Swift wrapper.
- Python wrapper with capability parity.
- TypeScript wrapper for Node.js LTS, initially on macOS arm64.
- Browser/WebAssembly wrapper with Worker-owned capability-separated databases.
- Independent local browser embedding provider.
- Kotlin/JVM wrapper with Android arm64-v8a native packaging.
- A public website demo that runs free-form document question-answering through
  local browser embedding, RetrievalKit WASM retrieval, and a browser SLM.
- Local persistent index.
- Exact vector search.
- BM25 lexical scoring as the internal lexical component of hybrid search.
- Hybrid ranking.
- Metadata filtering.
- Add/update/delete document operations.
- Batch indexing.
- Query API.
- Result trace/debug API.
- Benchmark CLI.
- Recall and latency reports.

V1 does not include:

- HNSW.
- ANN indexing.
- Hosted cloud service.
- Multi-user auth.
- Distributed indexes.
- Sharding.
- Realtime sync.
- Vector replication.
- Dashboard UI.
- Built-in embedding model training.
- General SQL query engine.

## Authorized Optional Embedding Providers

Embedding model execution is authorized as an additive, optional integration
layer. It does not move into `retrievalkit-core`, and no retrieval-capable
database constructs, downloads, selects, or invokes an embedding model.
Existing caller-produced embedding APIs and every existing language wrapper
remain unchanged.

The first shared provider experiment is frozen to
`sentence-transformers/all-MiniLM-L6-v2` revision
`c9745ed1d9f207416be6d2e6f8de32d1f16199bf`. Its cross-runtime contract is:

```text
maximum input: 256 WordPiece tokens
pooling: masked mean
normalization: L2
output: 384 finite F32 values
recommended metric: cosine
profiles: FP32, FP16, Q8
```

Native Rust exposes the cross-platform ONNX provider from the separate
`retrievalkit-embedding` crate. It remains optional and unchanged. Production
Swift embedding is direct Core ML through the provider-neutral
`wrappers/swift/EmbeddingKit` package; the completed Swift ONNX comparison and
its Apple ONNX Runtime build material are retired from active source,
packaging, and CI. Rust embedding crates remain source-only and are not
published to crates.io in v0.1.0.

Production Python and Node embedding integrations live in this monorepo but
ship as separate optional distributions:

```text
wrappers/python-embedding       retrievalkit-embedding
wrappers/typescript/embedding   @gungorbasa/retrievalkit-embedding
```

Both are thin language-native bindings over the separate
`retrievalkit-embedding` Rust crate. They expose only the canonical FP32
profile, preserve the fixed 256-token behavior, validate exactly 384 finite
L2-normalized F32 output values, and do not depend on a retrieval package.
Python exposes synchronous `load`, `prefetch`, `embed`, and `embed_batch`
operations while releasing the GIL around native model work. Node exposes
promise-based `load`, `prefetch`, `embed`, and `embedBatch`, performs blocking
native work away from the JavaScript event loop, and provides explicit
lifecycle cleanup. Their v0.1.0 registry identities are fixed as
`retrievalkit-embedding` and `@gungorbasa/retrievalkit-embedding`. Neither
package is published until the release gates pass.

Production Kotlin embedding is another independently distributable optional
integration:

```text
wrappers/kotlin/embedding          retrievalkit-embedding
wrappers/kotlin/android-embedding  retrievalkit-embedding-android
```

Both modules bind the separate Rust ONNX provider through the private
`retrievalkit-jni-embedding` aggregate and have no retrieval or graph
dependency. The blocking `OnnxEmbedder` surface is limited to FP32 `load`,
`prefetch`, `embed`, `embedBatch`, immutable model information, and
deterministic `close`. Empty or blank inputs and empty batches fail with typed
embedding errors. Output must contain exactly 384 finite, L2-normalized F32
values with the qualified 256-token limit. Kotlin adds no coroutine dependency;
callers select their own dispatcher. `AndroidOnnxEmbedder` places verified
artifacts under the application cache and returns the same `OnnxEmbedder`.
Their v0.1.0 Maven coordinates are fixed under `io.github.gungorbasa` as
`retrievalkit-embedding` and `retrievalkit-embedding-android`. Neither module
is published until the release gates pass.

Kotlin model acquisition is limited to `load` or explicit `prefetch`.
`localOnly` is network-free. The implementation reuses the Rust provider's
immutable HTTPS pins, OS/application cache, cross-process acquisition lock,
temporary files, exact size/SHA-256 verification, corrupt/partial cleanup, and
atomic publication. Kotlin/JVM bundles the verified official macOS arm64 ONNX
Runtime 1.24.3 library identified below. Android bundles only arm64-v8a from
the official `onnxruntime-android:1.24.3` AAR: the AAR is `40,948,335` bytes
with SHA-256
`67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce`,
and its selected `libonnxruntime.so` is `25,831,632` bytes with SHA-256
`4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad`.
Package builds fail closed on runtime or legal-file drift; generated native
binaries are not source files.

Browser embedding is a separate, independently distributable integration at
`wrappers/browser-embedding`. It does not bind Rust and does not import the
browser retrieval package or a Node package. A dedicated module Worker owns
verified model acquisition, tokenization, ONNX session creation, warmup, and
inference through the exact dependencies `@huggingface/tokenizers` 0.1.3 and
`onnxruntime-web` 1.27.0. The public promise-based surface is limited to
`load`, `prefetch`, `embed`, `embedBatch`, immutable model information, the
selected execution provider, and explicit close. It exposes only FP32, applies
the qualified 256-token behavior, and returns exactly 384 finite,
L2-normalized `Float32Array` values.

Browser model acquisition occurs only in `load` or explicit `prefetch`.
`localOnly` makes no model-artifact request and requires a fully verified
cache. The cache inventory is closed to the immutable manifest, FP32 ONNX
model, and four tokenizer files pinned below. Every response and every cache
hit is checked for exact size and SHA-256; a completion marker is written
last, corrupt or partial state is removed, concurrent module callers share one
acquisition, and the Web Locks API coordinates Workers where available. The
Worker entry, package JavaScript, and ONNX Runtime Web loader/WASM files are
application assets rather than model artifacts; a fully offline application
must serve or precache those assets separately.

Browser `execution: "auto"` first creates and warms WebGPU with same-model
WASM operator fallback, then creates a fresh WASM-only session if that attempt
fails. Explicit `"webgpu"` and `"wasm"` choices are strict. The package
verifies and ships the provider-specific ONNX Runtime Web 1.27.0 loaders and
WASM binaries at build time:

```text
ort-wasm-simd-threaded.mjs
  24,180 bytes
  0a1e718d99c41b22c21f2520ff4f9e883a6b5533856e398d21816ee8eb8185d3
ort-wasm-simd-threaded.wasm
  13,479,978 bytes
  d1ab1b94b16a65b29d710d0b587b29e7bed336827577623913479b8afe8113e6
ort-wasm-simd-threaded.asyncify.mjs
  47,507 bytes
  7236653b8565da4046e459cd0e274123419a1d9f1f8f18fd36c28058346ca655
ort-wasm-simd-threaded.asyncify.wasm
  24,254,953 bytes
  7e83cd6cee77e478bc96a7e91b198144fb5e4126287daf1f9b54bb195ebcd55a
```

The v0.1.0 browser embedding registry identity is fixed as
`@gungorbasa/retrievalkit-browser-embedding`. It is not published until the
release gates pass.

On the Apple M1 Max reference host, a real Chrome 150 dedicated-Worker WebGPU
run with 50 warm-ups and 750 measured 32-token queries produced warm p95
`7.500 ms`. The matching 50K×384d I8 SIMD128 retrieval rerun produced vector
p95 `1.887 ms` and hybrid p95 `2.250 ms`. A later production same-page run
through the separate embedding and retrieval Workers measured Chrome WebGPU
embedding p95 `10.560 ms`, retrieval p95 `1.905 ms`, and end-to-end p95
`12.405 ms`; therefore the earlier separately summed sub-10-ms estimate is not
an end-to-end pass. Actual ONNX Runtime Web WASM produced all 94 frozen
vectors with median cosine `0.9999999999996866` and 100% mean/exact/minimum
Top-10 agreement versus Rust FP32. Its warm p95 was `19.804 ms`, so WASM is a
correct compatibility fallback but not a sub-10-ms tier. Chrome qualification
now includes real CacheStorage and the full 50K same-page path. Firefox 150
passes the same correctness/cache/retrieval matrix through the WASM embedding
fallback with `21.660 ms` end-to-end p95. Safari, mobile GPUs, private-mode
cache behavior, and naturally occurring CacheStorage quota/eviction behavior
remain release gates.

A 2026-07-28 Chrome phase profile localized the 50K regression inside ONNX
Runtime Web inference rather than Worker RPC: the 256-chunk runtime/public
embedding p95 values were `7.005/7.090 ms`, while the 50K values were
`10.515/10.685 ms`. Removing two redundant post-inference F32 copies preserved
all validation but did not materially change the production result; the final
uninstrumented 50K embedding/retrieval/end-to-end p95 values were
`10.610/1.995/12.460 ms`. Precision, the 32-token benchmark input, corpus size,
provider, retrieval semantics, and separate Worker ownership must not be
weakened to manufacture a pass.

The owner accepted provider-tiered browser qualification budgets on
2026-07-28. On the fixed Apple M1 Max reference contract above, WebGPU
embedding plus SIMD128 retrieval must have end-to-end p95 at or below `15 ms`;
the deterministic WASM embedding compatibility tier plus SIMD128 retrieval
must have end-to-end p95 at or below `25 ms`; retrieval-only p95 remains at or
below `8 ms`. These are qualification budgets for the named reference
contract, not universal device latency guarantees. Chrome passes the
accelerated tier at `12.460 ms`, and Firefox passes the compatibility tier at
`21.660 ms`. Browser performance is therefore accepted for the qualified
Chrome and Firefox providers. Safari 26.5.2 is now functionally qualified:
it selected WebGPU, passed all correctness/cache gates, and measured retrieval
p95 `1.940 ms` and end-to-end p95 `18.380 ms`. On 2026-07-28 the owner
accepted a Safari-specific WebGPU reference budget of `20 ms`, so Safari
passes. This platform-specific budget does not change the general `15 ms`
WebGPU budget used for Chrome. Safari performance optimization is deferred;
mobile-device measurements remain open.

The Rust provider uses the official ONNX Runtime 1.24.3 binary through
`ort` 2.0.0-rc.12's API-24 dynamic-loading boundary and requires the
application to supply the packaged runtime library path. It must not silently
substitute the crate's older prebuilt runtime. `retrievalkit-core` does not
depend on ONNX Runtime, Core ML, an embedding library, HTTP, or model download
code.

Python, Node, and Kotlin acquire model files only while loading an embedder or
through explicit prefetch. `local_only`/`localOnly` prohibits network access.
All three use
the Rust provider's OS cache, immutable HTTPS artifact URLs, exact size and
SHA-256 verification, cross-process file lock, temporary download files,
atomic publication, and corrupt/partial cleanup. Their independently built
macOS arm64 packages may bundle the official ONNX Runtime 1.24.3 library only
after verifying its exact size (`27,724,968` bytes) and SHA-256
`b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729`
and including its license and third-party notices. The runtime binary is not
stored in this repository. Retrieval database construction, indexing, search,
graph operations, and persistence remain network-free and embedding-neutral.

On the Apple M1 Max reference host, Kotlin/JVM produced all 94 frozen FP32
vectors with median cosine `1.0`, minimum cosine
`0.9999999999998386`, and 100% mean/exact/minimum Top-10 agreement against the
Rust FP32 reference. A release run with 50 warm-ups and 750 measured 32-token
queries produced warm embedding p95 `8.175 ms` through the final packaged
JAR. The frozen native I8 retrieval p95 is `0.218 ms`, so the separately
measured sum is `8.393 ms`, below the 10 ms combined gate; retrieval-only
remains below 8 ms. Both Kotlin/Rust directions also pass the actual
RetrievalKit vector, hybrid, graph-scoped vector, and graph-scoped hybrid I8
gates, while BM25, graph-scoped BM25, and graph-only selection remain exactly
identical. Android qualification currently covers cross-compilation and closed
arm64-v8a AAR inspection; live device inference remains a release gate.

The Rust ONNX artifacts remain pinned in public repository
`gungorbasa/retrievalkit-minilm` at immutable commit
`617ce926c1f9e0289365d3e999474cc28b1645d4`; the SHA-256 of their
`manifest-v1.json` is
`b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`.

Production Swift pins the deterministic uncompressed POSIX-ustar archive
`all-MiniLM-L6-v2-coreml-fp32-v1.tar` from the same repository at immutable
commit `405818d6afef1aaf2fc8da67da6caf20b55f0a28`. The archive is
`90,664,960` bytes with SHA-256
`e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f`;
its `archive-manifest-v1.json` is `2,029` bytes with SHA-256
`085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60`,
and its canonical payload-tree SHA-256 is
`29f56defb74316d8491e7fba4eeba98cf24dc10b0e2b5b1df4a2d4e352f5fe5c`.
Production code never resolves `main`.

`CoreMLEmbedder.load(...)` downloads only while constructing the embedder, and
`CoreMLEmbedder.prefetch(...)` makes acquisition explicit. Both use HTTPS,
verify the archive's exact size and SHA-256, safely extract only the closed
manifest inventory, verify every payload size and SHA-256, publish cache state
atomically, and deduplicate concurrent in-process acquisition. `localOnly`
performs no network request. The package compiles the `.mlpackage` locally with
official Core ML APIs and keys compiled cache state by immutable artifact
identity plus OS/Core ML compatibility; a failed compiled-model load removes
that cache and recompiles once. Retrieval database initialization and every
indexing, retrieval, graph, and persistence method remain network-free.

Embedding-model precision and database-vector encoding are independent.
FP32 is the canonical ONNX/Core ML inference profile because direct provider
rankings had 100% Top-10 agreement. Production Swift uses only the fixed
256-token FP32 Core ML package, defaults to Core ML compute units `.all`, and
returns exactly 384 finite, L2-normalized F32 values. RetrievalKit accepts those
F32 values publicly and applies its existing `I8ScalarQuantized` database
encoding by default: one signed byte per dimension plus one F32 scale per
vector. After I8 encoding, both cross-provider database/query directions
measured 99.76% mean Top-10 overlap and 97.62% exact Top-10 sets. Selecting a
Q8 embedding model does not make a RetrievalKit database smaller than storing
FP32-model output with `I8ScalarQuantized`, and Q8/FP16 are not production
Swift runtime choices.

The Q8 profile is runtime-specific while preserving the shared output
contract: ONNX uses dynamic signed-INT8 transformer quantization while retaining
seven quality-sensitive MatMul nodes in full precision, and the Core ML
comparison uses broadly compatible weight-only INT8. A profile is called faster
or suitable as a default only after measured quality and latency qualification.
Index and query embeddings should use the same artifact profile unless a
cross-profile combination has separately passed conformance.

Provider qualification is separate from artifact qualification. Historical
Q8 and FP16 comparisons remain in the dated experiment report, but they do not
form a production Swift fallback. Direct Core ML Q8 did not improve latency,
and ONNX CPU Q8 plus the ONNX Core ML execution-provider Q8 missed their
quality gate through actual SDK boundaries.

Cross-provider FP32 qualification requires median cosine at least 0.9999, mean
Top-10 overlap at least 99%, exact Top-10 sets on at least 90% of queries, and
no query below 90% overlap. The same ranking gates apply after per-vector
signed-I8 database encoding when either provider builds the database and the
other produces the query.

The Rust ONNX CPU provider uses bounded intra-operation parallelism selected at
construction (up to four threads by default) and keeps inter-operation
parallelism at one. Applications may override both values. This setting belongs
only to the optional embedding provider and does not alter retrieval scoring,
database behavior, or an existing language wrapper.

## Authorized Browser/WebAssembly Target

Browser/WebAssembly is an additive target authorized on 2026-07-26. It must
reuse the Rust retrieval and graph implementations without changing the code
paths, dependencies, packaging, or measured performance of native Swift,
Python, Node.js, Kotlin/JVM, or Android builds.

The browser package exposes the same capability-separated products:

```text
RetrievalDatabase      = CorpusIndex + RetrievalIndex
GraphDatabase          = CorpusIndex + GraphEngine
GraphRetrievalDatabase = CorpusIndex + GraphEngine + RetrievalIndex
```

`RetrievalDatabase` and `GraphRetrievalDatabase` support embedding-only exact
vector search, text-only BM25 search, and text-plus-embedding hybrid search
with query-time `alpha`. `GraphDatabase` performs graph traversal and
projection without accepting embeddings. `GraphRetrievalDatabase` additionally
supports graph-scoped vector, BM25, and hybrid retrieval through
generation-bound selections.

The browser architecture is fixed for its first implementation:

- A separate `wasm-bindgen` Rust boundary and browser TypeScript package are
  used. The existing N-API/Tokio Node.js wrapper is unchanged and is never
  bundled into a browser.
- Every database is owned by a dedicated Web Worker. Indexing, graph building,
  traversal, filtering, scoring, and ranking do not execute on the UI thread.
- The initial database is in-memory. Filesystem directory persistence,
  cross-process locks, mmap, and native compressed snapshots are excluded from
  the WASM build rather than emulated.
- Browser persistence remains deferred. A future versioned
  `save_to_bytes`/`load_from_bytes` design may store snapshots through the
  TypeScript layer in IndexedDB or OPFS, but it must not be treated as a
  committed cross-platform format until portability, compatibility, security,
  size, startup, and migration requirements are specified and qualified.
- Embeddings remain caller-produced at every retrieval API. Applications may
  use the independent `wrappers/browser-embedding` provider to produce them,
  but the retrieval package does not import, construct, download, or invoke
  that provider. Embedding inference is not part of the Rust retrieval core
  and retrieval does not require WebGPU.
- Structured values use typed JavaScript/WASM conversion. Query paths must not
  serialize or parse textual JSON.
- Embeddings cross the boundary in contiguous `Float32Array` batches.
  JavaScript must not invoke WASM once per vector, chunk, or result.
- The default browser execution tier is one Worker with single-threaded exact
  scoring. WASM SIMD128 and threaded WASM are separate benchmark-gated
  optimizations. Threaded execution must remain optional because it adds
  cross-origin-isolation deployment requirements.
- The canonical compact browser profile matches the native product:
  384-dimensional caller-provided F32 embeddings stored and scored as
  per-vector symmetric `I8ScalarQuantized` values with zero-point 0 and one
  F32 scale per vector. The browser must not substitute a lower-dimensional
  embedding profile merely to satisfy latency. Dimension remains inferred and
  fixed per database, but any profile other than the qualified 384d profile
  requires its own quality evidence.

Native SimSIMD remains unchanged. The first WASM slice tested SimSIMD 6.5.16:
Rust compilation succeeds, but its build script can fail to emit the C archive
for `wasm32-unknown-unknown` and still request `-lsimsimd`, so a release artifact
does not link. The initial browser tier therefore uses an explicitly
WASM-only portable Rust scoring and half-conversion fallback. Native targets
continue to use SimSIMD's existing dynamic dispatch without source or feature
changes. WASM SIMD128 was added only after the portable Rust path missed the
frozen browser performance gates. `zstd` remains available to native builds;
it is omitted from the initial in-memory browser artifact and may be enabled
later for byte snapshots only after startup, size, and decompression
measurements justify it.

The portable WASM I8 scorer does not meet the 50K latency gate. The browser
therefore requires an optional SIMD128 artifact with explicit signed-I8 dot
product acceleration and a portable fallback. This is a browser-target
implementation detail; it must not change native SimSIMD, NEON/dot-product,
Python, Node, Swift, Kotlin, Android, persistence, or package behavior.

The browser target retains the V1 fewer-than-50K-chunk exact-search envelope.
It does not authorize HNSW, ANN, WebGPU retrieval, server mode, synchronization,
or a different ranking implementation. Browser and native results must match
on checked-in conformance fixtures, including filtering, lifecycle,
deterministic tie-breaking, traces, graph projection, and stale-selection
rejection.

Browser performance qualification reports cold startup, warm startup, batch
ingestion, retrieval-only latency, embedding latency, end-to-end latency,
Worker transfer time, and peak memory separately. It covers 10K, 25K, and 50K
corpora at 384 and 768 dimensions, top-k 5 and 10, dense and sparse filters,
vector/BM25/hybrid queries, graph traversal, and graph-scoped retrieval. Every
reported latency includes p50 and p95. No browser speed claim is permitted
outside the named qualified desktop-browser contracts, and mobile-device
qualification remains a separate release gate.

## Public Website Local Q&A Demo

The public website demo is an acceptance surface for the browser packages, not
a separate retrieval implementation. Website source and deployment remain in
the private `gungorbasa/RetrievalKit-Website` repository.

The complete demo pipeline is:

```text
curated first-party documents
  -> deterministic Python/CLI corpus-pack build
  -> chunks, source offsets, document embeddings, metadata, and graph records
  -> validated versioned corpus pack
  -> in-memory RetrievalKit WASM database build
  -> arbitrary visitor question
  -> local browser query embedding
  -> exact/BM25/hybrid RetrievalKit search
  -> bounded retrieved evidence and trace
  -> grounded browser SLM answer
  -> verified citation mapped to the original document span
```

Demo requirements:

- Visitors do not upload documents. The site ships one or more curated,
  redistribution-safe first-party documents with stable source identities.
- Python or the RetrievalKit CLI prepares the bundled corpus pack before
  deployment. The browser validates its version and integrity, then builds the
  database through the real WASM core. Query embeddings remain local in the
  browser. The UI exposes honest asset, validation, indexing, and model
  progress.
- Model assets and deterministic document inputs may be cached only with
  version and integrity checks. The current WASM database is in-memory;
  persistent database snapshots must not be claimed until the versioned
  `save_to_bytes`/`load_from_bytes` work is implemented and qualified.
- Visitors may type arbitrary questions. Suggested questions reduce onboarding
  friction but must use exactly the same live pipeline and must never select a
  canned answer by question text.
- The website owns SLM selection, prompt construction, and orchestration.
  RetrievalKit remains model-agnostic. The SLM receives only the question,
  bounded retrieved passages, and the answer contract—not the full corpus.
- Each exact citation includes a document identity and verbatim evidence quote.
  The application validates that quote against a retrieved chunk before mapping
  its range through retained source offsets. Only validated spans may be
  underlined or highlighted; invalid citations fall back to passage evidence.
- If the retrieved documents do not support an answer, the UI says so. A hosted
  answer or retrieval fallback must never be presented as a successful local
  demo run.
- Questions, retrieved context, embeddings, and generated answers remain in the
  browser and are excluded from telemetry. Once pinned assets are loaded, a
  query requires no network request.
- The UI reports embedding, retrieval, time-to-first-token, and total answer
  latency separately and exposes the live RetrievalKit trace.

### Portable Corpus Packs And Deferred Cross-Platform Snapshots

The first website demo uses data-model interchange, not storage-level
interchange. Python or the RetrievalKit CLI emits a versioned, checksummed
portable corpus pack containing canonical chunks, retained source offsets,
precomputed document embeddings, metadata, graph schema and records, aliases,
and evaluation inputs. The browser validates that pack and constructs its
Worker-owned in-memory WASM database through the public builders.

This corpus-pack contract is intentionally independent from RetrievalKit's
native transactional persistence layout. It avoids requiring Swift, Python,
Node, Kotlin, Android, and WebAssembly to load one permanent binary database
format before cross-platform compatibility requirements are understood.
Precomputed document embeddings provide most of the initialization benefit;
each target may still build inexpensive exact, BM25, metadata, and graph
structures locally.

A portable byte snapshot that can be built on one target and loaded on another
is deferred future work. Its design discussion must cover:

- distribution snapshots versus platform-native runtime persistence;
- schema versioning, compatibility windows, migrations, and rollback;
- canonical numeric representation, endianness, alignment, and optional
  derived-index sections;
- integrity, signing, hostile-input validation, and allocation limits;
- compression, download and app-bundle size, peak memory, copying, and startup;
- query-embedding model identity and document-vector compatibility;
- import into native transactional layouts on iOS, macOS, Android, Python,
  Node, and Kotlin, and direct in-memory restoration plus IndexedDB or OPFS
  caching in browsers; and
- qualification proving identical corpus identities, graph behavior,
  retrieval results, and lifecycle guarantees across targets.

Do not claim portable database snapshots until that contract is specified,
implemented, benchmarked, and cross-platform qualified. Revisit implementation
after corpus-pack import measurements show that rebuilding derived structures
is a material startup cost, or when signed prebuilt databases are required as
cross-platform distribution artifacts.

The browser retrieval package is implemented and desktop-qualified but remains
unpublished. The browser embedding package is included in the v0.1.0 release
inventory. The private website repository ships a
versioned Apollo 11 corpus pack and builds one combined
`GraphRetrievalDatabase` in RetrievalKit WASM. Vector mode uses local MiniLM
ranking followed by a Qwen3 0.6B answer. Graph Text validates and executes
Qwen-selected graph plans before Qwen answers from graph-selected source
paragraphs. Combined mode validates a Qwen-selected path ending at `Passage`,
projects the resulting graph selection, applies it as the `within` boundary for
MiniLM vector search, and asks Qwen to select exact source facts. The
application renders the selected NASA sentences and citations rather than
accepting generated combined-answer prose.

The production demo is deployed at
`https://retrievalkit-docs.gungorbasa.chatgpt.site/demo/`. Its full pipeline is
release-qualified on the tested Apple-silicon, Chromium-based desktop WebGPU
environment. The website build enforces a 64 MiB uncompressed client-artifact
limit, a 1 MiB corpus-pack limit, and a 460 MiB combined remote-model limit;
current observations are approximately 46 MiB, less than 0.1 MiB, and 443 MiB.
WebLLM reports approximately 1.4 GB of required GPU memory for the selected
Qwen build. Safari, Firefox, Android, iPhone, and iPad remain unqualified until
the complete model, Worker, and WASM pipeline is run on physical target
hardware.

## Optional Local Graph Roadmap

RetrievalKit may add graph retrieval as a separate, optional, fully local package.
This roadmap is additive to the graph-free product. `retrievalkit-core` remains
graph-neutral. A canonical `CorpusIndex` owns records, chunks, stable identity
maps, generations, lifecycle, and hydration. `RetrievalIndex` and the optional
`GraphEngine` are rebuildable derived capabilities over that corpus; neither is
an independent payload owner. Base native aggregates and non-Swift base
distributions must not link graph code, open graph files, initialize graph
state, or route ordinary queries through graph-aware dispatch. The public Swift
distribution is an explicit packaging exception: it ships one graph-capable
native aggregate so base and graph Swift products can coexist, while selecting
only `RetrievalKit` keeps graph APIs and graph initialization out of the Swift
target.

The supported database products are fixed during builder creation:

```text
RetrievalDatabase      = CorpusIndex + RetrievalIndex
GraphDatabase          = CorpusIndex + GraphEngine
GraphRetrievalDatabase = CorpusIndex + GraphEngine + RetrievalIndex
```

Every retrieval-capable database builds exact-vector and BM25 state and supports
both semantic and hybrid queries. Hybrid blending is selected at query time with
`alpha`, where `1` is vector-only, `0` is BM25-only, and values between them use
weighted normalized-score fusion. BM25 may be omitted from a compact persisted
snapshot, but it is rebuilt from canonical chunk text when that snapshot is
loaded. Keyword-only search remains an internal benchmark surface rather than a
standalone high-level product mode. Graph-only builders accept neither vector
configuration nor embeddings. Combined graph selections become opaque
generation-bound candidate scopes consumed by the retrieval capability.
Graph-only and combined databases may also materialize a selection as stable,
lexically ordered `(RecordId, ChunkKey)` identities, optionally intersected by
the owning corpus's production metadata filter. The corpus must reject stale or
cross-corpus scopes before filtering or materialization; internal candidate IDs
and sparse/dense membership remain private.

"Store once" means one canonical source record and payload owner, not one
physical representation. Chunks, vector arrays, BM25 postings, flattened
metadata indexes, stable-ID maps, and future graph adjacency are rebuildable
derived structures. The query hot path continues to use dense internal `u64`
`ChunkId` values. Stable external `RecordId`, `ChunkKey`, and future `NodeId`
values resolve into generation-bound internal IDs before scoring.

The roadmap is gated and must progress in order:

```text
M0 product authorization + generic conformance contract
  -> M1 graph-neutral RecordStore, stable IDs, CandidateScope,
        scoped exact/BM25/hybrid search, and bulk hydration
      -> M2 optional Rust schema + bounded graph engine
          -> M3 composite persistence + crash recovery
              -> M4 first customer-selected wrapper
                  -> M5 second wrapper + migration cutover
```

Milestone gates:

- M0 defines a domain-neutral conformance contract without inventing customer
  facts. Generic implementation uses synthetic fixtures spanning different
  record, field, reference, collection, cycle, update, and deletion shapes.
  Sanitized customer fixtures remain private acceptance evidence rather than a
  prerequisite or a source of hard-coded schema concepts.
- M1 is graph-neutral. It preserves the existing unscoped exact, BM25, and
  hybrid function bodies wherever practical, adds generation-bound adaptive
  candidate scoping and bulk hydration, and proves scoped/unscoped equivalence,
  stale-generation rejection, filter intersection, and lifecycle correctness.
- M2 may start after M1 tests and the graph-free unscoped latency gate pass.
  It implements the generic typed schema and concrete bounded local engine
  against synthetic conformance and scale fixtures. Its published capacity
  envelope remains provisional until representative real workloads and pinned
  target-device measurements establish headroom; workloads outside that
  envelope trigger a separate embedded-backend design.
- Every later milestone must preserve a coherent retrieval/graph generation,
  deterministic results, typed failures, local-only operation, and one linked
  core/state universe in graph-enabled wrappers.

The canonical graph schema is defined once in Rust and persisted inside the
graph-enabled database. Python and Swift builders marshal the same typed schema
IR; they do not implement schema validation or maintain synchronized JSON
sidecars. JSON export may exist for inspection or one-time migration only.

M3 composite persistence uses one manifest to activate one immutable
capability generation:

```text
graph_database/
  manifest.json
  .snapshots/
    <safe-generation-id>/
      corpus/               # canonical records/chunks/identity generation
      retrieval/            # present only when retrieval is enabled
      graph/                # present only when graph is enabled
      schema.json           # present only when graph is enabled
```

Saving writes the corpus and enabled derived payloads into a staging generation,
checks payload sizes and BLAKE3 checksums, reopens the corpus, validates every
derived payload against that exact corpus/generation, syncs the staged files
and directories, renames the generation into place, and atomically replaces
`manifest.json`.
Only that manifest selects an active generation. A failure before manifest
replacement leaves the previously active generation queryable. Read-only
validation follows the complete load path and does not clean staging or old
generations. An OS-released exclusive database lock serializes writers. Loaders
briefly hold a shared database lock while selecting and leasing the active
generation, then retain a shared per-generation lease for the lifetime of the
loaded database. Locked-save recovery removes abandoned staging and
unreferenced generations only when no reader lease is present.

The detailed ownership, Swift API, error, qualification, and commit contract is
defined in `docs/product/capability-separated-architecture.md`.

M4 Swift packaging uses one public graph-capable native artifact.
`retrievalkit-ffi` keeps its graph dependency behind an off-by-default Cargo
feature, and internal qualification still builds graph-free `RetrievalKitFFI`
to prove base isolation. The public Swift package resolves only
`RetrievalKitGraphFFI`, built from the same crate with the `graph` feature and
exporting both retrieval and graph entry points. It exposes independently
selectable `RetrievalKit` and `RetrievalKitGraph` products, and applications may
use either or both without linking competing native aggregates.

This is a deliberate Swift distribution and developer-experience exception to
the native capability boundary. Its cost is that a base-only Swift consumer
downloads the graph-capable binary. Its benefits are one package repository,
one version, one native handle universe, and direct base-plus-graph composition.
The exception does not move graph state into base database types: graph files,
initialization, and dispatch remain opt-in through graph APIs. Tests must prove
that the combined products link and run together and that the internal
graph-free artifact remains graph-neutral.

M5 Python packaging follows the same capability boundary. The base `retrievalkit`
distribution builds `retrievalkit-python` without graph features. The optional
`retrievalkit-graph` aggregate builds the same binding crate with its `graph`
feature and provides graph-only and combined graph-and-retrieval APIs. A
combined database exposes separate `graph` and `retrieval` query namespaces.
Applications may install both distributions for environment compatibility but
must import only one native distribution in a process.

TypeScript and Kotlin follow the same aggregate boundary. TypeScript uses the
separate `@gungorbasa/retrievalkit` and
`@gungorbasa/retrievalkit-graph` Node packages backed by separate napi-rs
native aggregates. The initial supported runtime range is
`^22.13.0 || ^24.0.0` on macOS arm64, with no browser or WebAssembly claim.
Kotlin uses separate base and graph-capable Kotlin/JVM modules under the fixed
`io.github.gungorbasa` Maven group, backed by thin JNI aggregates. The initial
native targets are macOS arm64 for JVM use and Android arm64-v8a, with no
Kotlin Multiplatform claim. Their release target IDs are `jvm-macos-arm64` and
`android-arm64-v8a`. Base consumers in either ecosystem must not load or depend
on graph code, and applications must not load both native aggregates in one
process. These npm names and Maven coordinates are fixed for `0.1.0`, but the
SDK packages remain unpublished until the release gates pass.

The first optional graph release is limited to deterministic explicit
references, reference collections, document/chunk structure, bounded typed
traversal, and only the retrieval composition mode proven by the customer
fixture. Arbitrary Cypher, automatic model extraction, PageRank and broad graph
analytics, SQL metadata storage, ANN/HNSW, incremental graph mutation,
and Kotlin Multiplatform remain out of scope until separately authorized.
Browser/WASM is governed by the separate authorized target above and does not
expand the first native graph release.

## Small-Index MVP Strategy

The first product version is optimized for fewer than 50K chunks.

The benchmark-only `100k-384d-v3-stress` workload is experimental scaling
evidence outside this supported envelope. It does not create a product,
support, quality, latency, or marketing claim; it does not affect the
10K/25K/50K product gate; and it does not authorize ANN/HNSW. Any future
capacity expansion requires a separate product-spec decision.

V1 goal:

```text
Make exact local hybrid retrieval extremely fast, correct, and easy to debug on iOS/macOS.
```

This deliberately defers HNSW and other ANN techniques.

Why:

- Exact search is perfectly accurate.
- Exact search is simpler to implement and test.
- Exact search is easier to combine with filters.
- Under 50K chunks, exact search can be fast enough with careful memory layout and SIMD.
- BM25, hybrid ranking, filtering, tracing, persistence, and Swift ergonomics are more important than ANN complexity at this stage.

V1 success means:

```text
<50K chunks
384d or 768d vectors
top_k 5-10
retrieval-only latency around 5-10 ms on modern iPhone hardware
deterministic exact results
high-quality hybrid retrieval
```

HNSW should only be reconsidered after the exact/hybrid engine is polished and benchmarks show exact search cannot meet the target for real user datasets.

## Core Concepts

### Canonical Record

`RecordStore` is the graph-neutral canonical payload owner for a corpus. A
record has a byte-exact stable `RecordId`, an ASCII `RecordType`, optional
content, and typed nested fields. Retrieval chunks are derived from records and
use stable caller/chunker `ChunkKey` values. Each generation persists a checked
mapping from `(RecordId, ChunkKey)` to the active dense internal `u64 ChunkId`.
Replacing a record retires its previous internal IDs and rebinds unchanged
external chunk identities to the new generation. Deletion removes the record,
its stable mappings, and every derived active chunk together.

The existing document/chunk ingestion shape is a one-record adapter. It assigns
positional chunk keys and is appropriate when callers do not need edit-stable
chunk identities. Graph and other structured-index integrations use the
record-first API with explicit stable chunk keys.

### Document

A document is the user-level object.

Examples:

- A note.
- A transcript.
- A PDF page.
- A video segment.
- A support article.

Required fields:

```rust
pub struct Document {
    pub id: String,
    pub text: String,
    pub metadata: Metadata,
}
```

### Chunk

A chunk is the retrievable unit.

Documents may contain one or many chunks. Search returns chunks, not whole documents. The caller can group chunks by document if needed.

Required fields:

```rust
pub struct Chunk {
    pub chunk_id: u64,
    pub document_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: Metadata,
    pub deleted: bool,
    pub version: u64,
}
```

Rules:

- `chunk_id` is an internal integer ID.
- `document_id` is caller-owned.
- All embeddings in one index must have the same dimension.
- If cosine similarity is configured, the index normalizes stored vectors on
  insert and normalizes query vectors once before search.
- Updates create a new version and mark old chunks inactive.
- Deleted chunks must be filtered from every final result set.

### Metadata

V1 metadata supports simple typed fields:

- string
- integer
- float
- boolean
- timestamp as integer milliseconds

V1 filters support:

- equals
- not equals
- in
- range for numeric/timestamp values
- exists

Do not support arbitrary nested JSON filters in V1.

## Local Index Layout

Use separate storage for each responsibility.

```text
index_directory/
  manifest.json
  .snapshots/
    <generation>/
      vectors.vec
      chunks.bin
      records.bin
      bm25.bin
      tombstones.bin
```

### Manifest

`manifest.json` stores compatibility and validation data.

Required fields:

```json
{
  "format_version": 4,
  "snapshot_id": "<safe-generation-id>",
  "created_with": "retrievalkit",
  "dimension": 384,
  "metric": "cosine",
  "vector_count": 24000,
  "active_chunk_count": 23500,
  "has_bm25": true,
  "has_records": true,
  "vector_encoding": "f32",
  "vector_bytes": 36864000,
  "chunk_bytes": 45678,
  "chunk_compression": "zstd",
  "chunk_uncompressed_bytes": 123456,
  "records_bytes": 12345,
  "records_compression": "zstd",
  "records_uncompressed_bytes": 45678,
  "bm25_bytes": 34567,
  "bm25_compression": "zstd",
  "bm25_uncompressed_bytes": 123456,
  "tombstone_bytes": 24000,
  "checksums": {
    "algorithm": "sha256",
    "vectors": "<64-lowercase-hex-characters>",
    "chunks": "<64-lowercase-hex-characters>",
    "records": "<64-lowercase-hex-characters>",
    "bm25": "<64-lowercase-hex-characters>",
    "tombstones": "<64-lowercase-hex-characters>"
  },
  "normalization": "unit_l2"
}
```

Load must fail clearly if:

- dimension mismatches the query vector.
- format version is unsupported.
- required files are missing.
- file sizes do not match manifest counts.
- compressed payloads fail to decompress.
- decompressed file sizes do not match manifest counts when recorded.
- checksum validation fails. Format V3 and V4 require SHA-256 checksums for
  every persisted payload; V1/V2 remain readable without them. V4 adds the
  canonical record payload and stable external/internal chunk mapping.
- a tombstone byte is not exactly `0` or `1`.

`chunks.bin` and `bm25.bin` may be compressed at rest. Loading must
transparently decompress them before rebuilding in-memory search structures so
compression does not affect the hot query path.

### Vector Store

`vectors.vec` stores contiguous vectors using the index's configured vector encoding:

```text
vector_offset = chunk_id * dimension * bytes_per_value
```

Requirements:

- Store values in little-endian format when the encoding uses multi-byte values.
- Support mmap loading.
- Support direct vector lookup by internal chunk ID.
- Avoid per-vector heap allocation during search.
- Keep query vectors accepted as `f32` at the public API boundary.
- Convert query vectors to the storage/search representation inside Rust when needed.

### Vector Encoding

V1 must separate the public embedding type from the stored vector encoding.

Public API:

```text
caller embeddings: f32
query embeddings: f32
```

Storage API:

```rust
pub enum VectorEncoding {
    F32,
    F16,
    BF16,
    I8ScalarQuantized,
    BinaryQuantized,
}
```

Recommended support order:

1. `F32`
2. `F16`
3. `BF16`
4. `I8ScalarQuantized`
5. `BinaryQuantized`

Do not implement product quantization or int4 in V1 unless benchmarks prove they are required.

`BinaryQuantized` means one bit per dimension. For example, a 768-dimensional embedding can be stored as 768 bits, or 96 bytes, before any metadata or alignment overhead. This is a size-constrained candidate retrieval format and must be benchmarked against `F32` exact search before becoming a default.

#### F32 Encoding

Use `F32` as the correctness and benchmark reference. Use
`I8ScalarQuantized` as the production SDK default after the V1 MiniLM fixture
measured 98.33% top-5 and 100% top-10 vector-only overlap against F32.

Properties:

- 4 bytes per dimension.
- Best correctness.
- Simplest exact search.
- Best ground-truth format.

Use for:

- correctness tests
- benchmark ground truth
- small indexes
- high-quality reranking

#### F16 Encoding

Use `F16` when disk and memory footprint matter but quality must stay close to `F32`.

Properties:

- 2 bytes per dimension.
- About 50% vector storage reduction.
- Query can remain `f32`.
- During scoring, decode blocks to `f32` or use SIMD-supported half conversion.

Rules:

- Normalize source vectors before conversion to `f16`, then store the encoded
  normalized values.
- Benchmark recall against `F32` exact search.
- Exact search over `f16` is exact relative to stored `f16`, not exact relative to original `f32`.

Acceptance gate:

```text
recall@10 >= 0.99 against F32 exact search
p95 latency no worse than F32 by more than 25%
vector storage reduced by about 50%
```

#### I8 Scalar Quantized Encoding

Use `I8ScalarQuantized` only when the app needs much smaller indexes.

Properties:

- 1 byte per dimension plus one `f32` scale per vector.
- About 75% vector storage reduction compared with `F32`.
- Higher recall risk than `F32`/`F16`.
- Best used for candidate retrieval, followed by reranking with better vectors if available.

Quantization metadata:

```rust
pub struct ScalarQuantizationParams {
    pub scale: f32,
}
```

V1 quantization options:

```rust
pub enum QuantizationScope {
    PerIndex,
    PerVector,
}
```

Default:

```text
I8 scope: per-vector
I8 mode: symmetric
```

Rules:

- Normalize source vectors before quantization when metric is cosine.
- Use symmetric quantization with `zero_point = 0` for the first implementation.
- Store quantization parameters in a compact sidecar section.
- Benchmark recall against `F32` exact search.
- Prefer `I8ScalarQuantized` for first-stage retrieval, not final quality ranking.
- If high-quality reranking is required, optionally store a second `F16` or `F32` rerank vector store.

Acceptance gate:

```text
recall@10 >= 0.95 against F32 exact search for vector-only retrieval
p95 latency is better than or equal to F32
vector storage reduced by about 75%
```

#### Optional Rerank Vector Store

For compressed indexes, support this later:

```text
vectors.vec          compressed retrieval vectors
vectors_rerank.vec   f16 or f32 vectors for exact reranking
```

This gives a useful tradeoff:

```text
small retrieval vectors
  -> larger candidate set
  -> rerank top candidates with f16/f32
```

Do not require the rerank vector store in V1. Keep it in the later exploration
column until real-data benchmarks show `I8ScalarQuantized` needs higher final
quality than its first-pass results provide.

### Chunk Store

`chunks.bin` stores compact result-display data:

- chunk ID
- document ID reference
- text offset and length
- metadata offset
- active/deleted flag
- version

Search should not decode full text unless requested. The default result may return text snippets only if they are stored in a compact directly-readable format.

### Keyword Store

BM25 needs:

- token dictionary
- postings lists
- document lengths
- average document length
- per-token document frequency

V1 tokenizer:

- lowercase
- unicode-aware word split if feasible
- ASCII fallback acceptable for first prototype
- configurable stopword list

## Search Modes

The V1 SDK supports exact vector search and hybrid search.

```rust
pub enum SearchMode {
    Exact,
    Hybrid,
}
```

### Exact Vector Search

Exact search scans all active vectors and computes exact similarity.

Purpose:

- primary V1 retrieval engine
- tests
- correctness baseline for future ANN work

Use exact search for all V1 vector retrieval.

Expected use:

```text
1K to <50K chunks
384d or 768d vectors
top_k 5-10
```

Optimization requirements:

- contiguous vector storage
- cache-friendly scans
- SIMD-accelerated dot product/cosine where available
- no per-vector heap allocation during search
- metadata filters represented as bitsets or compact ID lists
- top-k maintained with a small fixed-size heap or partial selection

### Future HNSW Search

HNSW is deferred until after V1.

Reconsider HNSW only when:

```text
real datasets exceed 50K chunks
or exact search misses the retrieval latency budget on device
```

Future HNSW rules:

- HNSW results are candidate results, not final truth.
- Final results must remove tombstoned chunks.
- Exact reranking over candidate vectors should be available.
- Recall must be benchmarked against exact search.
- Metadata filters require over-fetching or filter-aware indexes.
- Do not strict-prefilter HNSW traversal in future ANN work. It can disconnect useful graph paths and reduce recall.

### BM25 Search

BM25 handles exact names, identifiers, rare terms, and lexical matching.

Default parameters:

```text
k1: 1.2
b: 0.75
```

BM25 must return:

- chunk ID
- BM25 score
- matched terms

### Hybrid Search

Hybrid search combines vector and BM25 results.

V1 public fusion method:

```text
weighted normalized score fusion with query-time alpha
```

Default:

```text
vector_candidates: 50
keyword_candidates: 50
alpha: 0.6
```

`alpha = 1` is vector-only and `alpha = 0` is BM25-only. At those endpoints,
Rust must not generate candidates from the zero-weight source. Intermediate
values generate both candidate sets. RRF remains an internal Rust benchmark
surface, not a language-wrapper or public C ABI option.

V1 hybrid query flow:

```text
query embedding
  -> metadata-prefiltered exact vector candidate retrieval
query text
  -> metadata-prefiltered BM25 candidate retrieval
vector candidates + BM25 candidates
  -> alpha-weighted normalized fusion
  -> final top_k
  -> result trace
```

Metadata filters must be applied before final top-k is returned. When a filter
can be represented by the metadata filter index, candidate generation should use
that narrowed set before vector scoring or BM25 scoring. Final results must
still verify filter correctness.

Hybrid candidate limits are part of the public API and are per-query tunables.
`top_k` is the final fused hit count; `vector_top_k` and `keyword_top_k` control
how many candidates each retrieval mode contributes before fusion.

## Query Planner

The V1 query planner chooses the exact scan scope per query.

Planner inputs:

- vector count
- vector dimension
- vector encoding
- `top_k`
- filter selectivity
- latency preset
- recall preset

Planner output:

```rust
pub enum QueryPlan {
    ExactAll,
    ExactFiltered,
    HybridExactAll,
    HybridExactFiltered,
}
```

### No-Filter Query Policy

V1 policy:

```text
exact scan all active vectors
```

Track operation count for benchmarking:

```text
exact_ops = active_chunk_count * dimension
```

For V1, the product target is that this remains within budget for fewer than 50K chunks.

### Filtered Query Policy

Filters must first produce a cheap matching-ID bitset or ID list.

V1 filtered policy:

```text
if filter exists:
    build/evaluate matching ID bitset
    exact scan only matching IDs
```

Track filtered operation count:

```text
filtered_ops = matching_count * dimension
```

Filtered exact search is fully correct and should usually be faster than scanning all active vectors when the filter is selective.

### Future ANN Policy

Do not implement ANN routing in V1.

When datasets exceed 50K chunks, reintroduce this policy:

```text
if no filter and exact search misses budget:
    HNSW global search -> exact rerank candidates

if filter exists and filtered exact search misses budget:
    if filter maps to a major partition with its own index:
        search partition HNSW index
    else:
        global HNSW overfetch -> postfilter -> exact rerank survivors
```

Major partition indexes are optional and should only be built for high-cardinality filters that are common and stable.

Good partition candidates:

- tenant ID
- workspace ID
- account ID
- language
- source type

Bad partition candidates:

- timestamps with many unique values
- arbitrary tags
- low-frequency metadata
- filters that change often

### Future Overfetch Policy

When using global HNSW with postfiltering in a future version, the planner must overfetch enough candidates to survive filtering.

Default:

```text
candidate_count = max(top_k * 10, 100)
```

If too few filtered results remain:

```text
increase candidate_count until:
  final_count >= top_k
  or candidate_count reaches max_candidate_count
  or latency budget is exhausted
```

Default cap:

```text
max_candidate_count = 1000
```

If the cap is reached and results are still insufficient, return fewer results with a trace field explaining that the filter was too selective for the selected plan.

### Planner Correctness Rule

Exact search is the V1 source of truth. Future ANN modes are acceleration paths, not correctness baselines.

## Public Swift API

V1 uses capability-specific database types with progressive disclosure:

```swift
RetrievalDatabase       // searchable documents
GraphDatabase           // graph records and traversal
GraphRetrievalDatabase  // graph records plus searchable documents
```

Embeddings are caller-produced `[Float]` values. The common initializers do not
accept an embedding dimension; the first embedding fixes the database dimension
and later document and query embeddings must match it. Apps may bring their own
model or use EmbeddingKit.

Retrieval-only ingestion:

```swift
let builder = try RetrievalDatabase.Builder(corpusID: "notes")
try await builder.upsert(
    Document(
        id: "note-42",
        text: "RetrievalKit provides private, on-device search.",
        metadata: ["project": .string("apollo")]
    ),
    embedding: documentEmbedding
)
let database = try await builder.build()
```

Graph-only ingestion never accepts retrieval configuration or embeddings:

```swift
let builder = try GraphDatabase.Builder(schema: graphSchema)
try await builder.upsert(record)
let database = try await builder.build()
let result = try await database.query(
    from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
    traversing: [GraphTraversal(relationship: "contains")]
)
```

The common combined path uses `Record.content` as one searchable document and
derives its stable `DocumentID` from the `RecordID`:

```swift
let builder = try GraphRetrievalDatabase.Builder(graph: graphSchema)
try await builder.upsert(record, embedding: documentEmbedding)
```

Records with multiple independently identifiable searchable documents use the
advanced overload:

```swift
try await builder.upsert(
    record,
    documents: [
        EmbeddedDocument(
            id: "note-42:summary",
            text: summary,
            embedding: summaryEmbedding
        ),
        EmbeddedDocument(
            id: "note-42:body",
            text: body,
            embedding: bodyEmbedding
        )
    ]
)
```

`ChunkKey`, keyed embedding dictionaries, and explicit record/document linking
are internal concepts, not common-path API requirements.

All retrieval-capable databases use one overloaded search family:

```swift
let semantic = try await database.search(
    embedding: queryEmbedding,
    limit: 10
)

let lexical = try await database.search(
    text: "private search",
    limit: 10
)

let hybrid = try await database.search(
    text: "private search",
    embedding: queryEmbedding,
    alpha: 0.6,
    limit: 10,
    filter: .equals("project", .string("apollo"))
)
```

For combined databases, a graph selection can constrain any search:

```swift
let hits = try await database.search(
    text: "private search",
    embedding: queryEmbedding,
    alpha: 0.6,
    within: selection,
    limit: 10
)
```

`alpha` is the vector contribution to weighted normalized-score fusion:

```text
alpha = 1     vector only
alpha = 0     BM25 only
0 < alpha < 1 hybrid
```

Combined results expose both identities:

```swift
struct GraphHybridHit {
    let documentID: String
    let recordID: String
    let text: String
    let metadata: [String: MetadataValue]
    let score: Float
    let vectorScore: Float?
    let keywordScore: Float?
    let trace: GraphHybridTrace
}

struct GraphHybridTrace {
    let alpha: Float
    let vectorRank: Int?
    let keywordRank: Int?
    let normalizedVectorScore: Float?
    let normalizedKeywordScore: Float?
    let matchedTerms: [String]
}
```

Exact, BM25, and hybrid results all return the effective stored metadata after
chunk metadata overrides document/projected metadata. A returned hit already
passed its filter, so public traces do not repeat a constant `filterMatched`
field. Exact traces expose the vector score. Hybrid traces expose the effective
query `alpha`, ranks, normalized scores, and matched terms. The public contract
does not expose the internal fusion enum.

Supported storage encodings remain advanced builder options:

```swift
enum VectorEncoding {
    case f32
    case f16
    case bf16
    case i8ScalarQuantized
}
```

Explicit-dimension builders, keyed embedding maps, and public chunk construction
are not part of the progressive database API. The lower-level mutable
`VectorIndex` remains available as a separate compatibility surface.

## Rust Core API

Rust should expose a clean internal API first:

```rust
pub trait VectorIndex {
    fn add_batch(&mut self, vectors: &[f32], ids: &[u64]) -> Result<()>;
    fn search(&self, query: &[f32], top_k: usize, params: SearchParams) -> Result<Vec<VectorHit>>;
    fn save(&self, path: &Path) -> Result<()>;
    fn load(path: &Path) -> Result<Self> where Self: Sized;
}
```

The V1 implementation is exact search. Future ANN implementations should conform to the same trait.

Search engine:

```rust
pub struct SearchEngine {
    vector_index: Box<dyn VectorIndex>,
    bm25_index: Bm25Index,
    stores: LocalStores,
}
```

Rust owns:

- progressive builder state and dimension inference
- canonical record/document/chunk identity derivation
- embedding coverage and dimension validation
- index loading
- vector search
- BM25 search
- filtering
- alpha validation and fusion
- result assembly

Every language wrapper owns only:

- idiomatic public API shape and naming
- conversion between native language values and the C ABI
- native-handle lifetime, async/threading integration, and cancellation bridging
- typed presentation of Rust status errors

Wrappers must not queue semantic work, infer retrieval configuration, derive
hidden identities, validate ranking rules, or reimplement retrieval and graph
behavior. This boundary applies to Swift first and to every later wrapper with
language-appropriate syntax.

## FFI Contract

The search hot path must use simple C-compatible types.

Do:

- pass query vector pointer and length.
- pass top-k and mode.
- pass public hybrid `alpha` directly; Rust validates and expands it into
  concrete fusion weights.
- return fixed-width result records whose strings reference one owned packed
  UTF-8 arena.
- return BM25 and hybrid matched terms through one flat range table referencing
  that same arena.
- return effective metadata through one flat metadata-entry table whose keys
  and string values reference that same arena.
- store hybrid `alpha` once on the result buffer rather than once per hit.

Do not:

- allocate or free a separate native string for every result field or matched
  term.
- pass JSON across FFI in the hot path.
- allocate large result objects per query.

The packed result buffer remains valid until its matching free function is
called. Wrappers decode native strings and metadata before freeing it and never
retain borrowed arena pointers. Exact results use one allocation each for hits,
UTF-8 bytes, and metadata entries. BM25 and hybrid add one matched-term range
allocation. Empty buffers use null pointers and zero counts.

Result hydration is fallible. If a ranked chunk or stable graph identity cannot
be resolved, Rust fails the query with an actionable core error; wrappers must
not synthesize empty text, metadata, or identities.

The common Swift document-upsert path also uses typed C strings, metadata
entries, and a contiguous float buffer. Schema-rich graph ingestion may use
JSON on the cold build path, but query embeddings and query controls never do.

Minimal C ABI shape:

```c
typedef struct {
  size_t offset;
  size_t length;
} RetrievalKitUtf8Range;

typedef struct {
  RetrievalKitUtf8Range key;
  uint32_t value_type;
  RetrievalKitUtf8Range string_value;
  int64_t integer_value;
  double float_value;
  bool bool_value;
} RetrievalKitPackedMetadataEntry;

typedef struct {
  const RetrievalKitSearchHit *hits;
  size_t count;
  const uint8_t *utf8;
  size_t utf8_len;
  const RetrievalKitPackedMetadataEntry *metadata;
  size_t metadata_count;
} RetrievalKitSearchResultBuffer;
```

## Correctness Requirements

### Vector Correctness

Tests must verify:

- dimension mismatch fails.
- empty index returns empty results.
- exact search returns known nearest neighbors.
- cosine normalization is applied or validated.
- dot product and cosine are not silently mixed.
- `F32`, `F16`, and `I8ScalarQuantized` indexes reject incompatible reopen configs.
- `F16` recall is measured against `F32` exact search.
- `I8ScalarQuantized` recall is measured against `F32` exact search.
- compressed encodings do not change filter, tombstone, or document-version behavior.

### Metadata Correctness

Tests must verify:

- equality filters.
- range filters.
- `in` filters.
- missing field behavior.
- deleted chunks are excluded.
- updated documents do not return old versions.

### Hybrid Correctness

Tests must verify:

- keyword-only matches can appear.
- vector-only matches can appear.
- exact name queries are not lost to semantic-only retrieval.
- weighted normalized and RRF ranking are deterministic.
- score traces match the ranking calculation.

### Persistence Correctness

Tests must verify:

- index can be closed and reopened.
- reopened search results match pre-close results for exact search.
- manifest mismatches fail.
- interrupted build does not corrupt the last valid index.
- tombstones persist across reloads.
- same-size payload corruption, truncation, and appended bytes are rejected.
- read-only validation reports corruption without changing the index directory.

Persistence uses immutable generation directories under `.snapshots`. Writers
fully write, sync, and validate generation file sizes before atomically
publishing the generation through `manifest.json`. Failures before publication
leave the previous manifest and generation untouched. The loader accepts legacy
V1 root-file indexes; the next successful save migrates them to the generation
layout and cleans abandoned or superseded files. An OS-released exclusive file
lock serializes writers to the same directory, including across processes, so a
crash cannot leave a stale logical lock and concurrent cleanup cannot remove the
published generation.

Format V4 manifests include SHA-256 checksums for vectors, chunks, canonical
records, BM25 when present, and tombstones. Rust `validate_dir`, Swift `VectorIndex.validate(at:)`,
and Python `Index.validate(path)` run the same complete validation path used by
load. Checksum failures identify the damaged file and instruct callers to
restore or rebuild the index. V1, V2, and V3 indexes remain readable; their next
save publishes a checksummed V4 snapshot.

## Speed Requirements

Measure these separately:

```text
embedding latency
vector retrieval latency
BM25 retrieval latency
metadata/filter latency
fusion/rerank latency
end-to-end search latency
```

Primary iOS target:

```text
retrieval-only latency: 5-10 ms on modern iPhone hardware
```

This excludes:

- query text embedding
- network calls
- index opening/loading
- UI rendering
- full document hydration

Initial device targets:

```text
10K chunks, 384d, top_k 10:
  exact vector search: <= 5 ms target
  hybrid search: <= 10 ms target

25K chunks, 384d, top_k 10:
  exact vector search: <= 8 ms target
  hybrid search: <= 12 ms target

50K chunks, 384d, top_k 10:
  exact vector search: <= 10 ms target
  hybrid search: <= 15 ms target
```

These are retrieval-only targets. They exclude embedding generation.

Recommended 5-10 ms retrieval budget:

```text
vector candidate retrieval: 1-5 ms
BM25 candidate retrieval: 1-3 ms
metadata/filter evaluation: <1-2 ms
fusion/top-k selection: <1-2 ms
Swift/Rust boundary and result formatting: <1 ms
```

Expected fast path:

```text
query embedding already available
  -> normalize f32 query vector when metric is cosine
  -> planner chooses exact-all or exact-filtered
  -> exact vector scoring
  -> BM25 candidate retrieval when hybrid
  -> apply tombstones and metadata filters
  -> fuse scores when hybrid
  -> compact metadata lookup
  -> return top 5-10 results
```

Hot path rules:

- no SQLite query
- no JSON decoding
- no network call
- no index construction
- no broad heap allocation
- no loading index files
- no full document decoding unless requested

## Benchmark Requirements

Create a benchmark CLI before adding indexing complexity.

Initial synthetic benchmark command:

```bash
retrievalkit bench synthetic --chunks 10000 --dimension 768 --queries 100 --encoding i8
```

Initial synthetic matrix command:

```bash
retrievalkit bench matrix --chunks 10000 --dimensions 384,768,1536 --top-k 5,10 --encodings f32,f16,bf16,i8
```

Synthetic and matrix reports must include recall@k against `F32` exact search
for every non-`F32` encoding. `F32` reports recall as `1.0` because it is the
ground-truth baseline.

Encoding fidelity and human relevance are separate benchmark tracks. F32 exact
overlap validates compact or approximate retrieval; it does not prove that the
ranking satisfies a user’s information need.

Later file-backed benchmark command:

```bash
retrievalkit bench --index ./data --queries ./queries.jsonl --out ./bench.json
```

Benchmark datasets:

```text
1K chunks
10K chunks
25K chunks
50K chunks
```

Dimensions:

```text
384
768
1536
```

Vector encodings:

```text
F32
F16
BF16
I8ScalarQuantized
BinaryQuantized
```

Metrics:

- p50 latency
- p95 latency
- p99 latency
- max latency
- recall@5
- recall@10
- MRR@10
- recall@10 vs F32 exact search
- memory usage
- index load time
- index size on disk
- build throughput
- vector bytes per chunk
- compression ratio vs F32
- selected exact query plan
- filtered query latency by selectivity bucket
- filtered recall@10 vs F32 exact filtered search

Human-judged retrieval reports must additionally include:

- NDCG@5 and NDCG@10
- human relevance Recall@5 and Recall@10
- Success@1 / Hit Rate
- Precision@5
- MRR@10
- MAP where binary judgments are available
- per-category and worst-decile results
- zero deleted, superseded, stale, filter, and persistence violations

Human relevance fixtures must use versioned qrels independent of search output.
For locked release evaluation, pool results from vector-only F32/I8, BM25,
multiple hybrid alpha values, internal RRF baselines, and every candidate configuration under
consideration. Blind judgments to originating system and rank.

Future gold-standard compatibility:

- Emit standard TREC qrels and run files.
- Cross-check metric calculations with `trec_eval` or `ir_measures`.
- Evaluate on BEIR SciFact and NFCorpus for external comparability.
- Run an appropriate official NIST TREC collection once release distribution
  is stable, and evaluate participating in the NIST TREC RAG track.
- Keep a separate product-specific locked set built from anonymized real-user
  queries; external benchmarks do not replace product evaluation.

Filter selectivity benchmark buckets:

```text
0.1% of chunks match
1% of chunks match
5% of chunks match
10% of chunks match
50% of chunks match
100% of chunks match
```

Exact-search acceptance gate:

```text
p95 exact vector retrieval latency stays within the configured latency preset
exact search returns deterministic results across repeated runs
```

Future HNSW work must be benchmarked against these exact-search results.

Filtered-query acceptance gate:

```text
filtered recall@10 >= 0.95 against F32 exact filtered search
p95 filtered retrieval latency stays within the configured latency preset
```

## Indexing Pipeline

Batch indexing flow:

```text
input documents
  -> chunking by caller or shared Rust SDK helper
  -> embedding by caller or embedding provider
  -> validation
  -> vector normalization
  -> write vectors
  -> write chunk records
  -> update BM25
  -> update exact index metadata
  -> atomically publish manifest
```

V1 should prefer batch builds over fully dynamic optimization.

The generic SDK helper provides deterministic fixed and sentence-aware
chunking from `retrievalkit-ingest`. Limits and overlap are Unicode-character
based, and chunks retain UTF-8 byte offsets into the original document. Exact
model-token budgets remain an embedding integration concern because tokenizer
behavior varies by model. Swift exposes `TextChunker` through the
`RetrievalKit` product and Python through `retrievalkit.ingest`; both call the
same Rust implementation rather than reimplementing it.

An optional wrapper-level pipeline composes chunking, embedding, and indexing
without moving model execution into Rust. Swift exposes this as the separate
`RetrievalKitPipeline` package; Python exposes `retrievalkit.pipeline`. The pipeline
must finish and validate every chunk embedding before document upsert so an
embedding failure cannot partially replace an existing document. Empty text is
an ingestion error, not an implicit delete operation.

The pipeline accepts an application-defined chunker through the same public
boundary used by the built-in Rust chunker. Custom chunkers must return text and
UTF-8 source byte ranges; embedding validation and index mutation remain owned
by the pipeline.

The custom chunker protocol belongs to the pipeline layer because it defines an
orchestration policy, not a Rust ingestion primitive. Pipeline chooses an
opinionated built-in default and validates custom chunk ordering, non-empty
text, UTF-8 boundaries, ranges, and source-text correspondence before embedding.

When an embedding provider exposes its exact tokenizer and model input limit,
Pipeline recursively subdivides Rust-produced chunks until every chunk fits the
token budget. Swift discovers this through `TextEmbedder`; Python accepts an
explicit token-count callback and limit. Providers without tokenizer access use
the documented character-based fallback. A document is represented by multiple
chunk vectors; the SDK does not truncate the document into one lossy vector.

Incremental add:

- append vector
- append chunk record
- update BM25 postings
- update manifest

Update:

- mark old chunks deleted
- add new chunks
- update document version

Delete:

- mark chunks tombstoned
- exclude from all final results
- rebuild later to reclaim space

Compaction:

- create a new index directory
- copy only active chunks
- rebuild vector/BM25 files
- atomically swap active index path

## Build Milestones

### Milestone 1: Exact Local Search

Deliverables:

- Rust crate.
- Add/search APIs.
- Contiguous vector store.
- Exact cosine/dot search.
- Basic metadata store.
- Unit tests.

Success criteria:

- Exact search returns known nearest neighbors.
- Index persists and reloads.
- Deleted chunks are excluded.

### Milestone 2: BM25 and Hybrid Search

Deliverables:

- Tokenizer.
- BM25 index.
- Hybrid weighted normalized fusion.
- Query-time hybrid alpha.
- Trace output.
- Filter support.

Success criteria:

- Exact-name queries work.
- Semantic paraphrase queries work.
- Hybrid traces explain ranking.

### Milestone 3: Benchmark Harness

Deliverables:

- Benchmark CLI.
- Exact ground truth generation.
- Latency and recall report.
- Dataset generator.

Success criteria:

- You can answer whether exact search meets the target for 1K, 10K, 25K, and 50K chunks.

### Deferred Optimization Decisions

Decide these after the benchmark harness can measure latency, memory, disk size,
and recall:

- Optional rerank vector store for compressed encodings: evaluate only after
  real-data benchmarks show `I8ScalarQuantized` needs better final quality.
  Measure disk size, memory, recall, and latency for `I8`, `I8 + F16 rerank`,
  and `I8 + F32 rerank` before implementation.
- Hybrid candidate defaults: keep the public per-query override. Before
  changing the default candidate limits, compare smaller limits such as
  `10/25` or `25/25` against a high-candidate same-encoding reference such as
  `50/50` or `100/100`.
- Faster BM25 maps/sets: the runtime BM25 index now uses hash-backed term
  lookups/postings while preserving deterministic persisted format and final
  ordering. Revisit custom hashers only if realistic benchmarks show the local
  BM25 implementation is again a bottleneck.
- External BM25 engine: evaluate the `bm25` crate or Tantivy BM25/tokenizer stack only if benchmarks show the local BM25 implementation is a bottleneck or search quality needs language-aware stemming/normalization.

Do not replace simple deterministic structures just because a faster crate exists. Use benchmark results to justify the dependency and keep final ranking deterministic.

### Milestone 4: Swift Wrapper

Deliverables:

- C ABI.
- Swift package.
- Swift-friendly types.
- iOS sample app.
- On-device benchmark screen.

Success criteria:

- A Swift app can open an index, add chunks, run exact/hybrid search, and display traceable results.

### Milestone 5: Small-Index Production Hardening

Deliverables:

- Atomic index writes.
- Corruption checks.
- Compaction.
- Memory budget tests.
- Thread-safety review.
- SIMD/vector math optimization.
- Filter bitset optimization.

Compaction is an explicit Rust-core operation surfaced through the wrappers. It
builds replacement vector, chunk, BM25, metadata-filter, active-offset, and
chunk-lookup structures before swapping them into the index. Active chunk IDs,
document versions, and the monotonic next ID are preserved; removed IDs stop
resolving and are never reused. Compaction changes loaded memory only. Callers
save afterward when they want a smaller transactional disk snapshot.
The all-or-nothing rebuild temporarily retains current and replacement
structures, so wrappers document compaction as a maintenance operation that
requires memory headroom and blocks operations on the same index instance.

Success criteria:

- App restart never corrupts an existing valid index.
- Search can run concurrently with reads.

The V1 concurrency contract permits concurrent exact, keyword, hybrid, filter,
and count reads after indexing or loading. Upsert, delete, save, compaction, and
destruction require exclusive access. The Rust search path remains lock-free;
FFI callers provide synchronization. Swift uses a writer-preferring
asynchronous gate and detached native tasks so searches on one `VectorIndex`
can overlap while mutations wait for active readers and block later readers.
Python releases the GIL during Rust retrieval and permits shared PyO3 borrows
for concurrent searches. Its mutation and persistence methods require exclusive
borrows, so conflicting calls fail safely with `RuntimeError` instead of racing.
- Index updates are predictable and recoverable.
- 50K chunks meet the configured on-device retrieval target.

### Milestone 6: Local Website Demo

Deliverables:

- Curated first-party documents compiled by Python or the RetrievalKit CLI into
  a versioned, checksummed corpus pack with stable chunks, source offsets,
  precomputed document embeddings, metadata, and graph records.
- Local browser query embedding and an in-memory RetrievalKit WASM database
  built from the validated corpus pack.
- Free-form questions and suggested questions through one live pipeline.
- Grounded browser-SLM answers with validated source highlighting.
- Capability detection, asset progress, privacy-safe telemetry, and honest
  unsupported states.

Success criteria:

- Arbitrary questions run through local embedding, Rust WASM retrieval, and
  local answer generation without a hosted query-time request.
- Exact highlighted spans are verified against retrieved source text.
- First-load bytes, peak memory, indexing, embedding, retrieval, and generation
  latency are reported separately.

### Milestone 7: Future ANN Research

Deliverables:

- HNSW or another ANN implementation behind `VectorIndex`.
- Exact reranking of ANN candidates.
- ANN persistence.
- Recall benchmark against exact search.

Success criteria:

- ANN only ships if it improves real datasets above 50K chunks without violating recall targets.

## Implementation Defaults

Start with:

```text
language: Rust core with native Swift, Python, TypeScript/Node, and Kotlin/JVM wrappers
browser: capability-separated TypeScript API over Rust WASM in a dedicated Worker
metric: cosine
normalization: unit L2
dimension: inferred from the first embedding, then fixed per index
vector encoding: I8ScalarQuantized by default, F32/F16 opt-in
exact search: enabled
BM25: enabled
hybrid fusion: weighted normalized score with alpha 0.6
internal RRF: benchmark-only
HNSW: deferred until after small-index MVP
metadata filters: simple typed filters
storage: local files with mmap-friendly layouts
```

Default search config:

```text
top_k: 10
mode: hybrid
vector_candidates: 50
keyword_candidates: 50
rerank: exact_vector
trace: false
alpha: 0.6
```

Debug search config:

```text
top_k: 10
mode: hybrid
vector_candidates: 100
keyword_candidates: 100
rerank: exact_vector
trace: true
```

## Key Product Decisions

### Small Exact Search First

Exact search is the V1 product. It defines correctness, keeps filters simple, and should be optimized aggressively for fewer than 50K chunks.

### Retrieval Quality Over Raw ANN Speed

Most RAG failures come from bad retrieval, not slow nearest-neighbor search. BM25, filters, reranking, and traces are V1 features, not extras.

### Local Hot Path Must Be Boring

The search path should be mostly pointer math, vector math, compact lookups, and ranking. Anything involving JSON, SQLite, networking, or loading files belongs outside the hot path.

### The Browser Demo Must Run The Product

The website may advertise representative answers before a visitor starts the
demo, but an interactive answer is evidence only when the question runs through
local embedding, the Rust WASM retrieval core, and the local grounded answer
model. A JavaScript ranking clone, canned response, or hosted fallback does not
demonstrate RetrievalKit.

### Swift API Must Stay Small

Swift developers should not need to understand indexing internals. V1 exposes exact search directly:

```swift
.exact
```

Future ANN presets can be added after exact/hybrid search is proven on real datasets.

## Open Questions

Resolve these before implementing the Swift wrapper:

- Should embeddings be caller-provided only in V1?
- What is the first real dataset: Rumi transcripts, notes, PDFs, or synthetic?
- What dimension should the first benchmark use?
- Is full text returned from Rust, or does Swift fetch text by chunk ID?
- What is the minimum iOS version target?
- Is macOS support required before iOS?

Resolved for the first website demo:

- The first curated document is NASA's text-only “Apollo 11 Mission Overview.”
- The website-owned V1 corpus pack uses checksummed JSON metadata and graph
  records plus a row-major little-endian FP32 embedding binary. It is an
  application interchange format, not a public cross-platform database
  snapshot contract.
- The initial browser SLM is `Qwen3-0.6B-q4f16_1-MLC` through WebLLM/WebGPU.

Resolved website demo release gates:

- Vector answer generation and graph-scoped vector retrieval use the same live
  local pipeline as free-form questions; suggested questions do not select
  canned results.
- Combined answers are source-locked: Qwen selects validated graph facts from
  graph-scoped, vector-ranked passages, and the application renders their exact
  source sentences.
- Maximum client, corpus, and pinned-model bytes are 64 MiB, 1 MiB, and
  460 MiB respectively. The selected Qwen build reports approximately 1.4 GB
  of required GPU memory.
- The production URL and the Apple-silicon Chromium/WebGPU configuration are
  qualified. Cross-browser and physical mobile expansion remains separate
  follow-up work and must not be advertised before testing.

## Recommended First Build

Build this first:

```text
Rust CLI/crate:
  create index
  add JSONL chunks with precomputed embeddings
  exact vector search
  BM25 search
  hybrid search
  metadata filters
  trace output
  benchmark exact search
```

Only after the small-index product is fast and correct:

```text
Test ANN/HNSW behind the same VectorIndex trait for >50K chunks.
Benchmark it against exact search before shipping it.
```
