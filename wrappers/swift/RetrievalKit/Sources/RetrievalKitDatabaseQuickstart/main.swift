import RetrievalKit

@main
enum DatabaseQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "project-notes",
      retrieval: RetrievalConfiguration(
        semantic: VectorIndexConfiguration(dimension: 2)
      )
    )
    try await builder.upsert(
      RecordInput(
        record: Record(
          id: "decision-swift",
          type: "Note",
          metadata: ["project": .string("apollo")]
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
    let database = try await builder.build()
    let hits = try await database.retrieval.semanticSearch(
      embedding: [1, 0], topK: 1
    )
    print("semantic=\(hits[0].documentID)")
  }
}
