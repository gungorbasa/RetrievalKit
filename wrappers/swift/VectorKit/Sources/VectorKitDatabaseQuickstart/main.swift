import VectorKit

@main
enum DatabaseQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "docs",
      retrieval: RetrievalConfiguration(
        semantic: VectorIndexConfiguration(dimension: 3)
      )
    )
    try await builder.upsert(
      RecordInput(
        record: Record(id: "local-first", type: "Article"),
        chunks: [Chunk(key: "summary", text: "Private retrieval on device.")]
      ),
      embeddings: ["summary": [1, 0, 0]]
    )
    let database = try await builder.build()
    let hits = try await database.retrieval.semanticSearch(
      embedding: [1, 0, 0], topK: 1
    )
    print(hits[0].documentID)
  }
}
