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
      encoding: .f32
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
        metadata: ["status": .string("approved")],
        content: "We chose Swift for Project Apollo's Apple platform client."
      ),
      embedding: [1, 0]
    )
    try await builder.upsert(
      Record(
        id: "launch-checklist",
        type: "Note",
        metadata: ["status": .string("draft")],
        content: "Project Apollo launch checklist and release owners."
      ),
      embedding: [0, 1]
    )
    let database = try await builder.build()
    let selection = try await database.query(
      from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
      traversing: [GraphTraversal(relationship: "contains")]
    )
    let hits = try await database.search(
      text: "Why did we choose Swift?",
      embedding: [1, 0],
      within: selection,
      limit: 1,
      filter: .equals("status", .string("approved"))
    )
    print("graph-hybrid=\(hits[0].recordID)")
  }
}
