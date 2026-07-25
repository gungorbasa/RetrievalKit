# Rust, Swift, Python, TypeScript, and Kotlin Parity Audit

Date: 2026-07-25

## Verdict

All five language surfaces now implement the same capability-separated
architecture:

- `RetrievalDatabase` owns a canonical corpus plus exact vector and BM25
  retrieval.
- `GraphDatabase` owns the same corpus model plus graph traversal, without
  vector or BM25 state.
- `GraphRetrievalDatabase` owns one corpus with both graph and retrieval
  capabilities.
- Graph scope narrows candidates; it never becomes a separate ranker.
- Stable identities, generation validation, filtering, ranking, traces,
  persistence, and candidate projection stay in Rust.

The Python graph boundary gaps recorded by the earlier audit are closed.
Queries, results, and stable candidate projection now use typed PyO3 values
instead of JSON, and projection calls the canonical corpus operation.
TypeScript/Node and Kotlin/JVM/Android begin with the same architecture rather
than reproducing retrieval or graph behavior in wrapper code.

This is an architecture, correctness-surface, and source-qualification
statement. No new wrapper performance measurements were run, and this report
does not claim equal overhead or latency across languages.

## Canonical Architecture Matrix

| Contract | Rust | Swift | Python | TypeScript/Node | Kotlin/JVM + Android |
|---|---|---|---|---|---|
| Canonical corpus ownership | `CorpusIndex` | Native aggregate handle | Native PyO3 object | Native N-API resource | Opaque JNI registry handle |
| Retrieval-only product | `RetrievalDatabase` | `RetrievalDatabase` | `RetrievalDatabase` | `RetrievalDatabase` | `RetrievalDatabase` |
| Graph-only product | `GraphDatabase` | `GraphDatabase` | `GraphDatabase` | `GraphDatabase` | `GraphDatabase` |
| Combined product | `GraphRetrievalDatabase` | Same | Same | Same | Same |
| Progressive common ingestion | `Document`/record plus direct embedding | Native document APIs | `Document` plus embedding | document plus `Float32Array` | `Document`/`Record` plus `FloatArray` |
| Dimension ownership | Inferred by Rust builder | Rust | Rust | Rust | Rust |
| Common search family | Exact vector, BM25 text, hybrid | Native overloads | Pythonic overloads | Discriminated query union | Kotlin overloads |
| Query-time `alpha` semantics | Rust validation and ranking | Rust | Rust | Rust | Rust |
| Typed metadata and filters | Rust enums | Swift value types | Python dataclasses/unions | TypeScript unions; `bigint` for i64 | Sealed types; `Long` for i64 |
| Graph query transport | Rust structs | Typed C ABI | Typed PyO3 | Typed N-API | Typed JNI |
| Full graph paths and provenance | Rust-owned | Typed values | Typed values | Typed values | Typed values |
| Opaque graph selection | Generation-bound `GraphResult` | Native resource | Native resource | Async disposable resource | `AutoCloseable` resource |
| Stable candidate projection | Corpus-owned operation | Typed native call | Typed native call | Typed native call | Typed native call |
| Persistence and validation | Rust | Native calls | Native calls | Worker-task native calls | JNI native calls |
| Base excludes graph | Feature off | Separate package/aggregate | Separate wheel | Separate `.node` package | Separate JAR/AAR aggregate |
| Aggregate mixing rule | One aggregate | Link one | Install/load one | Process-global guard | Depend on/load one |
| Lifecycle | Rust ownership | ARC/actors and `close()` | Context manager/`close()` | promises and async disposal | `AutoCloseable` |
| Blocking work policy | Native | Actor isolation | Releases GIL | N-API worker tasks | Caller-selected executor; per-resource native lock |

The wrappers intentionally differ in spelling and concurrency style. Swift uses
actors and labeled arguments; Python uses snake_case, context managers, and
synchronous calls that release the GIL; TypeScript uses promises, discriminated
unions, `Float32Array`, and async disposal; Kotlin uses overloads,
`FloatArray`, sealed values, and `AutoCloseable`. Those differences do not
change corpus ownership or Rust behavior.

## Package and Aggregate Boundaries

The graph aggregate contains retrieval capability, so applications choose
either base or graph:

| Language | Base | Graph aggregate | Initial target |
|---|---|---|---|
| Swift | `RetrievalKit` | `RetrievalKitGraph` | macOS/iOS arm64 family |
| Python | `retrievalkit` | `retrievalkit-graph` | supported CPython targets |
| TypeScript | `retrievalkit-node-local` | `retrievalkit-node-graph-local` | Node.js LTS, macOS arm64 |
| Kotlin/JVM | `retrievalkit` | `retrievalkit-graph` | JVM 11+ |
| Android | `retrievalkit-android` | `retrievalkit-graph-android` | arm64-v8a, API 24+ |

The TypeScript names and Kotlin/Maven coordinates are explicitly provisional
and repository-local. Browser/WASM, Kotlin Multiplatform, other Node native
targets, other Android ABIs, and public registry publication are not implied.

Base artifacts are checked for graph exclusion. Graph artifacts contain the
single graph-capable native aggregate and must not be loaded beside the base
aggregate in one process.

## Boundary and Ownership Review

No hot graph or retrieval query/result path uses JSON in the new or repaired
wrappers. Native values are converted directly:

- Python uses typed PyO3 extraction and construction.
- Node uses typed napi-rs objects and `Float32Array`; signed 64-bit values are
  represented as JavaScript `bigint` without rounding.
- Kotlin uses typed JNI objects and `FloatArray`; native handles are opaque
  numeric registry keys rather than raw pointers.

Node performs blocking native work on worker tasks. Kotlin resources use a
short-held global registry lock only to resolve opaque handles, followed by a
per-resource lock, so unrelated databases do not serialize globally.
Close/removal is idempotent and waits for in-flight access to that resource.

## Correctness Coverage

The wrapper tests cover the relevant portions of the shared contract:

- exact vector, text/BM25, and hybrid search;
- query-time alpha endpoints and invalid arguments;
- metadata filtering and deterministic result ordering;
- dimension mismatch and missing embeddings;
- graph-only traversal and combined graph-scoped retrieval;
- complete path-edge provenance and typed graph traces;
- stable projection filtering, lexical order, stale selections, and
  cross-corpus rejection;
- persistence save/load/validation;
- builder, database, and selection close/consumed behavior;
- base/graph artifact separation and aggregate-mixing rejection;
- local package installation or JVM/AAR artifact consumption.

Existing Rust, Swift, and Python conformance fixtures remain the canonical
cross-wrapper result references. TypeScript and Kotlin use the same fixture
semantics in their repository-local tests; publication-grade fixture
automation can be added when their names and registries are approved.

## Remaining Qualification Work

No architectural blocker remains for the requested source wrappers. The
remaining work is release qualification, not implementation parity:

1. approve final npm and Maven names/coordinates;
2. provision public signing, registry, and CI secrets;
3. qualify additional operating systems, CPU architectures, and Android ABIs
   before advertising them;
4. run separately authorized wrapper-overhead measurements before making any
   new performance claim.

The current source work deliberately ran no benchmark or performance workload.

## Verification Policy

Changes are accepted only after the relevant native feature builds, wrapper
type/lint checks, unit and conformance tests, package-content inspection,
isolated install/consumption smoke tests, and full Rust workspace checks pass.
Shared Rust-core or existing native-boundary changes additionally require the
Swift regression and linkage checks. Exact commands and observed outcomes are
recorded in `docs/product/working-memory.md`.
