# RetrievalKit for Swift

RetrievalKit is a local-first retrieval SDK with three capability-specific
database types:

| Use case | Database |
|---|---|
| Searchable documents without relationships | `RetrievalDatabase` |
| Graph records and traversal without retrieval | `GraphDatabase` |
| Graph traversal followed by scoped retrieval | `GraphRetrievalDatabase` |

The base and graph packages contain alternative native aggregates. An
application links `RetrievalKit` or `RetrievalKitGraph`, never both.

## Retrieval only

Build the local XCFramework and run the checked-in example:

```bash
scripts/build-xcframework.sh --macos-only
swift run --package-path wrappers/swift/RetrievalKit \
  RetrievalKitRetrievalQuickstart
```

The caller owns document IDs, text, metadata, and embeddings. RetrievalKit
infers the dimension from the first embedding:

```swift
import RetrievalKit

let builder = try RetrievalDatabase.Builder(
  corpusID: "project-notes",
  encoding: .f32
)

try await builder.upsert(
  Document(
    id: "decision-swift",
    text: "We chose Swift for Project Apollo's Apple platform client.",
    metadata: [
      "project": .string("apollo"),
      "status": .string("approved"),
    ]
  ),
  embedding: [1, 0]
)

try await builder.upsert(
  Document(
    id: "launch-checklist",
    text: "Project Apollo launch checklist and release owners.",
    metadata: [
      "project": .string("apollo"),
      "status": .string("draft"),
    ]
  ),
  embedding: [0, 1]
)

let database = try await builder.build()

let hits = try await database.search(
  text: "Why did we choose Swift?",
  embedding: [1, 0],
  alpha: 0.6,
  limit: 1
)
```

Expected first document ID:

```text
decision-swift
```

## One search family

The arguments determine the retrieval behavior:

```swift
// Exact vector search.
let semantic = try await database.search(
  embedding: queryEmbedding,
  limit: 10
)

// BM25 text search.
let lexical = try await database.search(
  text: "private search",
  limit: 10
)

// Weighted vector + BM25 search.
let hybrid = try await database.search(
  text: "private search",
  embedding: queryEmbedding,
  alpha: 0.6,
  limit: 10,
  filter: .equals("project", .string("apollo"))
)
```

`alpha` is the vector weight. `1` is vector-only, `0` is BM25-only, and values
between them combine both signals.

## Graph only

Graph-only databases never accept embeddings or retrieval configuration:

```swift
import RetrievalKitGraph

let builder = try GraphDatabase.Builder(
  corpusID: "project-notes",
  schema: schema
)

try await builder.upsert(
  Record(
    id: "apollo",
    type: "Project",
    fields: [
      "note_ids": .list([
        .string("decision-swift"),
        .string("launch-checklist"),
      ])
    ]
  )
)

try await builder.upsert(
  Record(
    id: "decision-swift",
    type: "Note",
    content: "We chose Swift for Project Apollo."
  )
)

let database = try await builder.build()
let result = try await database.query(
  from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
  traversing: [GraphTraversal(relationship: "contains")]
)
```

The graph result contains matched nodes and paths and can be used without
retrieval.

## Graph and retrieval together

Build and run the combined example:

```bash
scripts/build-xcframework.sh --macos-only --graph
swift run --package-path wrappers/swift/RetrievalKitGraph \
  RetrievalKitGraphRetrievalQuickstart
```

The common path stores graph records without embeddings and searchable records
with one caller-produced embedding:

```swift
let builder = try GraphRetrievalDatabase.Builder(
  corpusID: "project-notes",
  graph: schema,
  encoding: .f32
)

try await builder.upsert(project)

try await builder.upsert(
  Record(
    id: "decision-swift",
    type: "Note",
    metadata: ["status": .string("approved")],
    content: "We chose Swift for Project Apollo's Apple platform client."
  ),
  embedding: [1, 0]
)

let database = try await builder.build()
let selection = try await database.query(
  from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
  traversing: [GraphTraversal(relationship: "contains")]
)

let hits = try await database.search(
  text: "Why did we choose Swift?",
  embedding: [1, 0],
  alpha: 0.6,
  within: selection,
  limit: 1,
  filter: .equals("status", .string("approved"))
)
```

The graph selects candidates; the same vector + BM25 ranker orders those
candidates. Graph evidence is not silently mixed into the retrieval score.

Combined hits expose both identities:

```swift
hits[0].documentID  // searchable document
hits[0].recordID    // owning graph record
```

For a record with multiple searchable documents, use the advanced overload:

```swift
try await builder.upsert(
  record,
  documents: [
    EmbeddedDocument(
      id: "note-42:summary",
      text: summary,
      embedding: summaryEmbedding
    ),
    EmbeddedDocument(
      id: "note-42:body",
      text: body,
      embedding: bodyEmbedding
    ),
  ]
)
```

## Persistence

Save, validate, and load complete database generations:

```swift
let snapshot = URL(fileURLWithPath: "project-notes.rk")
try await database.save(to: snapshot)
try GraphRetrievalDatabase.validate(at: snapshot)
let reloaded = try GraphRetrievalDatabase.load(from: snapshot)
```

Use the corresponding `RetrievalDatabase` or `GraphDatabase` static methods for
the other capability sets.

## Embeddings stay your choice

The two-dimensional vectors above make the examples deterministic; they are not
a production embedding model. Bring embeddings from any model or use
[`EmbeddingKit`](../../wrappers/swift/EmbeddingKit/README.md). Every document
and query embedding in one database must come from the same model.

For lower-level ownership and build details, see the
[`RetrievalKit` wrapper reference](../../wrappers/swift/RetrievalKit/README.md)
and
[`RetrievalKitGraph` wrapper reference](../../wrappers/swift/RetrievalKitGraph/README.md).
