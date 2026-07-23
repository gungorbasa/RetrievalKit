# RetrievalKit for Swift

RetrievalKit is one local retrieval system with two Swift packages:

- Link `RetrievalKitGraph` when records have useful relationships. It already
  contains semantic and hybrid retrieval.
- Link `RetrievalKit` when the corpus is flat and graph traversal would add no
  value.

The packages contain alternative native aggregates, so an app links one or the
other—never both. The examples below use the same Project Apollo notes to show
that graph scope changes the candidate set, not the ranker.

## Choose the right path

| Your data and question | Use | Why |
|---|---|---|
| Notes belong to projects, messages belong to threads, or documents cite one another | `RetrievalKitGraph` with `GraphRetrievalDatabase` | Traverse relationships to choose candidates, then rank those candidates |
| Records form a flat collection | `RetrievalKit` with `RetrievalDatabase` | Get semantic and hybrid retrieval without defining a graph |
| You have query text and an embedding | Hybrid search | Meaning and exact keyword evidence can support each other |
| You have only an embedding, or wording should not matter | Semantic search | Rank by vector similarity alone |
| You need hard tenant, status, type, or date rules | Metadata filters | Filters are constraints and work with either retrieval mode |
| You only need traversal and candidate projection | `GraphDatabase` | Avoid retrieval configuration and embeddings entirely |

Hybrid search is the normal default for app and document search. Semantic-only
search is a query variation for cases where keyword evidence is unavailable or
deliberately irrelevant; it is not a separate database architecture.

## Complete product: graph-scoped hybrid search

Suppose a workspace contains notes from many projects. The user asks:
“Why did we choose Swift?” while viewing Project Apollo.

The graph answers *where to search*: start at Apollo and follow `contains` to
its notes. The metadata filter requires an approved note. Hybrid retrieval then
answers *which candidate ranks first* using semantic and BM25 evidence.

Build the graph-enabled XCFramework and run the checked-in program:

```bash
scripts/build-xcframework.sh --macos-only --graph
swift run --package-path wrappers/swift/RetrievalKitGraph \
  RetrievalKitGraphRetrievalQuickstart
```

Complete runnable source:

```swift
import RetrievalKitGraph

@main
struct RetrievalKitGraphRetrievalQuickstart {
  static func main() async throws {
    let schema = GraphSchema(
      recordNodes: [
        GraphRecordNodeSchema(recordType: "Project", nodeType: "Project"),
        GraphRecordNodeSchema(recordType: "Note", nodeType: "Note"),
      ],
      relationships: [
        GraphRelationshipSchema(
          relationshipType: "contains",
          sourceNodeType: "Project",
          targetNodeType: "Note",
          sourceField: "note_ids",
          cardinality: .many
        )
      ]
    )
    let builder = try GraphRetrievalDatabase.Builder(
      corpusID: "project-notes",
      graph: schema,
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32)
      )
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "apollo",
          type: "Project",
          fields: [
            "note_ids": .list([
              .string("decision-swift"),
              .string("launch-checklist"),
            ])
          ]
        )
      ),
      embeddings: [:]
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "decision-swift",
          type: "Note",
          metadata: ["status": .string("approved")]
        ),
        chunks: [
          Chunk(
            key: "body",
            text: "We chose Swift for Project Apollo's Apple platform client."
          )
        ]
      ),
      embeddings: ["body": [1, 0]]
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "launch-checklist",
          type: "Note",
          metadata: ["status": .string("draft")]
        ),
        chunks: [
          Chunk(
            key: "body",
            text: "Project Apollo launch checklist and release owners."
          )
        ]
      ),
      embeddings: ["body": [0, 1]]
    )
    let database = try await builder.build()
    let selection = try await database.graph.query(
      from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
      traversing: [GraphTraversal(relationship: "contains")]
    )
    let hits = try await database.retrieval.hybridSearch(
      text: "Why did we choose Swift?",
      embedding: [1, 0],
      topK: 1,
      within: selection,
      filter: .equals("status", .string("approved"))
    )
    print("graph-hybrid=\(hits[0].recordID)")
  }
}
```

