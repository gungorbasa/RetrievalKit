import RetrievalKitGraph

@main
struct RetrievalKitGraphRetrievalQuickstart {
  static func main() async throws {
    let schema = GraphSchema(recordNodes: [
      GraphRecordNodeSchema(
        recordType: "Topic",
        nodeType: "Topic",
        queryableFields: ["title"]
      )
    ])
    let builder = try GraphRetrievalDatabase.Builder(
      corpusID: "knowledge",
      graph: schema,
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32)
      )
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "rust",
          type: "Topic",
          fields: ["title": .string("Rust")]
        ),
        chunks: [
          Chunk(key: "summary", text: "Rust provides native retrieval.")
        ]
      ),
      embeddings: ["summary": [1, 0]]
    )
    let database = try await builder.build()
    let selection = try await database.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )
    let hits = try await database.retrieval.hybridSearch(
      text: "native retrieval",
      embedding: [1, 0],
      within: selection
    )
    print("combined=\(hits.map(\.recordID).joined(separator: ","))")
  }
}
