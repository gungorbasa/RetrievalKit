import Foundation
import XCTest
@testable import VectorKitGraph

final class VectorKitGraphTests: XCTestCase {
    func testGenericSchemaBuildAndPersistence() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "swift-generic")
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "alpha", recordType: "Item", fields: ["name": .string("Alpha"), "priority": .int(7), "active": .bool(true), "related_id": .string("beta")]), projectedMetadata: ["tenant": .string("a"), "rank": .integer(1)], chunks: [GraphChunk(key: "body", text: "searchable alpha", embedding: [1, 0])]))
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "beta", recordType: "Item", fields: ["name": .string("Beta")]), projectedMetadata: ["tenant": .string("b"), "rank": .integer(2)], chunks: [GraphChunk(key: "body", text: "searchable beta", embedding: [0, 1])]))
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

        let both = try await graph.query(nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha"), .string("Beta")])
        let betaFilter = GraphFilter.all([.equals("tenant", .string("b")), .range("rank", lower: .integer(2), upper: .integer(2))])
        let filteredExact = try await graph.search([1, 0], topK: 10, in: both, filter: betaFilter)
        let filteredKeyword = try await graph.keywordSearch("searchable", topK: 10, in: both, filter: betaFilter)
        let filteredHybrid = try await graph.hybridSearch(text: "searchable", embedding: [1, 0], topK: 10, in: both, filter: betaFilter)
        XCTAssertEqual(filteredExact.map(\.recordID), ["beta"])
        XCTAssertEqual(filteredKeyword.map(\.recordID), ["beta"])
        XCTAssertEqual(filteredHybrid.map(\.recordID), ["beta"])
        XCTAssertTrue(filteredExact[0].filterMatched)
        XCTAssertTrue(filteredHybrid[0].trace.filterMatched)

        let vectorOnly = GraphHybridOptions(vectorTopK: 2, keywordTopK: 2, fusion: .weightedNormalizedScore(vectorWeight: 1, keywordWeight: 0))
        let keywordOnly = GraphHybridOptions(vectorTopK: 2, keywordTopK: 2, fusion: .weightedNormalizedScore(vectorWeight: 0, keywordWeight: 1))
        let vectorPreferred = try await graph.hybridSearch(text: "beta", embedding: [1, 0], topK: 2, in: both, options: vectorOnly)
        let keywordPreferred = try await graph.hybridSearch(text: "beta", embedding: [1, 0], topK: 2, in: both, options: keywordOnly)
        XCTAssertEqual(vectorPreferred.first?.recordID, "alpha")
        XCTAssertEqual(keywordPreferred.first?.recordID, "beta")
        XCTAssertNotNil(vectorPreferred.first?.trace.normalizedVectorScore)
        XCTAssertNotNil(keywordPreferred.first?.trace.normalizedKeywordScore)

        let traversed = try await graph.query(nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha")], traversing: [GraphTraversal(relationship: "related_to")])
        XCTAssertEqual(traversed.matches.count, 1)
        XCTAssertEqual(traversed.matches[0].nodeID, GraphNodeID(nodeType: "Item", recordID: "beta"))
        XCTAssertEqual(traversed.matches[0].path, [GraphPathEdge(relationship: "related_to", source: GraphNodeID(nodeType: "Item", recordID: "alpha"), target: GraphNodeID(nodeType: "Item", recordID: "beta"), occurrenceOrdinal: 0, provenance: GraphEdgeProvenance(schemaRuleIndex: 0, sourceRecordID: "alpha", sourceField: GraphFieldPath("related_id"), derivedInverse: false, builtIn: false))])

        let inverse = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "beta")], traversing: [GraphTraversal(relationship: "related_from")])
        XCTAssertEqual(inverse.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
        XCTAssertTrue(inverse.matches[0].path[0].provenance.derivedInverse)

        let cancellation = GraphCancellationToken(); cancellation.cancel()
        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "alpha")], cancellation: cancellation); XCTFail("expected cancellation") }
        catch let error as VectorKitGraphError { guard case .cancelled = error else { return XCTFail("expected typed cancellation, got \(error)") } }

        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "alpha")], limits: GraphQueryLimits(maxResults: 0)); XCTFail("expected invalid query") }
        catch let error as VectorKitGraphError { guard case .queryLimitExceeded = error else { return XCTFail("expected typed query limit, got \(error)") } }

        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "alpha")], limits: GraphQueryLimits(maxHops: 65)); XCTFail("expected query limit error") }
        catch let error as VectorKitGraphError { guard case .queryLimitExceeded = error else { return XCTFail("expected typed query limit, got \(error)") } }

        do { _ = try await graph.search([1], topK: 1, in: result); XCTFail("expected core dimension error") }
        catch let error as VectorKitGraphError { guard case .internalError = error else { return XCTFail("expected typed internal error, got \(error)") } }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("vectorkit-graph-swift-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        try await graph.save(to: url)
        try GraphIndex.validate(at: url)
        _ = try GraphIndex.load(from: url)
        let manifestURL = url.appendingPathComponent("manifest.json")
        var manifest = try JSONSerialization.jsonObject(with: Data(contentsOf: manifestURL)) as! [String: Any]
        manifest["format_version"] = 999
        try JSONSerialization.data(withJSONObject: manifest).write(to: manifestURL)
        do { _ = try GraphIndex.load(from: url); XCTFail("expected incompatible version") }
        catch let error as VectorKitGraphError { guard case .incompatibleVersion = error else { return XCTFail("expected typed incompatible version, got \(error)") } }
    }

    func testBuilderIsConsumedAfterFinalization() async throws {
        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "consumed")
        _ = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")]))
        do { _ = try await builder.build(schema: GraphSchema(recordNodes: [])); XCTFail("expected consumed builder") }
        catch { XCTAssertEqual(error as? VectorKitGraphError, .consumedBuilder) }
    }

    func testTypedBuildAndPersistenceErrors() async throws {
        let invalidSchemaBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "invalid-schema")
        do { _ = try await invalidSchemaBuilder.build(schema: GraphSchema(recordNodes: [])); XCTFail("expected invalid schema") }
        catch let error as VectorKitGraphError { guard case .invalidSchema = error else { return XCTFail("expected typed invalid schema, got \(error)") } }
        do { _ = try await invalidSchemaBuilder.build(schema: GraphSchema(recordNodes: [])); XCTFail("expected consumed builder") }
        catch { XCTAssertEqual(error as? VectorKitGraphError, .consumedBuilder) }

        let missingTargetBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "missing-target")
        try await missingTargetBuilder.upsert(GraphRecordBatch(record: GraphRecord(id: "source", recordType: "Item", fields: ["related_id": .string("missing")]), chunks: []))
        let relationship = GraphRelationshipSchema(relationshipType: "related_to", sourceNodeType: "Item", targetNodeType: "Item", sourceField: GraphFieldPath("related_id"), cardinality: .one)
        do { _ = try await missingTargetBuilder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")], relationships: [relationship])); XCTFail("expected missing target") }
        catch let error as VectorKitGraphError { guard case .invalidIdentity = error else { return XCTFail("expected typed invalid identity, got \(error)") } }

        let missingDirectory = FileManager.default.temporaryDirectory.appendingPathComponent("vectorkit-graph-missing-\(UUID().uuidString)")
        do { _ = try GraphIndex.load(from: missingDirectory); XCTFail("expected invalid snapshot") }
        catch let error as VectorKitGraphError { guard case .corruptSnapshot = error else { return XCTFail("expected typed corrupt snapshot, got \(error)") } }
    }

    func testExplicitCloseIsIdempotentAndRejectsUseAfterClose() async throws {
        let closedBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "closed-builder")
        await closedBuilder.close(); await closedBuilder.close()
        do { try await closedBuilder.upsert(GraphRecordBatch(record: GraphRecord(id: "item", recordType: "Item"), chunks: [])); XCTFail("expected closed builder") }
        catch let error as VectorKitGraphError { guard case .graphUnavailable = error else { return XCTFail("expected graph unavailable, got \(error)") } }

        let builder = try GraphIndexBuilder(dimension: 2, corpusID: "lifecycle")
        try await builder.upsert(GraphRecordBatch(record: GraphRecord(id: "item", recordType: "Item"), chunks: [GraphChunk(key: "body", text: "lifecycle item", embedding: [1, 0])]))
        let graph = try await builder.build(schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")]))
        let result = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")])
        result.close(); result.close()
        do { _ = try await graph.search([1, 0], topK: 1, in: result); XCTFail("expected closed result") }
        catch let error as VectorKitGraphError { guard case .graphUnavailable = error else { return XCTFail("expected graph unavailable, got \(error)") } }

        let cancellation = GraphCancellationToken(); cancellation.close(); cancellation.close()
        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")], cancellation: cancellation); XCTFail("expected closed cancellation token") }
        catch let error as VectorKitGraphError { guard case .graphUnavailable = error else { return XCTFail("expected graph unavailable, got \(error)") } }

        let stressResult = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")])
        let safeOutcomes = await withTaskGroup(of: Bool.self, returning: [Bool].self) { group in
            for _ in 0..<32 {
                group.addTask {
                    do { _ = try await graph.search([1, 0], topK: 1, in: stressResult); return true }
                    catch let error as VectorKitGraphError { if case .graphUnavailable = error { return true }; return false }
                    catch { return false }
                }
            }
            group.addTask { stressResult.close(); return true }
            var values: [Bool] = []; for await value in group { values.append(value) }; return values
        }
        XCTAssertEqual(safeOutcomes.count, 33); XCTAssertTrue(safeOutcomes.allSatisfy { $0 })

        await graph.close(); await graph.close()
        do { _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")]); XCTFail("expected closed index") }
        catch let error as VectorKitGraphError { guard case .graphUnavailable = error else { return XCTFail("expected graph unavailable, got \(error)") } }
    }
}
