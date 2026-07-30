# TypeScript Agent Guidance

TypeScript has two separate targets: the public V1 Node.js LTS wrapper,
initially distributed as repository-local macOS arm64 packages, and the
additive browser/WebAssembly package. Read this file before creating or
modifying TypeScript, Node native-addon, or browser Worker code.

## Architecture

- Bind directly to the Rust core through napi-rs. TypeScript owns API shape,
  marshaling, scheduling, error presentation, and native-object lifetime only.
- Keep base and graph-capable packages and native aggregates separate. A base
  package must neither load nor depend on graph code, and an application must
  not load both native aggregates in one process.
- Keep registry names provisional until naming clearance. Do not claim npm
  publication or availability.
- Keep the Node and browser packages separate. The browser package binds the
  dedicated `wasm-bindgen` aggregate and must not import, emulate, or bundle the
  N-API addon. Browser/WASM was separately authorized on 2026-07-26.
- Keep the optional Node embedding package under
  `wrappers/typescript/embedding` as an independently distributable N-API
  package over the separate `retrievalkit-embedding` Rust crate. It must not
  depend on the base or graph retrieval packages, and it is not the browser
  embedding implementation.
- Keep browser embedding in the independent
  `wrappers/browser-embedding` package. It uses the browser-native tokenizer
  and ONNX Runtime Web directly inside its own Worker; it must not import a
  retrieval package, the Node N-API addon, or the Rust retrieval core.
- Browser databases are Worker-owned. Public browser APIs are asynchronous and
  must not run retrieval, graph traversal, or bulk ingestion on the UI thread.
- Browser embedding model acquisition, tokenization, session creation,
  warmup, and inference are likewise Worker-owned. Retrieval database
  construction and operations never create or download an embedding model.

## Public API

- Expose `RetrievalDatabase`, `GraphDatabase`, and
  `GraphRetrievalDatabase` with idiomatic camel-case methods and typed
  interfaces or discriminated unions.
- Accept `Float32Array` for embeddings. Infer dimension from the first
  embedding and never expose chunk-key embedding maps in the common API.
- Use promises for native work that could block the JavaScript event loop.
- Support bulk ingestion. Keep native handles, C structs, internal chunk IDs,
  and candidate-scope internals private.
- Provide explicit `close()` and `[Symbol.dispose]()` where supported.
  Operations after close must fail deterministically with a typed error.
- Use optional properties instead of sentinels and preserve stable
  `(recordId, chunkKey)` identities only in candidate-projection results.
- The production Node embedding surface is FP32-only, promise-based, fixed to
  256 input tokens, and returns exactly 384 finite, normalized
  `Float32Array` values. Acquisition occurs only in `load` or `prefetch`, and
  `localOnly` must prohibit network access.
- Browser embedding follows the same FP32/256-token/384-value contract.
  `load`, `prefetch`, `embed`, `embedBatch`, model information, the selected
  execution provider, and explicit close are the complete public boundary.
  Keep model precision distinct from RetrievalKit's independent
  `I8ScalarQuantized` database encoding.

## Boundary And Performance

- Node search, filtering, graph query, candidate projection, and result paths
  must use typed N-API conversion, never JSON. Browser query paths use typed
  JavaScript/WASM conversion and contiguous typed arrays, never textual JSON.
- Keep contiguous embedding buffers and use bulk result conversion. Do not
  implement ranking, filtering, graph traversal, generation checks, identity
  derivation, persistence, or fallback behavior in TypeScript.
- Native operations that can block must run through napi-rs async tasks. State
  lifetime must remain valid until an in-flight task finishes.
- Browser operations use a request-ID Worker protocol. Transfer contiguous
  embedding buffers in bulk, avoid textual JSON on the query path, and keep
  result conversion proportional to top-k rather than corpus size.
- Browser embedding batches use one dynamic, batch-longest inference call and
  one contiguous `Float32Array` transfer. The default attempts WebGPU with
  same-model WASM operator fallback and then a separately initialized
  WASM-only session; an explicit provider choice remains strict.
- Browser close, cancellation, stale requests, Worker failure, and operations
  after close must fail deterministically with typed errors.

## Website Demo Boundary

- The website supplies curated documents, the independent browser embedding
  provider, and a browser SLM. Those are orchestration dependencies of the
  website, not dependencies of `retrievalkit-core` or the browser retrieval
  package.
- Every free-form or suggested question in the interactive demo must invoke
  local embedding, WASM retrieval, and grounded local answer generation.
  Static marketing examples may be pre-rendered only outside live-demo result
  state.
- The initial WASM database is in-memory. A clean browser session builds it
  locally from the bundled documents. Model and deterministic document inputs
  may be cached with version and integrity checks; database byte snapshots must
  not be claimed until `save_to_bytes`/`load_from_bytes` is implemented.
- Validate every exact evidence quote against a retrieved chunk before mapping
  it through retained source offsets and highlighting the original document.
  Invalid citations fall back to passage-level evidence.
- Do not send questions, retrieved passages, embeddings, or generated answers
  to analytics, a model API, or a retrieval service.

## Errors And Lifecycle

- Map stable Rust error categories to exported TypeScript error classes while
  retaining actionable Rust messages.
- Document thread safety, close behavior, persistence compatibility, and the
  prohibition on loading both native aggregates.
- Never expose raw pointers or numeric native handles.

## Packaging And Testing

- Put the wrapper under `wrappers/typescript/` with separate base and graph
  packages and shared sources when practical.
- Put the browser package under `wrappers/browser/`. Its registry name remains
  provisional and it must not be added to Node package publication scripts.
- Put optional browser embedding under `wrappers/browser-embedding/` with its
  own lockfile, legal notices, runtime-asset verification, tests, and package
  content audit. It is independently distributable and remains unpublished.
- Keep `@gungorbasa/retrievalkit-embedding` private/provisional until package
  naming and publication are separately authorized.
- Declare Apache-2.0 and include `LICENSE` and `NOTICE` in every distributable.
- Test lifecycle, Unicode, metadata, alpha endpoints, persistence, graph
  selection, candidate projection, conformance fixtures, and package contents.
- Run native builds, typechecking, lint, tests, clean local-install smoke tests,
  and base-package graph-exclusion inspection before completion.
