# VectorKitGraph

Optional schema-driven local graph retrieval for Swift. This package links the
aggregate `VectorKitGraphFFI` artifact. Install it instead of the base
`VectorKit` package; never link both native artifacts into one application.

`GraphIndexBuilder` accepts domain-neutral records and consumes itself when
`build(schema:)` creates the sole graph owner. Schema and record JSON are
cold-path transport validated in Rust. `GraphIndex.query` uses typed native
node-ID or exact queryable-property seeds (`String`, `Int64`, and `Bool`),
bounded traversal steps, limits, result matches, traces, and an atomic
`GraphCancellationToken`; query hot paths do not parse JSON. Every match
materializes its canonical path, including relationship endpoints and schema,
source-field, inverse-edge, and built-in-edge provenance.
Rust graph failures cross the ABI as stable graph-specific status codes. Swift
maps them to `VectorKitGraphError` cases for invalid schema/identity, stale
generation, incompatible version, unavailable graph data, corrupt snapshots,
query limits, cancellation, timeout, lock contention, and internal failures;
Swift does not re-run graph validation to classify errors.
Builder, index, query-result/scope, and cancellation-token owners provide
idempotent `close()` methods for deterministic native resource release; `deinit`
remains a fallback. Closed resources reject further work with
`VectorKitGraphError.graphUnavailable`. Result and cancellation-token closure
is synchronized against active native calls so concurrent closure cannot free a
handle still in use.
`GraphIndex` admits immutable graph queries and scoped rankers through shared
read leases and runs native work in detached tasks, so independent reads can
execute concurrently instead of blocking the Swift actor. Composite saves and
explicit index closure use writer-preferring exclusive leases; once either is
waiting, later reads cannot starve it.
Each `GraphQueryResult` retains its native generation-bound candidate scope and
can feed `search`, `keywordSearch`, or `hybridSearch` without exporting internal
chunk IDs or changing the graph-free ranking implementations. All three scoped
rankers accept composable `GraphFilter` metadata predicates. Hybrid retrieval
also accepts candidate limits and RRF or weighted normalized-score fusion
through `GraphHybridOptions`; results expose the native ranks, normalized
scores, matched terms, and filter decision.
