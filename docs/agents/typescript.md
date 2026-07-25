# TypeScript Agent Guidance

TypeScript is a public V1 wrapper target for Node.js LTS, initially distributed
as repository-local macOS arm64 packages. Read this file before creating or
modifying TypeScript or Node native-addon code.

## Architecture

- Bind directly to the Rust core through napi-rs. TypeScript owns API shape,
  marshaling, scheduling, error presentation, and native-object lifetime only.
- Keep base and graph-capable packages and native aggregates separate. A base
  package must neither load nor depend on graph code, and an application must
  not load both native aggregates in one process.
- Keep registry names provisional until naming clearance. Do not claim npm
  publication or availability.
- Do not support browsers or WebAssembly until separately authorized.

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

## Boundary And Performance

- Search, filtering, graph query, candidate projection, and result paths must
  use typed N-API conversion, never JSON.
- Keep contiguous embedding buffers and use bulk result conversion. Do not
  implement ranking, filtering, graph traversal, generation checks, identity
  derivation, persistence, or fallback behavior in TypeScript.
- Native operations that can block must run through napi-rs async tasks. State
  lifetime must remain valid until an in-flight task finishes.

## Errors And Lifecycle

- Map stable Rust error categories to exported TypeScript error classes while
  retaining actionable Rust messages.
- Document thread safety, close behavior, persistence compatibility, and the
  prohibition on loading both native aggregates.
- Never expose raw pointers or numeric native handles.

## Packaging And Testing

- Put the wrapper under `wrappers/typescript/` with separate base and graph
  packages and shared sources when practical.
- Declare Apache-2.0 and include `LICENSE` and `NOTICE` in every distributable.
- Test lifecycle, Unicode, metadata, alpha endpoints, persistence, graph
  selection, candidate projection, conformance fixtures, and package contents.
- Run native builds, typechecking, lint, tests, clean local-install smoke tests,
  and base-package graph-exclusion inspection before completion.
