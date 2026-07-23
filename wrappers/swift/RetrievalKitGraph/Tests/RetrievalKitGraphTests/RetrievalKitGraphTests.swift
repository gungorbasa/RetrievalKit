import Foundation
import XCTest

@testable import RetrievalKitGraph

final class RetrievalKitGraphTests: XCTestCase {
  func testGenericSchemaBuildAndPersistence() async throws {
    let builder = try GraphIndexBuilder(dimension: 2, corpusID: "swift-generic")
    try await builder.upsert(
      GraphRecordBatch(
        record: GraphRecord(
          id: "alpha", recordType: "Item",
          fields: [
            "name": .string("Alpha"), "priority": .int(7), "active": .bool(true),
            "related_id": .string("beta"),
          ]), projectedMetadata: ["tenant": .string("a"), "rank": .integer(1)],
        chunks: [GraphChunk(key: "body", text: "searchable alpha", embedding: [1, 0])]))
    try await builder.upsert(
      GraphRecordBatch(
        record: GraphRecord(id: "beta", recordType: "Item", fields: ["name": .string("Beta")]),
        projectedMetadata: ["tenant": .string("b"), "rank": .integer(2)],
        chunks: [GraphChunk(key: "body", text: "searchable beta", embedding: [0, 1])]))
    let relationship = GraphRelationshipSchema(
      relationshipType: "related_to", sourceNodeType: "Item", targetNodeType: "Item",
      sourceField: GraphFieldPath("related_id"), cardinality: .optionalOne,
      inverseRelationship: "related_from")
    let graph = try await builder.build(
      schema: GraphSchema(
        recordNodes: [
          GraphRecordNodeSchema(
            recordType: "Item", nodeType: "Item",
            queryableFields: [
              GraphFieldPath("name"), GraphFieldPath("priority"), GraphFieldPath("active"),
            ])
        ], relationships: [relationship]))
    let result = try await graph.query(
      nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha")])
    XCTAssertEqual(
      result.matches,
      [GraphMatch(nodeID: GraphNodeID(nodeType: "Item", recordID: "alpha"), depth: 0)])
    XCTAssertEqual(result.trace.resultCount, 1)
    XCTAssertNil(result.trace.truncationReason)
    XCTAssertEqual(result.projection, GraphProjectionTrace(sourceNodes: 1, resolvedChunks: 1))
    let integerResult = try await graph.query(
      nodeType: "Item", field: GraphFieldPath("priority"), equals: [.integer(7)])
    let booleanResult = try await graph.query(
      nodeType: "Item", field: GraphFieldPath("active"), equals: [.boolean(true)])
    XCTAssertEqual(
      integerResult.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
    XCTAssertEqual(
      booleanResult.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
    let exact = try await graph.search([1, 0], topK: 10, in: result)
    let keyword = try await graph.keywordSearch("searchable", topK: 10, in: result)
    let hybrid = try await graph.hybridSearch(
      text: "searchable", embedding: [1, 0], topK: 10, in: result)
    XCTAssertEqual(exact.map(\.recordID), ["alpha"])
    XCTAssertEqual(keyword.map(\.recordID), ["alpha"])
    XCTAssertEqual(hybrid.map(\.recordID), ["alpha"])

    let both = try await graph.query(
      nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha"), .string("Beta")])
    XCTAssertEqual(both.projection, GraphProjectionTrace(sourceNodes: 2, resolvedChunks: 2))
    let truncated = try await graph.query(
      from: [
        GraphNodeID(nodeType: "Item", recordID: "alpha"),
        GraphNodeID(nodeType: "Item", recordID: "beta"),
      ], limits: GraphQueryLimits(maxResults: 1))
    XCTAssertEqual(truncated.trace.truncationReason, .maxResults)
    let betaFilter = GraphFilter.all([
      .equals("tenant", .string("b")), .range("rank", lower: .integer(2), upper: .integer(2)),
    ])
    let filteredExact = try await graph.search([1, 0], topK: 10, in: both, filter: betaFilter)
    let filteredKeyword = try await graph.keywordSearch(
      "searchable", topK: 10, in: both, filter: betaFilter)
    let filteredHybrid = try await graph.hybridSearch(
      text: "searchable", embedding: [1, 0], topK: 10, in: both, filter: betaFilter)
    XCTAssertEqual(filteredExact.map(\.recordID), ["beta"])
    XCTAssertEqual(filteredKeyword.map(\.recordID), ["beta"])
    XCTAssertEqual(filteredHybrid.map(\.recordID), ["beta"])
    XCTAssertTrue(filteredExact[0].filterMatched)
    XCTAssertTrue(filteredHybrid[0].trace.filterMatched)

    let candidates = GraphHybridOptions(vectorTopK: 2, keywordTopK: 2)
    let vectorPreferred = try await graph.hybridSearch(
      text: "beta", embedding: [1, 0], topK: 2, in: both, alpha: 1, options: candidates)
    let keywordPreferred = try await graph.hybridSearch(
      text: "beta", embedding: [1, 0], topK: 2, in: both, alpha: 0, options: candidates)
    XCTAssertEqual(vectorPreferred.first?.recordID, "alpha")
    XCTAssertEqual(keywordPreferred.first?.recordID, "beta")
    XCTAssertNotNil(vectorPreferred.first?.trace.normalizedVectorScore)
    XCTAssertNotNil(keywordPreferred.first?.trace.normalizedKeywordScore)

    let traversed = try await graph.query(
      nodeType: "Item", field: GraphFieldPath("name"), equals: [.string("Alpha")],
      traversing: [GraphTraversal(relationship: "related_to")])
    XCTAssertEqual(traversed.matches.count, 1)
    XCTAssertEqual(traversed.matches[0].nodeID, GraphNodeID(nodeType: "Item", recordID: "beta"))
    XCTAssertEqual(
      traversed.matches[0].path,
      [
        GraphPathEdge(
          relationship: "related_to", source: GraphNodeID(nodeType: "Item", recordID: "alpha"),
          target: GraphNodeID(nodeType: "Item", recordID: "beta"), occurrenceOrdinal: 0,
          provenance: GraphEdgeProvenance(
            schemaRuleIndex: 0, sourceRecordID: "alpha", sourceField: GraphFieldPath("related_id"),
            derivedInverse: false, builtIn: false))
      ])

    let inverse = try await graph.query(
      from: [GraphNodeID(nodeType: "Item", recordID: "beta")],
      traversing: [GraphTraversal(relationship: "related_from")])
    XCTAssertEqual(
      inverse.matches.map(\.nodeID), [GraphNodeID(nodeType: "Item", recordID: "alpha")])
    XCTAssertTrue(inverse.matches[0].path[0].provenance.derivedInverse)

    let cancellation = GraphCancellationToken()
    cancellation.cancel()
    do {
      _ = try await graph.query(
        from: [GraphNodeID(nodeType: "Item", recordID: "alpha")], cancellation: cancellation)
      XCTFail("expected cancellation")
    } catch let error as RetrievalKitGraphError {
      guard case .cancelled = error else {
        return XCTFail("expected typed cancellation, got \(error)")
      }
    }

    do {
      _ = try await graph.query(
        from: [GraphNodeID(nodeType: "Item", recordID: "alpha")],
        limits: GraphQueryLimits(maxResults: 0))
      XCTFail("expected invalid query")
    } catch let error as RetrievalKitGraphError {
      guard case .queryLimitExceeded = error else {
        return XCTFail("expected typed query limit, got \(error)")
      }
    }

    do {
      _ = try await graph.query(
        from: [GraphNodeID(nodeType: "Item", recordID: "alpha")],
        limits: GraphQueryLimits(maxHops: 65))
      XCTFail("expected query limit error")
    } catch let error as RetrievalKitGraphError {
      guard case .queryLimitExceeded = error else {
        return XCTFail("expected typed query limit, got \(error)")
      }
    }

    do {
      _ = try await graph.search([1], topK: 1, in: result)
      XCTFail("expected dimension error")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidDimension = error else {
        return XCTFail("expected typed dimension error, got \(error)")
      }
    }
    let url = FileManager.default.temporaryDirectory.appendingPathComponent(
      "retrievalkit-graph-swift-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: url) }
    try await graph.save(to: url)
    try GraphIndex.validate(at: url)
    _ = try GraphIndex.load(from: url)
    let manifestURL = url.appendingPathComponent("manifest.json")
    var manifest =
      try JSONSerialization.jsonObject(with: Data(contentsOf: manifestURL)) as! [String: Any]
    manifest["format_version"] = 999
    try JSONSerialization.data(withJSONObject: manifest).write(to: manifestURL)
    do {
      _ = try GraphIndex.load(from: url)
      XCTFail("expected incompatible version")
    } catch let error as RetrievalKitGraphError {
      guard case .incompatibleVersion = error else {
        return XCTFail("expected typed incompatible version, got \(error)")
      }
    }
  }

  func testBuilderIsConsumedAfterFinalization() async throws {
    let builder = try GraphIndexBuilder(dimension: 2, corpusID: "consumed")
    _ = try await builder.build(
      schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")]
      ))
    do {
      _ = try await builder.build(schema: GraphSchema(recordNodes: []))
      XCTFail("expected consumed builder")
    } catch { XCTAssertEqual(error as? RetrievalKitGraphError, .consumedBuilder) }
  }

  func testTypedBuildAndPersistenceErrors() async throws {
    do {
      _ = try GraphIndexBuilder(dimension: -1, corpusID: "negative")
      XCTFail("expected negative dimension rejection")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidIdentity = error else {
        return XCTFail("expected invalid identity, got \(error)")
      }
    }
    let invalidSchemaBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "invalid-schema")
    do {
      _ = try await invalidSchemaBuilder.build(schema: GraphSchema(recordNodes: []))
      XCTFail("expected invalid schema")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidSchema = error else {
        return XCTFail("expected typed invalid schema, got \(error)")
      }
    }
    do {
      _ = try await invalidSchemaBuilder.build(schema: GraphSchema(recordNodes: []))
      XCTFail("expected consumed builder")
    } catch { XCTAssertEqual(error as? RetrievalKitGraphError, .consumedBuilder) }

    let missingTargetBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "missing-target")
    try await missingTargetBuilder.upsert(
      GraphRecordBatch(
        record: GraphRecord(
          id: "source", recordType: "Item", fields: ["related_id": .string("missing")]), chunks: [])
    )
    let relationship = GraphRelationshipSchema(
      relationshipType: "related_to", sourceNodeType: "Item", targetNodeType: "Item",
      sourceField: GraphFieldPath("related_id"), cardinality: .one)
    do {
      _ = try await missingTargetBuilder.build(
        schema: GraphSchema(
          recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")],
          relationships: [relationship]))
      XCTFail("expected missing target")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidIdentity = error else {
        return XCTFail("expected typed invalid identity, got \(error)")
      }
    }

    let missingDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(
      "retrievalkit-graph-missing-\(UUID().uuidString)")
    do {
      _ = try GraphIndex.load(from: missingDirectory)
      XCTFail("expected invalid snapshot")
    } catch let error as RetrievalKitGraphError {
      guard case .corruptSnapshot = error else {
        return XCTFail("expected typed corrupt snapshot, got \(error)")
      }
    }
  }

  func testExplicitCloseIsIdempotentAndRejectsUseAfterClose() async throws {
    let closedBuilder = try GraphIndexBuilder(dimension: 2, corpusID: "closed-builder")
    await closedBuilder.close()
    await closedBuilder.close()
    do {
      try await closedBuilder.upsert(
        GraphRecordBatch(record: GraphRecord(id: "item", recordType: "Item"), chunks: []))
      XCTFail("expected closed builder")
    } catch let error as RetrievalKitGraphError {
      guard case .graphUnavailable = error else {
        return XCTFail("expected graph unavailable, got \(error)")
      }
    }

    let builder = try GraphIndexBuilder(dimension: 2, corpusID: "lifecycle")
    try await builder.upsert(
      GraphRecordBatch(
        record: GraphRecord(id: "item", recordType: "Item"),
        chunks: [GraphChunk(key: "body", text: "lifecycle item", embedding: [1, 0])]))
    let graph = try await builder.build(
      schema: GraphSchema(recordNodes: [GraphRecordNodeSchema(recordType: "Item", nodeType: "Item")]
      ))
    let result = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")])
    do {
      _ = try await graph.search([1, 0], topK: -1, in: result)
      XCTFail("expected negative topK rejection")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidIdentity = error else {
        return XCTFail("expected invalid identity, got \(error)")
      }
    }
    do {
      _ = try await graph.query(
        from: [GraphNodeID(nodeType: "Item", recordID: "item")],
        traversing: [GraphTraversal(relationship: "owns", minHops: -1)])
      XCTFail("expected negative hop rejection")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidIdentity = error else {
        return XCTFail("expected invalid identity, got \(error)")
      }
    }
    do {
      _ = try await graph.hybridSearch(
        text: "item", embedding: [1, 0], topK: 1, in: result,
        options: GraphHybridOptions(vectorTopK: -1))
      XCTFail("expected negative candidate rejection")
    } catch let error as RetrievalKitGraphError {
      guard case .invalidIdentity = error else {
        return XCTFail("expected invalid identity, got \(error)")
      }
    }
    result.close()
    result.close()
    do {
      _ = try await graph.search([1, 0], topK: 1, in: result)
      XCTFail("expected closed result")
    } catch let error as RetrievalKitGraphError {
      guard case .graphUnavailable = error else {
        return XCTFail("expected graph unavailable, got \(error)")
      }
    }

    let cancellation = GraphCancellationToken()
    cancellation.close()
    cancellation.close()
    do {
      _ = try await graph.query(
        from: [GraphNodeID(nodeType: "Item", recordID: "item")], cancellation: cancellation)
      XCTFail("expected closed cancellation token")
    } catch let error as RetrievalKitGraphError {
      guard case .graphUnavailable = error else {
        return XCTFail("expected graph unavailable, got \(error)")
      }
    }

    let stressResult = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")]
    )
    let safeOutcomes = await withTaskGroup(of: Bool.self, returning: [Bool].self) { group in
      for _ in 0..<32 {
        group.addTask {
          do {
            _ = try await graph.search([1, 0], topK: 1, in: stressResult)
            return true
          } catch let error as RetrievalKitGraphError {
            if case .graphUnavailable = error { return true }
            return false
          } catch { return false }
        }
      }
      group.addTask {
        stressResult.close()
        return true
      }
      var values: [Bool] = []
      for await value in group { values.append(value) }
      return values
    }
    XCTAssertEqual(safeOutcomes.count, 33)
    XCTAssertTrue(safeOutcomes.allSatisfy { $0 })

    await graph.close()
    await graph.close()
    do {
      _ = try await graph.query(from: [GraphNodeID(nodeType: "Item", recordID: "item")])
      XCTFail("expected closed index")
    } catch let error as RetrievalKitGraphError {
      guard case .graphUnavailable = error else {
        return XCTFail("expected graph unavailable, got \(error)")
      }
    }
  }

  func testGraphReadWriteGateRunsReadsConcurrently() async throws {
    let gate = GraphReadWriteGate()
    let release = GraphTestLatch()
    let started = expectation(description: "both graph readers started")
    started.expectedFulfillmentCount = 2
    async let first: Int = gate.withRead {
      started.fulfill()
      await release.wait()
      return 1
    }
    async let second: Int = gate.withRead {
      started.fulfill()
      await release.wait()
      return 2
    }
    await fulfillment(of: [started], timeout: 1)
    await release.open()
    let values = await [first, second]
    XCTAssertEqual(values, [1, 2])
  }

  func testGraphReadWriteGatePrefersExclusiveWork() async throws {
    let gate = GraphReadWriteGate()
    let release = GraphTestLatch()
    let events = GraphEventRecorder()
    let readerStarted = expectation(description: "reader started")
    let writerAttempted = expectation(description: "writer attempted")
    let writerStarted = expectation(description: "writer started")
    let laterReaderAttempted = expectation(description: "later reader attempted")
    let laterReaderStarted = expectation(description: "later reader started")
    let reader = Task {
      await gate.withRead {
        await events.append("reader-start")
        readerStarted.fulfill()
        await release.wait()
        await events.append("reader-end")
      }
    }
    await fulfillment(of: [readerStarted], timeout: 1)
    let writer = Task {
      writerAttempted.fulfill()
      await gate.withWrite {
        await events.append("writer")
        writerStarted.fulfill()
      }
    }
    await fulfillment(of: [writerAttempted], timeout: 1)
    let laterReader = Task {
      laterReaderAttempted.fulfill()
      await gate.withRead {
        await events.append("later-reader")
        laterReaderStarted.fulfill()
      }
    }
    await fulfillment(of: [laterReaderAttempted], timeout: 1)
    try await Task.sleep(for: .milliseconds(25))
    let blockedEvents = await events.values()
    XCTAssertEqual(blockedEvents, ["reader-start"])
    await release.open()
    await fulfillment(of: [writerStarted, laterReaderStarted], timeout: 1)
    await reader.value
    await writer.value
    await laterReader.value
    let finalEvents = await events.values()
    XCTAssertEqual(finalEvents, ["reader-start", "reader-end", "writer", "later-reader"])
  }

  func testSwiftMatchesGenericCrossWrapperFixture() async throws {
    let fixture = try JSONDecoder().decode(
      GraphConformanceFixture.self, from: Data(contentsOf: graphConformanceFixtureURL()))
    XCTAssertEqual(fixture.schemaVersion, 1)
    XCTAssertEqual(fixture.fixtureID, "generic-topics-v1")
    let builder = try GraphIndexBuilder(
      dimension: fixture.dimension, corpusID: fixture.corpusID, metric: .dotProduct)
    for record in fixture.records { try await builder.upsert(record) }
    let graph = try await builder.build(schema: fixture.schema)

    let equality = fixture.expectations.equality
    let equalityResult = try await graph.query(
      nodeType: equality.nodeType, field: equality.field, equals: [.string(equality.value)])
    XCTAssertEqual(equalityResult.matches.map(\.nodeID.recordID), equality.nodeIDs)
    XCTAssertEqual(equalityResult.projection.sourceNodes, equality.sourceNodes)
    XCTAssertEqual(equalityResult.projection.resolvedChunks, equality.resolvedChunks)

    let traversal = fixture.expectations.traversal
    let traversalResult = try await graph.query(
      from: [GraphNodeID(nodeType: "Topic", recordID: traversal.seedRecordID)],
      traversing: [
        GraphTraversal(
          relationship: traversal.relationship, minHops: traversal.minHops,
          maxHops: traversal.maxHops)
      ])
    XCTAssertEqual(traversalResult.matches.map(\.nodeID.recordID), traversal.nodeIDs)
    XCTAssertEqual(traversalResult.matches.map { $0.path.map(\.relationship) }, traversal.paths)
    XCTAssertEqual(traversalResult.projection.sourceNodes, traversal.sourceNodes)
    XCTAssertEqual(traversalResult.projection.resolvedChunks, traversal.resolvedChunks)

    let filtered = fixture.expectations.filteredExact
    let all = try await graph.query(
      nodeType: "Topic", field: GraphFieldPath("title"),
      equals: filtered.seedTitles.map(GraphScalar.string))
    let exact = try await graph.search(
      filtered.embedding, topK: 10, in: all,
      filter: .equals(filtered.filterField, .string(filtered.filterValue)))
    XCTAssertEqual(exact.map(\.recordID), filtered.recordIDs)
    let keyword = try await graph.keywordSearch(
      fixture.expectations.keyword.text, topK: 10, in: all)
    XCTAssertEqual(keyword.map(\.recordID), fixture.expectations.keyword.recordIDs)
    await graph.close()
  }

  func testGraphOnlyDatabaseNeedsNoRetrievalConfigurationOrEmbeddings() async throws {
    let schema = capabilitySchema()
    let builder = try GraphDatabase.Builder(corpusID: "knowledge", schema: schema)
    try await builder.upsert(capabilityInput(id: "rust", title: "Rust", text: "native retrieval"))
    try await builder.upsert(
      capabilityInput(id: "swift", title: "Swift", text: "native application code"))
    let database = try await builder.build()

    let selection = try await database.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: [.string("Swift"), .string("Rust")]
    )
    XCTAssertEqual(selection.matches.map(\.nodeID.recordID), ["rust", "swift"])
    let allCandidates = try await database.projectCandidates(from: selection)
    XCTAssertEqual(
      allCandidates.candidates,
      [
        GraphChunkIdentity(recordID: "rust", chunkKey: "summary"),
        GraphChunkIdentity(recordID: "swift", chunkKey: "summary"),
      ])
    XCTAssertEqual(allCandidates.sourceNodes, 2)
    XCTAssertEqual(allCandidates.projectedChunksBeforeFilter, 2)
    XCTAssertEqual(allCandidates.projectedChunksAfterFilter, 2)
    let mobileCandidates = try await database.projectCandidates(
      from: selection, filter: .equals("team", .string("mobile")))
    XCTAssertEqual(
      mobileCandidates.candidates,
      [GraphChunkIdentity(recordID: "rust", chunkKey: "summary")])
    XCTAssertEqual(mobileCandidates.projectedChunksBeforeFilter, 2)
    XCTAssertEqual(mobileCandidates.projectedChunksAfterFilter, 1)

    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("retrievalkit-graph-only-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: directory) }
    try await database.save(to: directory)
    try GraphDatabase.validate(at: directory)
    XCTAssertFalse(
      FileManager.default.fileExists(atPath: directory.appendingPathComponent("retrieval").path))

    let reopened = try GraphDatabase.load(from: directory)
    let reopenedSelection = try await reopened.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )
    XCTAssertEqual(reopenedSelection.matches.map(\.nodeID.recordID), ["rust"])
    let reopenedCandidates = try await reopened.projectCandidates(from: reopenedSelection)
    XCTAssertEqual(
      reopenedCandidates.candidates,
      [GraphChunkIdentity(recordID: "rust", chunkKey: "summary")])
  }

