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
          metadata: ["project": .string("apollo"), "status": .string("approved")]
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
          metadata: ["project": .string("apollo"), "status": .string("draft")]
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
