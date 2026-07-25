import RetrievalKitGraph

@main
struct RetrievalKitGraphQuickstart {
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
    let builder = try GraphDatabase.Builder(corpusID: "project-notes", schema: schema)
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
        content: "We chose Swift for Project Apollo."
      )
    )
    try await builder.upsert(
      Record(
        id: "launch-checklist",
        type: "Note",
        content: "Project Apollo launch checklist."
      )
    )
    let database = try await builder.build()
    let selection = try await database.query(
      from: [GraphNodeID(nodeType: "Project", recordID: "apollo")],
      traversing: [GraphTraversal(relationship: "contains")]
    )
    print("graph-scope=\(selection.matches.map(\.nodeID.recordID).joined(separator: ","))")
  }
}