  func testCombinedDatabaseScopesHybridRetrievalWithoutCopyingRecords() async throws {
    let builder = try GraphRetrievalDatabase.Builder(
      corpusID: "knowledge",
      graph: capabilitySchema(),
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32)
      )
    )
    try await builder.upsert(
      capabilityInput(id: "rust", title: "Rust", text: "native retrieval"),
      embeddings: ["summary": [1, 0]]
    )
    try await builder.upsert(
      capabilityInput(id: "swift", title: "Swift", text: "native application code"),
      embeddings: ["summary": [0, 1]]
    )
    let database = try await builder.build()
    let rustOnly = try await database.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )

    let scoped = try await database.retrieval.hybridSearch(
      text: "native",
      embedding: [0, 1],
      within: rustOnly
    )
    XCTAssertEqual(scoped.map(\.recordID), ["rust"])
    let projected = try await database.projectCandidates(from: rustOnly)
    XCTAssertEqual(
      projected.candidates,
      [GraphChunkIdentity(recordID: "rust", chunkKey: "summary")])

    let unscoped = try await database.retrieval.semanticSearch(embedding: [0, 1])
    XCTAssertEqual(unscoped.first?.recordID, "swift")

    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("retrievalkit-combined-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: directory) }
    try await database.save(to: directory)
    try GraphRetrievalDatabase.validate(at: directory)
    let reopened = try GraphRetrievalDatabase.load(from: directory)
    let reopenedSelection = try await reopened.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )
    let reopenedHits = try await reopened.retrieval.hybridSearch(
      text: "native",
      embedding: [0, 1],
      within: reopenedSelection
    )
    XCTAssertEqual(reopenedHits.map(\.recordID), ["rust"])
  }

  func testCombinedDatabaseSupportsHybridQueriesDirectly() async throws {
    let builder = try GraphRetrievalDatabase.Builder(
      corpusID: "semantic-graph",
      graph: capabilitySchema(),
      retrieval: .init(semantic: .init(dimension: 2, encoding: .f32))
    )
    try await builder.upsert(
      capabilityInput(id: "rust", title: "Rust", text: "native retrieval"),
      embeddings: ["summary": [1, 0]]
    )
    let database = try await builder.build()

    let hybrid = try await database.retrieval.hybridSearch(
      text: "native",
      embedding: [1, 0]
    )
    XCTAssertEqual(hybrid.map(\.recordID), ["rust"])
  }

  func testCombinedDatabaseRejectsSelectionFromAnotherCorpus() async throws {
    let graphBuilder = try GraphDatabase.Builder(
      corpusID: "source-corpus",
      schema: capabilitySchema()
    )
    try await graphBuilder.upsert(
      capabilityInput(id: "rust", title: "Rust", text: "native retrieval")
    )
    let graphDatabase = try await graphBuilder.build()
    let foreignSelection = try await graphDatabase.graph.query(
      nodeType: "Topic",
      field: "title",
      equals: .string("Rust")
    )

    let combinedBuilder = try GraphRetrievalDatabase.Builder(
      corpusID: "target-corpus",
      graph: capabilitySchema(),
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32)
      )
    )
    try await combinedBuilder.upsert(
      capabilityInput(id: "rust", title: "Rust", text: "native retrieval"),
      embeddings: ["summary": [1, 0]]
    )
    let combined = try await combinedBuilder.build()

    do {
      _ = try await combined.retrieval.semanticSearch(
        embedding: [1, 0],
        within: foreignSelection
      )
      XCTFail("a selection from another corpus must be rejected")
    } catch RetrievalKitGraphError.staleGeneration(let message) {
      XCTAssertTrue(message.contains("source-corpus"))
      XCTAssertTrue(message.contains("target-corpus"))
    }
    do {
      _ = try await combined.projectCandidates(from: foreignSelection)
      XCTFail("a projection from another corpus must be rejected")
    } catch RetrievalKitGraphError.staleGeneration(let message) {
      XCTAssertTrue(message.contains("source-corpus"))
      XCTAssertTrue(message.contains("target-corpus"))
    }
  }

  private func capabilitySchema() -> GraphSchema {
    GraphSchema(recordNodes: [
      GraphRecordNodeSchema(
        recordType: "Topic",
        nodeType: "Topic",
        queryableFields: ["title"]
      )
    ])
  }

  private func capabilityInput(id: RecordID, title: String, text: String) -> RecordInput {
    RecordInput(
      record: Record(
        id: id,
        type: "Topic",
        fields: ["title": .string(title)],
        metadata: ["team": .string(id == "rust" ? "mobile" : "platform")]
      ),
      chunks: [Chunk(key: "summary", text: text)]
    )
  }

  private func graphConformanceFixtureURL() -> URL {
    var root = URL(fileURLWithPath: #filePath)
    for _ in 0..<6 { root.deleteLastPathComponent() }
    return root.appendingPathComponent("benchmarks/graph-conformance/v1/fixture.json")
  }
}

