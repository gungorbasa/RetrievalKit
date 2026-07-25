# Kotlin Agent Guidance

Kotlin is a public V1 wrapper target for Kotlin/JVM with an initial Android
arm64-v8a native artifact. Read this file before creating or modifying Kotlin,
JNI, Gradle, or Android packaging code.

## Architecture

- Bind directly to the Rust core through a thin, typed JNI layer. Kotlin owns
  API shape, marshaling, error presentation, and deterministic native lifetime
  only.
- Keep base and graph-capable Gradle modules and native aggregates separate.
  The base artifact must exclude graph code, and an application must not load
  both native aggregates in one process.
- Maven coordinates are repository-local and provisional until naming
  clearance. Do not claim public Maven availability.
- Do not claim Kotlin Multiplatform support until separately authorized.

## Public API

- Expose idiomatic `RetrievalDatabase`, `GraphDatabase`, and
  `GraphRetrievalDatabase` classes, data classes, sealed interfaces, and
  nullable properties.
- Accept `FloatArray` embeddings, infer dimension from the first embedding, and
  never expose chunk-key embedding maps in the common API.
- Provide blocking methods with a clear thread-safety contract. RetrievalKit
  does not add a coroutine dependency; callers choose their own dispatcher for
  disk, build, and query work.
- Implement `AutoCloseable` for Kotlin `use`. Operations after close must fail
  deterministically with a typed exception.
- Support bulk ingestion. Keep JNI symbols, native handles, C structs, internal
  chunk IDs, and candidate-scope internals out of the public API.

## Boundary And Performance

- Search, filtering, graph query, candidate projection, and result paths must
  use typed JNI values, never JSON, reflection, or serialization.
- Pass embeddings through primitive arrays and batch result conversion where
  practical. Do not implement ranking, filtering, traversal, generation
  validation, identity derivation, persistence, or fallback behavior in Kotlin.
- Every native handle has one owner, deterministic close behavior, and safe
  failure for use-after-close. JNI code must validate array and string inputs
  before calling Rust.

## Errors And Threading

- Map stable Rust error categories to a typed Kotlin exception hierarchy while
  retaining actionable Rust messages.
- Document the implemented thread-safety contract rather than implying broad
  concurrency. Never expose `Long` native handles publicly.

## Packaging And Testing

- Put the wrapper under `wrappers/kotlin/` with separate base and graph Gradle
  modules plus shared API sources when practical.
- Package Android arm64-v8a libraries in their respective AAR/JAR resources.
  Declare Apache-2.0 and include `LICENSE` and `NOTICE`.
- Test lifecycle, errors, Unicode, metadata, alpha endpoints, persistence,
  graph selection, candidate projection, conformance fixtures, JNI loading,
  and artifact contents.
- Run Gradle/JVM tests, native builds, Android arm64-v8a packaging checks when
  the installed toolchain permits, and base-artifact graph-exclusion
  inspection before completion.