Expected output:

```text
graph-hybrid=decision-swift
```

The relationship is application data: RetrievalKit validates and traverses
`note_ids`, but it does not extract or invent relationships.

## Simpler product: hybrid search without a graph

If `project` is just metadata and users do not navigate relationships, use the
base package:

```bash
scripts/build-xcframework.sh --macos-only
swift run --package-path wrappers/swift/RetrievalKit \
  RetrievalKitRetrievalQuickstart
```

The complete program is:

```swift
import RetrievalKit

@main
struct RetrievalKitRetrievalQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "project-notes",
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32)
      )
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "decision-swift",
          type: "Note",
          metadata: [
            "project": .string("apollo"),
            "status": .string("approved"),
          ]
        ),
        chunks: [
          Chunk(
            key: "body",
            text: "We chose Swift for Project Apollo's Apple platform client."
          )
        ]
      ),
      embeddings: ["body": [1, 0]]
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "launch-checklist",
          type: "Note",
          metadata: [
            "project": .string("apollo"),
            "status": .string("draft"),
          ]
        ),
        chunks: [
          Chunk(
            key: "body",
            text: "Project Apollo launch checklist and release owners."
          )
        ]
      ),
      embeddings: ["body": [0, 1]]
    )
    let database = try await builder.build()
    let hits = try await database.retrieval.hybridSearch(
      text: "Why did we choose Swift?",
      embedding: [1, 0],
      topK: 1
    )
    print("hybrid=\(hits[0].documentID)")
  }
}
```

Expected output:

```text
hybrid=decision-swift
```

For a hard Apollo-only constraint, pass
`filter: .all([.equals("project", .string("apollo")), ...])`. Use a graph
selection instead when “inside Apollo” means traversing explicit relationships
rather than comparing fields.

## Semantic-only is a query variation

Both database types expose semantic search. Reuse the database and omit query
text:

```swift
// Graph-enabled database; `within` is optional.
let semanticHits = try await database.retrieval.semanticSearch(
  embedding: [1, 0],
  topK: 1,
  within: selection,
  filter: .equals("status", .string("approved"))
)
```

On a base `RetrievalDatabase`, call the same method without `within`:

```swift
let semanticHits = try await database.retrieval.semanticSearch(
  embedding: [1, 0],
  topK: 1,
  filter: .equals("project", .string("apollo"))
)
```

Choose this when there is no meaningful query text—for example, finding notes
similar to another note—or when exact terms should intentionally have no
influence.

## Traces and persistence

Hybrid hits expose the fused score and an explanation of each component:

```swift
let hit = hits[0]
print(hit.trace.vectorRank as Any)
print(hit.trace.keywordRank as Any)
print(hit.trace.matchedTerms)
print(hit.trace.filterMatched)
```

Save, validate, and reload the complete graph, corpus, retrieval indexes, and
metadata together:

```swift
let snapshot = URL(fileURLWithPath: "project-notes.rk")
try await database.save(to: snapshot)
try GraphRetrievalDatabase.validate(at: snapshot)
let reloaded = try GraphRetrievalDatabase.load(from: snapshot)
```

Use `RetrievalDatabase.save`, `validate`, and `load` for the base package.
Persistence, filtering, graph traversal, ranking, and trace construction all
run in the shared Rust core.

## Embeddings stay your choice

The two-dimensional vectors above make the example deterministic; they are not
a production embedding model. RetrievalKit requires one caller-provided
embedding per indexed chunk and a query embedding from the same model.

For an on-device text-to-results pipeline, use
[`EmbeddingKit`](../../wrappers/swift/EmbeddingKit/README.md) with
[`RetrievalKitPipeline`](../../wrappers/swift/RetrievalKitPipeline/README.md).
If your app sends text to a remote embedding API, that embedding step is remote
even though indexing and retrieval remain local.

For lower-level build, ownership, and API details, see the
[`RetrievalKit` wrapper reference](../../wrappers/swift/RetrievalKit/README.md)
and
[`RetrievalKitGraph` wrapper reference](../../wrappers/swift/RetrievalKitGraph/README.md).