private actor GraphTestLatch {
  private var isOpen = false
  private var waiters: [CheckedContinuation<Void, Never>] = []
  func wait() async {
    guard !isOpen else { return }
    await withCheckedContinuation { waiters.append($0) }
  }
  func open() {
    isOpen = true
    let pending = waiters
    waiters.removeAll()
    pending.forEach { $0.resume() }
  }
}

private actor GraphEventRecorder {
  private var events: [String] = []
  func append(_ event: String) { events.append(event) }
  func values() -> [String] { events }
}

private struct GraphConformanceFixture: Decodable {
  let schemaVersion: Int
  let fixtureID: String
  let dimension: Int
  let corpusID: String
  let records: [GraphRecordBatch]
  let schema: GraphSchema
  let expectations: GraphFixtureExpectations
  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case fixtureID = "fixture_id"
    case dimension
    case corpusID = "corpus_id"
    case records, schema, expectations
  }
}

private struct GraphFixtureExpectations: Decodable {
  let equality: GraphEqualityExpectation
  let traversal: GraphTraversalExpectation
  let filteredExact: GraphFilteredExactExpectation
  let keyword: GraphKeywordExpectation
  enum CodingKeys: String, CodingKey {
    case equality, traversal
    case filteredExact = "filtered_exact"
    case keyword
  }
}

