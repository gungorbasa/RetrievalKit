# Capability-Separated VectorKit Architecture

Date: 2026-07-12

## Decision

VectorKit stores one canonical corpus and composes optional derived query
capabilities over it:

```text
CorpusIndex
  RecordStore
  stable RecordId / ChunkKey -> dense ChunkId maps
  generation, active chunks, text, metadata, hydration, lifecycle

RetrievalIndex
  exact vectors
  BM25 only in hybrid mode
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
- `RetrievalDatabase` accepts `.semantic` or `.hybrid` retrieval configuration
  and embeddings keyed by stable chunk key.
- `GraphRetrievalDatabase` accepts both configurations and is the only owner
  that composes a `GraphSelection` with semantic or hybrid retrieval.
- BM25 is built only for hybrid mode. It remains directly tested and
  benchmarkable in Rust, but has no standalone high-level Swift database mode.
- Capability selection occurs when the builder is created. It is not inferred
  from data or attached to a completed database.

The common record input is independent from capabilities:

```text
RecordInput
  record
    id: RecordId
    type: RecordType
    fields: nested RecordValue map
    metadata: flat filter metadata inherited by chunks
  chunks[]
    key: ChunkKey
    text
    metadata: flat per-chunk filter metadata
```

Chunk metadata overrides inherited record metadata on a duplicate key.
Retrieval-capable upserts provide a separate embedding map whose keys must
exactly match the input chunks. Graph-only upserts have no embedding parameter.

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
unexpected embedding, invalid embedding dimension, unavailable retrieval mode,
stale selection, consumed/closed handle, schema validation, and persistence
corruption. Every mapped message names the problem, relevant identity or
expected value, and correction. Dimension mismatch is never an internal error.

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
3. `RetrievalIndex` and retrieval modes.
4. `GraphEngine`, database owners, and capability persistence.
5. Capability-specific FFI and errors.
6. Swift products, examples, and tests.
7. Full qualification and benchmark report.

Each gate must pass its scoped tests and end in a clean commit before the next
begins.
