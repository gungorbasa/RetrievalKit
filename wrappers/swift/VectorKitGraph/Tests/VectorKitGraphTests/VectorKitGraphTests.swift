import Foundation
import XCTest
@testable import VectorKitGraph

final class VectorKitGraphTests: XCTestCase {
    func testGenericSchemaBuildAndPersistence() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "swift-generic")
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "alpha", recordType: "Item", fields: ["name": .string("Alpha"), "priority": .int(7), "active": .bool(true), "related_id": .string("beta")]), chunks: [GraphChunk(key: "body", text: "searchable alpha", embedding: [1, 0])]))
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "beta", recordType: "Item", fields: ["name": .string("Beta")]), chunks: [GraphChunk(key: "body", text: "searchable beta", embedding: [0, 1])]))
        let relationship = GraphRelationshipSchema(relationshipType: "related_to", sourceNodeType: "Item", targetNodeType: "Item", sourceField: GraphFieldPath("related_id"), cardinality: .optionalOne, inverseRelationship: "related_from")
        let graph = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item", queryableFields: [GraphFieldPath("name"), GraphFieldPath("priority"), GraphFieldPath("active")])], relationships: [relationship]))
        let result = try await graph.query(nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha")])
        XCTAssertEqual(result.matches, [GraphMatch(nodeID: GraphNodeID(nodeType: "Item", recordID: "alpha"), depth: 0)])
        XCTAssertEqual(result.trace.resultCount, 1)
        let integerResult = try await graph.query(nodeType: "Item", field: GraphFieldPath("priority"), equals: [.integer(7)])
        let booleanResult = try await graph.query(nodeType: "Item", field: GraphFieldPath("active"), equals: [.boolean(true)])
        XCTAssertEqual(integerResult.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
        XCTAssertEqual(booleanResult.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
        let exact = try await graph.search([1, 0], topK: 10, in: result)
        let keyword = try await graph.keywordSearch("searchable", topK: 10, in: result)
        let hybrid = try await graph.hybridSearch(text: "searchable", embedding: [1, 0], topK: 10, in: result)
        XCTAssertEqual(exact.map(\.recordID), ["alpha"])
        XCTAssertEqual(keyword.map(\.recordID), ["alpha"])
        XCTAssertEqual(hybrid.map(\.recordID), ["alpha"])

        let traversed = try await graph.query(nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha")], traversing: [GraphTraversal(relationship: "related_to")])
        XCTAssertEqual(traversed.matches.count, 1)
        XCTAssertEqual(traversed.matches[0].nodeID, GraphNodeID(nodeType: "Item", recordID: "beta"))
        XCTAssertEqual(traversed.matches[0].path, [GraphPathEdge(relationship: "related_to", source: GraphNodeID(nodeType: "Item", recordID: "alpha"), target: GraphNodeID(nodeType: "Item", recordID: "beta"), occurrenceOrdinal: 0, provenance: GraphEdgeProvenance(schemaRuleIndex: 0, sourceRecordID: "alpha", sourceField: GraphFieldPath("related_id"), derivedInverse: false, builtIn: false))])

        let inverse = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "beta")], traversing: [GraphTraversal(relationship: "related_from")])
        XCTAssertEqual(inverse.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
        XCTAssertTrue(inverse.matches[0].path[0].provenance.derivedInverse)

        let cancellation = GraphCancellationToken(); cancellation.cancel()
        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "alpha")], cancellation: cancellation); XCTFail("expected cancellation") }
        catch { XCTAssertNotNil(error as? VectorKitGraphError) }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("vectorkit-graph-swift-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        try await graph.save(to: url)
        try GraphIndex.validate(at: url)
        _ = try GraphIndex.load(from: url)
    }

    func testBuilderIsConsumedAfterFinalization() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "consumed")
        _ = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")]))
        do { _ = try await builder.build(schema: GraphSchema(recordNodes: [])); XCTFail("expected consumed builder") }
        catch { XCTAssertEqual(error as? VectorKitGraphError, .consumedBuilder) }
    }
}