private struct GraphEqualityExpectation: Decodable {
  let nodeType: String
  let field: GraphFieldPath
  let value: String
  let nodeIDs: [String]
  let sourceNodes, resolvedChunks: Int
  enum CodingKeys: String, CodingKey {
    case nodeType = "node_type"
    case field, value
    case nodeIDs = "node_ids"
    case sourceNodes = "source_nodes"
    case resolvedChunks = "resolved_chunks"
  }
}

private struct GraphTraversalExpectation: Decodable {
  let seedRecordID, relationship: String
  let minHops, maxHops: Int
  let nodeIDs: [String]
  let paths: [[String]]
  let sourceNodes, resolvedChunks: Int
  enum CodingKeys: String, CodingKey {
    case seedRecordID = "seed_record_id"
    case relationship
    case minHops = "min_hops"
    case maxHops = "max_hops"
    case nodeIDs = "node_ids"
    case paths
    case sourceNodes = "source_nodes"
    case resolvedChunks = "resolved_chunks"
  }
}

private struct GraphFilteredExactExpectation: Decodable {
  let seedTitles: [String]
  let embedding: [Float]
  let filterField, filterValue: String
  let recordIDs: [String]
  enum CodingKeys: String, CodingKey {
    case seedTitles = "seed_titles"
    case embedding
    case filterField = "filter_field"
    case filterValue = "filter_value"
    case recordIDs = "record_ids"
  }
}

private struct GraphKeywordExpectation: Decodable {
  let text: String
  let recordIDs: [String]
  enum CodingKeys: String, CodingKey {
    case text
    case recordIDs = "record_ids"
  }
}
