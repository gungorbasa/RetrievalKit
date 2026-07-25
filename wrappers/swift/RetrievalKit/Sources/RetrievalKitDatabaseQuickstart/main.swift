import RetrievalKit

@main
enum DatabaseQuickstart {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(corpusID: "project-notes")
    try await builder.upsert(
      Document(
        id: "decision-swift",
        text: "We chose Swift for Project Apollo's Apple platform client.",
        metadata: ["project": .string("apollo")]
      ),
      embedding: [1, 0]
    )
    let database = try await builder.build()
    let hits = try await database.search(embedding: [1, 0], limit: 1)
    print("semantic=\(hits[0].documentID)")
  }
}
