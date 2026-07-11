import Foundation
import XCTest
@testable import VectorKitGraph

final class VectorKitGraphTests: XCTestCase {
    func testGenericSchemaBuildAndPersistence() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "swift-generic")
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "item", recordType: "Item", fields: ["name": .string("Item")]), chunks: [GraphChunk(key: "body", text: "searchable item", embedding: [1, 0])]))
        let graph = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item", queryableFields: [GraphFieldPath("name")])]))
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
