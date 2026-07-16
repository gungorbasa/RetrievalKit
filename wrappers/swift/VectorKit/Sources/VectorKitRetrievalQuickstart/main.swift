import VectorKit

@main
struct VectorKitRetrievalQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "knowledge",
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
    let hits = try await database.retrieval.hybridSearch(
      text: "native retrieval",
      embedding: [1, 0],
      topK: 10
    )
    print("retrieval=\(hits.map(\.documentID).joined(separator: ","))")
  }
}
