# Rust, Swift, and Python Parity Audit

Date: 2026-07-12

## Verdict

Rust, Swift, and Python now share the same capability-separated database
architecture and canonical ingestion model. The audit fixed the two structural
Python mismatches that previously prevented that conclusion:

1. Python now exposes `RetrievalDatabase`, `GraphDatabase`, and
   `GraphRetrievalDatabase` as its canonical database products.
2. Retrieval-capable Python builders now accept capability-neutral records and
   a separate embedding map keyed by stable chunk key. Embeddings are no longer
   fields on canonical chunks.

The architecture is aligned. Full performance-and-result-surface qualification
is not yet complete because the remaining follow-ups below require benchmarked
FFI work rather than wrapper-only changes.

## Canonical Architecture Matrix

| Contract | Rust | Swift | Python | Status |
|---|---|---|---|---|
| Canonical corpus owns records, chunks, identities, and generation | `CorpusIndex` | Native handle ownership | Native PyO3 ownership | Aligned |
| Retrieval-only database | `RetrievalDatabase` | `RetrievalDatabase` | `RetrievalDatabase` | Aligned |
| Graph-only database | `GraphDatabase` | `GraphDatabase` | `GraphDatabase` | Aligned |
| Combined graph and retrieval database | `GraphRetrievalDatabase` | `GraphRetrievalDatabase` | `GraphRetrievalDatabase` | Aligned |
| Graph excluded from base package | Off-by-default Cargo feature | Separate `RetrievalKitGraph` package and aggregate XCFramework | Separate `retrievalkit-graph` distribution | Aligned |
| Semantic and hybrid retrieval with query-time alpha | `RetrievalConfiguration` | `RetrievalConfiguration` | `RetrievalConfiguration` | Aligned |
| Graph schema separate from retrieval configuration | Separate constructor arguments | `graph:` and `retrieval:` | `graph=` and `retrieval=` | Aligned |
| Capability-neutral record and chunks | `RecordInput` and `CorpusChunkInput` | `RecordInput` and `Chunk` | Nested typed dictionaries `RecordInput` and `RecordChunk`/`Chunk` | Aligned |
| Embeddings supplied separately by stable chunk key | Core builder marshaling contract | `[ChunkKey: [Float]]` | `Mapping[str, Sequence[float]]` | Aligned |
| Exact embedding-key validation | Rust validation | Swift preflight plus Rust | Python preflight plus Rust | Aligned |
| Separate query capabilities | `graph()` and `retrieval()` accessors | `.graph` and `.retrieval` actors | `.graph` and `.retrieval` objects | Aligned |
| Semantic retrieval | Rust exact search | `semanticSearch` | `semantic_search` | Aligned |
| Hybrid retrieval | Rust hybrid ranking | `hybridSearch` | `hybrid_search` | Aligned |
| Graph selection scopes retrieval | Generation-bound `GraphResult` projection | `within:` | `within=` | Aligned |
| Graph selection projects stable chunk identities | Corpus-owned `project_candidate_identities` | Actor-isolated `projectCandidates(from:filter:)` | Not exposed | Rust/Swift aligned; Python deferred |
| Metadata filter semantics | Rust `Filter` | Swift typed `Filter`/`GraphFilter` | Python `where={...}` and helpers | Aligned |
| Save, load, and read-only validation | Rust persistence implementation | Swift native calls | Python native calls | Aligned |
| Closed/consumed lifecycle safety | Rust ownership and typed results | ARC, actors, and explicit `close()` | native ownership, `close()`, and context managers | Aligned, idiomatic |
| Dimension and unavailable-capability errors | Typed Rust variants | Typed Swift cases | Typed Python exceptions | Aligned |
| Cross-wrapper graph fixture | Canonical fixture assertions | Canonical fixture assertions | Canonical fixture assertions | Aligned |

## Intentional Native-Language Differences

These differences preserve architecture and behavior while making each API
native to its language:

