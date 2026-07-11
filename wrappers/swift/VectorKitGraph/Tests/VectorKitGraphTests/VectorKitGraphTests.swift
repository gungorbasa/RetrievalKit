import Foundation
import XCTest
@testable import VectorKitGraph

final class VectorKitGraphTests: XCTestCase {
    func testGenericSchemaBuildAndPersistence() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "swift-generic")
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "item", recordType: "Item", fields: ["name": .string("Item")]), chunks: [GraphChunk(key: "body", text: "searchable item", embedding: [1, 0])]))
        let graph = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item", queryableFields: [GraphFieldPath("name")])]))
        let result = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")])
        XCTAssertEqual(result.matches, [GraphMatch(nodeID: GraphNodeID(nodeType: "Item", recordID: "item"), depth: 0, pathLength: 0)])
        XCTAssertEqual(result.trace.resultCount, 1)
        let exact = try await graph.search([1, 0], topK: 10, in: result)
        let keyword = try await graph.keywordSearch("searchable", topK: 10, in: result)
        let hybrid = try await graph.hybridSearch(text: "searchable", embedding: [1, 0], topK: 10, in: result)
        XCTAssertEqual(exact.map(\.recordID), ["item"])
        XCTAssertEqual(keyword.map(\.recordID), ["item"])
        XCTAssertEqual(hybrid.map(\.recordID), ["item"])
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
