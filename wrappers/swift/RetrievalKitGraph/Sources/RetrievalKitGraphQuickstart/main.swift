import RetrievalKitGraph

@main
struct RetrievalKitGraphQuickstart {
  static func main() async throws {
    let schema = GraphSchema(recordNodes: [
      GraphRecordNodeSchema(
        recordType: "Topic",
        nodeType: "Topic",
        queryableFields: ["title"]
      )
    ])
    let builder = try GraphDatabase.Builder(
      corpusID: "knowledge",
      schema: schema
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
      )
    )
    let database = try await builder.build()
    let selection = try await database.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )
    print("graph=\(selection.matches.map(\.nodeID.recordID).joined(separator: ","))")
  }
}
