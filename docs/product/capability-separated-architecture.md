# Capability-Separated RetrievalKit Architecture

Date: 2026-07-12

## Decision

RetrievalKit stores one canonical corpus and composes optional derived query
capabilities over it:

```text
CorpusIndex
  RecordStore
  stable RecordId / ChunkKey -> dense ChunkId maps
  generation, active chunks, text, metadata, hydration, lifecycle

RetrievalIndex
  exact vectors
  BM25 for direct hybrid queries
  filtering and existing scoring loops

GraphEngine
  canonical GraphSchema
  nodes, edges, traversal, projection to ChunkId
```

The concrete owners are:

```text
RetrievalDatabase      = CorpusIndex + RetrievalIndex
GraphDatabase          = CorpusIndex + GraphEngine
GraphRetrievalDatabase = CorpusIndex + GraphEngine + RetrievalIndex
```

This is composition, not three copies of the data. Each database has one
`CorpusIndex` and one generation. Derived payloads are immutable after build
and validated against that corpus.

## Public Capability Contract

- `GraphDatabase` accepts a graph schema and capability-neutral records. It
  never accepts dimensions, vector metrics, encodings, or embeddings.
- `RetrievalDatabase` accepts searchable `Document` values paired directly
  with caller-produced embeddings. The first embedding fixes its dimension.
- `GraphRetrievalDatabase` accepts graph-only `Record` values, the common
  `Record` plus embedding path for one searchable `content` value, and an
  advanced list of stable `EmbeddedDocument` values. It is the only owner that
  composes a `GraphSelection` with retrieval.
- BM25 is built for every retrieval-capable database. High-level hybrid calls
  accept query-time `alpha`; direct text-only overloads run BM25 without a
  query embedding.
- Capability selection occurs when the builder is created. It is not inferred
  from data or attached to a completed database.

The common public values are independent from internal chunking:

```text
Document
  id: DocumentId
  text
  metadata

Record
  id: RecordId
  type: RecordType
  fields
  metadata
  content?

EmbeddedDocument
  document
  embedding
```

`ChunkKey`, `RecordInput`, and keyed embedding maps remain internal or
lower-level graph compatibility concepts; progressive retrieval builders do not
expose them. Graph-only upserts have no embedding parameter.
Internally a stable document ID maps to the record-bound chunk identity used by
candidate projection and persistence.

## Candidate Projection Contract

`CorpusIndex` owns generation validation, metadata-filter intersection, and
stable identity materialization for candidate scopes. The production Rust API
is:

```rust
CorpusIndex::filter_candidate_scope(scope, filter)
CorpusIndex::candidate_scope_identities(scope)
```

`CandidateScope` remains opaque. Callers cannot inspect internal chunk IDs,
membership, or the adaptive sparse/dense representation. Filtering uses the
same production `Filter::matches` semantics as retrieval, rejects stale or
cross-corpus scopes even when no filter is supplied, and excludes unavailable,
deleted, or superseded chunks. Identity materialization returns each stable
`(RecordId, ChunkKey)` once in lexical order and rejects inconsistent mappings.

`GraphEngine` still performs graph-to-scope projection. `GraphDatabase` and
`GraphRetrievalDatabase` then delegate optional filtering and identity
materialization to their one canonical corpus through
`project_candidate_identities`. Graph-only projection does not construct or
require vector, BM25, or embedding state.

The aggregate graph C ABI exposes typed, non-JSON projection operations for
both database types. The caller receives owned `record_id`/`chunk_key` strings,
`source_nodes`, and before/after filter counts, and releases the complete value
with the provided free or clear function. Swift maps that result to
`GraphChunkIdentity` and `GraphCandidateProjection` through actor-isolated
`projectCandidates(from:filter:)` methods. Stale and cross-database selections
use the existing typed stale-generation error.

The work is O(scope size), aside from the required lexical identity sort, and
does not scan the corpus. A no-filter projection still validates generation and
scope membership. The current production Python graph surface does not expose
this operation; an equivalent native projection must be added when that wrapper
is introduced rather than reimplementing filtering in Python.

## Persistence Contract

Each atomically activated immutable generation stores the corpus once and only
the enabled derived payloads:

```text
.snapshots/<generation>/
  corpus/
  retrieval/   # optional
  graph/       # optional; contains schema.json and graph.bin
```

The manifest records the database capability variant, retrieval mode, format
version, payload sizes, and BLAKE3 digests. Save/load validation, writer locks,
reader leases, staged generation recovery, and atomic manifest activation keep
their existing guarantees. Development snapshots from the previous composite
format fail with an actionable incompatible-version error; no compatibility
adapter is required.

## Swift Contract

Swift exposes the three concrete database types. Each owns actor query views:

```text
GraphDatabase.graph
RetrievalDatabase.retrieval
GraphRetrievalDatabase.graph
GraphRetrievalDatabase.retrieval
```

Identity wrappers are string-literal expressible and nonthrowing to construct;
Rust validates them at `upsert`. Database and engine handles are actor-isolated
and automatically release native ownership. Explicit `close()` remains for
deterministic release. Immutable graph selections release automatically and do
not require normal callers to call `close()`.

Swift performs marshaling, ownership, concurrency coordination, and error
mapping only. Schema validation, indexing, filtering, ranking, traversal,
candidate projection, and persistence remain Rust logic.

## Error Contract

Public typed errors include invalid identity, duplicate chunk key, missing or
unexpected embedding, invalid embedding dimension, unavailable retrieval
capability, stale selection, consumed/closed handle, schema validation, and
persistence corruption. Every mapped message names the problem, relevant
identity or expected value, and correction. Dimension mismatch is never an
internal error.

## Benchmark Protocol

The graph-free gate uses a release build, the pinned 10K x 384d deterministic
fixture, a prebuilt warmed index, 100 warmups, 1,000 measured samples,
nearest-rank p95, top-k 10, and excludes embedding generation. Compare on the
same host/toolchain using interleaved baseline and candidate runs; report the
median p95 of at least three final runs for exact, internal BM25, and hybrid.
Each candidate p95 must remain within 3% of the pre-refactor baseline.

## Commit Gates

1. Contract and benchmark protocol.
2. Canonical `CorpusIndex` extraction.
3. `RetrievalIndex` with exact-vector and BM25 state.
4. `GraphEngine`, database owners, and capability persistence.
5. Capability-specific FFI and errors.
6. Swift products, examples, and tests.
7. Full qualification and benchmark report.

Each gate must pass its scoped tests and end in a clean commit before the next
begins.
