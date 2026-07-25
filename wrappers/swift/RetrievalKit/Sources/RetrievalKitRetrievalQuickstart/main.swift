import RetrievalKit

@main
struct RetrievalKitRetrievalQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(corpusID: "project-notes", encoding: .f32)
    try await builder.upsert(
      Document(
        id: "decision-swift",
        text: "We chose Swift for Project Apollo's Apple platform client.",
        metadata: ["project": .string("apollo"), "status": .string("approved")]
      ),
      embedding: [1, 0]
    )
    try await builder.upsert(
      Document(
        id: "launch-checklist",
        text: "Project Apollo launch checklist and release owners.",
        metadata: ["project": .string("apollo"), "status": .string("draft")]
      ),
      embedding: [0, 1]
    )
    let database = try await builder.build()
    let hits = try await database.search(
      text: "Why did we choose Swift?",
      embedding: [1, 0],
      limit: 1
    )
    print("hybrid=\(hits[0].documentID)")
  }
}
