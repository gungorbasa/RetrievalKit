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
