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
Each `GraphQueryResult` retains its native generation-bound candidate scope and
can feed `search`, `keywordSearch`, or `hybridSearch` without exporting internal
chunk IDs or changing the graph-free ranking implementations.
