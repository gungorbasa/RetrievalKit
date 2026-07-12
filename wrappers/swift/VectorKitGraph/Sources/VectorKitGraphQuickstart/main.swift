import Foundation
import VectorKitGraph

private enum QuickstartError: Error { case unexpectedResult }

@main
struct VectorKitGraphQuickstart {
    static func main() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "quickstart")
        try await builder.upsert(
            GraphRecordBatch(
                record: GraphRecord(
                    id: "local-search",
                    recordType: "Topic",
                    fields: [
                        "title": .string("Local Search"),
                        "related_id": .string("graph-retrieval"),
                    ]
                ),
                projectedMetadata: ["audience": .string("developer")],
                chunks: [
                    GraphChunk(
                        key: "summary",
                        text: "fast private local search",
                        embedding: [1, 0]
                    )
                ]
            )
        )
        try await builder.upsert(
            GraphRecordBatch(
                record: GraphRecord(
                    id: "graph-retrieval",
                    recordType: "Topic",
                    fields: ["title": .string("Graph Retrieval")]
                ),
                projectedMetadata: ["audience": .string("developer")],
                chunks: [
                    GraphChunk(
                        key: "summary",
                        text: "schema driven graph retrieval",
                        embedding: [0, 1]
                    )
                ]
            )
        )

        let schema = GraphSchema(
            recordNodes: [
                GraphRecordNodeSchema(
                    recordType: "Topic",
                    nodeType: "Topic",
                    queryableFields: [GraphFieldPath("title")]
                )
            ],
            relationships: [
                GraphRelationshipSchema(
                    relationshipType: "related_to",
                    sourceNodeType: "Topic",
                    targetNodeType: "Topic",
                    sourceField: GraphFieldPath("related_id"),
                    cardinality: .optionalOne,
                    inverseRelationship: "related_from"
                )
            ]
        )
        let index = try await builder.build(schema: schema)
        let traversal = try await index.query(
            nodeType: "Topic",
            field: GraphFieldPath("title"),
            equals: [.string("Local Search")],
            traversing: [GraphTraversal(relationship: "related_to")]
        )
        let hybrid = try await index.hybridSearch(
            text: "graph retrieval",
            embedding: [0, 1],
            topK: 1,
            in: traversal,
            filter: .equals("audience", .string("developer"))
        )

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vectorkit-graph-quickstart-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        try await index.save(to: directory)
        let reopened = try GraphIndex.load(from: directory)
        let reopenedResult = try await reopened.query(
            from: [GraphNodeID(nodeType: "Topic", recordID: "graph-retrieval")]
        )

        guard traversal.matches.map(\.nodeID.recordID) == ["graph-retrieval"],
              hybrid.map(\.recordID) == ["graph-retrieval"],
              traversal.projection.sourceNodes == 1,
              traversal.projection.resolvedChunks == 1,
              reopenedResult.matches.map(\.nodeID.recordID) == ["graph-retrieval"]
        else { throw QuickstartError.unexpectedResult }

        print("matches=\(traversal.matches.map(\.nodeID.recordID).joined(separator: ","))")
        print("hybrid=\(hybrid.map(\.recordID).joined(separator: ","))")
        print("projection=\(traversal.projection.sourceNodes)/\(traversal.projection.resolvedChunks)")
        print("reloaded=\(reopenedResult.matches.map(\.nodeID.recordID).joined(separator: ","))")

        traversal.close()
        reopenedResult.close()
        await reopened.close()
        await index.close()
    }
}
