# RetrievalKitGraph

RetrievalKit with schema-driven graph capabilities included. This package links
the aggregate `RetrievalKitGraphFFI` artifact, including semantic and hybrid
retrieval. Install it instead of the base `RetrievalKit` native artifact; never
link both native artifacts into one app.

For a human-readable Project Apollo walkthrough and decision guide, start with
the canonical [Swift guide](../../../docs/guides/swift.md).

The package exposes two concrete products over the same canonical corpus model:

- `GraphDatabase` owns corpus and graph capabilities only.
- `GraphRetrievalDatabase` owns corpus, graph, and retrieval capabilities.

Graph-only construction has no metric, encoding, dimension, or embeddings:

```swift
let builder = try GraphDatabase.Builder(
    corpusID: "knowledge",
    schema: graphSchema
)
try await builder.upsert(record)
let database = try await builder.build()

let selection = try await database.query(
    nodeType: "Topic",
    field: "title",
    equals: .string("Rust")
)
let projection = try await database.projectCandidates(
    from: selection,
    filter: .equals("team", .string("mobile"))
)
// Stable lexical (recordID, chunkKey) identities; no internal IDs are exposed.
print(projection.candidates)
```

Combined construction adds retrieval explicitly. A generation-bound graph
selection can scope retrieval without copying records or exposing internal IDs:

```swift
let builder = try GraphRetrievalDatabase.Builder(
    corpusID: "knowledge",
    graph: graphSchema
)
try await builder.upsert(
    Record(
        id: "note-42",
        type: "Topic",
        content: "native retrieval"
    ),
    embedding: embedding
)
let database = try await builder.build()

let selection = try await database.query(
    nodeType: "Topic",
    field: "title",
    equals: .string("Rust")
)
let hits = try await database.search(
    text: "native retrieval",
    embedding: queryEmbedding,
    alpha: 0.6,
    within: selection,
    limit: 10
)
```

Exact, keyword, and hybrid hits return effective metadata using the shared
`MetadataValue` type. Hybrid traces expose `alpha`; graph scope constrains the
candidate set but is not another scoring signal.

Both database owners and their query views are actors. `GraphSelection` retains
its native candidate scope and releases it automatically; callers do not close
query results manually. Database `close()` is available for deterministic early
release, with `deinit` as the normal fallback. Both database types expose
`projectCandidates(from:filter:)`; filtering, generation checks, and stable
identity ordering run in Rust. The returned `GraphCandidateProjection` also
reports source-node and before/after-filter chunk counts.

Run the focused examples:

```bash
scripts/build-xcframework.sh --macos-only --graph
swift run --package-path wrappers/swift/RetrievalKitGraph RetrievalKitGraphQuickstart
swift run --package-path wrappers/swift/RetrievalKitGraph RetrievalKitGraphRetrievalQuickstart
```

Run `scripts/verify-swift-graph-wrapper.sh` for linkage isolation, all Swift
tests, and exact output checks for retrieval-only, graph-only, and combined
examples. Rust and Swift also consume the generic conformance fixture at
`benchmarks/graph-conformance/v1/fixture.json`; no customer data is required.

`GraphIndex` and `GraphIndexBuilder` remain temporary compatibility APIs. New
code should use the capability-specific database products above.