- Rust uses ownership, borrowing, `Result`, and explicit query structs.
- Swift uses actors, `async throws`, labeled arguments, typed identifiers, and
  deterministic ARC ownership.
- Python uses snake_case, keyword arguments, ordinary mappings and sequences,
  synchronous calls that release the GIL, iterable bulk ingestion, and context
  managers.
- Swift spells query controls as `topK` and `filter`; Python uses `limit` and
  `where`.
- Python offers a bulk `add(...)` convenience in addition to single-record
  `upsert(...)`. It does not change canonical ownership or Rust behavior.
- Python graph queries offer a seconds-based timeout implemented through the
  Rust cancellation token. Swift currently exposes explicit cancellation.
- Python exposes ordered record/chunk hydration for developer validation. This
  reads canonical Rust-owned state and does not create a second payload owner.
- The older Python `Index` API remains as a compatibility, pipeline, mutation,
  compaction, and direct-BM25 surface. It is not the canonical high-level
  database architecture, and standalone keyword search is not added to the new
  database products.

## Gaps Fixed During This Audit

- Added the Python `RetrievalDatabaseBuilder`, `RetrievalDatabase`, and
  `RetrievalQueries` API.
- Moved Python retrieval ingestion to the shared record-plus-embedding-map
  contract for both retrieval-only and combined graph retrieval.
- Added exact missing and unexpected embedding-key failures at the Python API
  boundary.
- Added a typed Python `RetrievalCapabilityUnavailableError` and
  `InvalidIdentityError` mapping.
- Preserved graph-free base packaging and verified that the base wheel contains
  neither graph Python files nor a `retrievalkit-graph` native dependency.
- Removed stale generated bytecode from wheel inputs after the audit found that
  local `__pycache__` files could otherwise be packaged.

## Remaining Qualification Work

These are not architectural ownership differences, but they matter for the
speed-and-quality bar and must be completed before claiming exhaustive wrapper
parity:

1. **P1: remove JSON from the Python graph query path.** Python currently
   serializes graph query requests and materialized selections through JSON at
   the PyO3 boundary. Swift uses typed C values. Replace the Python graph query
   transport with typed PyO3 conversion and benchmark wrapper overhead
   separately from Rust traversal.
2. **P1: unify result and trace payloads.** Python returns hydrated metadata and
   the complete Rust hybrid fusion trace. The current Swift capability result
   structs omit metadata and fusion configuration. Decide the canonical public
   result contract, extend the FFI once, and qualify both Swift packages against
   it. Do not reconstruct Rust ranking traces in wrapper code.
3. **P1: add a retrieval-only cross-wrapper fixture.** Graph retrieval already
   shares a canonical fixture across Rust, Swift, and Python. Add an equivalent
   retrieval-only fixture covering compact persistence with BM25 rebuild,
   metadata filters, alpha-controlled hybrid ordering, and trace equality.
4. **P2: benchmark Python wrapper overhead.** The Rust retrieval path remains
   native and releases the GIL, but the new Python builder and query view need a
   pinned release-mode wrapper-overhead report before performance claims.

Until these items are complete, describe the state as **architecture parity
with qualified graph behavior**, not exhaustive result/performance parity.

The stable candidate-identity projection added after this audit is deliberately
not counted as Python parity. Its filtering and generation semantics are
corpus-owned in Rust and exposed through typed C and Swift APIs. Any future
production Python graph wrapper must call the same Rust operation and return the
same lexical identities and counts; it must not reproduce scope filtering in
Python.

## Verification Performed

- Cargo compilation with and without the Python graph feature.
- Ruff and strict mypy checks for both Python distributions.
- Isolated installed-wheel tests for the base and graph distributions.
- Graph-free wheel content inspection and Cargo dependency-tree inspection.
- Existing shared graph-conformance fixture through Python; Rust and Swift
  coverage remains in their existing qualification suites.
