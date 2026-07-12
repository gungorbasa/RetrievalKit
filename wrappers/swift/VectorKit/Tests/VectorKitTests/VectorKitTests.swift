import XCTest

@testable import VectorKit

final class VectorKitTests: XCTestCase {
  private struct RealDataQuery: Decodable {
    var query: String
    var dimension: Int
    var embedding: [Float]
  }

  func testProductionHybridDefaultsMatchQualityBenchmark() {
    XCTAssertEqual(
      HybridOptions.default,
      HybridOptions(
        vectorTopK: 50,
        keywordTopK: 50,
        fusion: .reciprocalRank(rrfK: 60)
      )
    )
  }

  func testExactSearchReturnsIndexedChunk() async throws {
    let index = try VectorIndex(dimension: 3)

    try await index.upsert(
      document: Document(id: "doc-1", metadata: ["source": .string("notes")]),
      chunks: [
        ChunkInput(text: "alpha topic", embedding: [1, 0, 0]),
        ChunkInput(text: "beta topic", embedding: [0, 1, 0]),
      ]
    )

    let results = try await index.search(embedding: [1, 0, 0], topK: 1)

    XCTAssertEqual(results.count, 1)
    XCTAssertEqual(results[0].documentID, "doc-1")
    XCTAssertEqual(results[0].text, "alpha topic")
  }

  func testMetadataFilterRestrictsSearchResults() async throws {
    let index = try VectorIndex(dimension: 2)

    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [
        ChunkInput(text: "keep", embedding: [1, 0], metadata: ["bucket": .integer(1)]),
        ChunkInput(text: "skip", embedding: [1, 0], metadata: ["bucket": .integer(2)]),
      ]
    )

    let filter = Filter.equals("bucket", .integer(2))
    let results = try await index.search(embedding: [1, 0], topK: 2, filter: filter)

    XCTAssertEqual(results.map(\.text), ["skip"])
  }

  func testKeywordAndHybridSearchReturnTextAndTrace() async throws {
    let index = try VectorIndex(dimension: 3)

    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [
        ChunkInput(text: "local private notes", embedding: [1, 0, 0]),
        ChunkInput(text: "remote public article", embedding: [0, 1, 0]),
      ]
    )

    let keywordResults = try await index.keywordSearch(text: "private notes", topK: 1)
    XCTAssertEqual(keywordResults.first?.text, "local private notes")
    XCTAssertTrue(keywordResults.first?.matchedTerms.contains("private") == true)

    let hybridResults = try await index.hybridSearch(
      text: "private notes", embedding: [1, 0, 0], topK: 1)
    XCTAssertEqual(hybridResults.first?.text, "local private notes")
    XCTAssertNotNil(hybridResults.first?.vectorScore)
    XCTAssertNotNil(hybridResults.first?.keywordScore)
    XCTAssertEqual(hybridResults.first?.trace.filterMatched, true)
  }

  func testDeleteRemovesDocumentFromResults() async throws {
    let index = try VectorIndex(dimension: 2)

    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "delete me", embedding: [1, 0])]
    )

    let deletedCount = try await index.deleteDocument(id: "doc-1")
    let results = try await index.search(embedding: [1, 0], topK: 1)

    XCTAssertEqual(deletedCount, 1)
    XCTAssertTrue(results.isEmpty)
  }

  func testCompactionReclaimsTombstonesAndPreservesResults() async throws {
    let index = try VectorIndex(dimension: 2)
    let oldIDs = try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "old", embedding: [1, 0])]
    )
    let activeIDs = try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "current", embedding: [0, 1])]
    )
    let resultsBefore = try await index.search(embedding: [0, 1], topK: 1)
    let totalBefore = await index.totalChunkCount
    let tombstonesBefore = await index.tombstonedChunkCount

    let report = try await index.compact()
    let resultsAfter = try await index.search(embedding: [0, 1], topK: 1)
    let totalAfter = await index.totalChunkCount
    let tombstonesAfter = await index.tombstonedChunkCount

    XCTAssertEqual(totalBefore, 2)
    XCTAssertEqual(tombstonesBefore, 1)
    XCTAssertEqual(report.chunksBefore, 2)
    XCTAssertEqual(report.chunksAfter, 1)
    XCTAssertEqual(report.chunksRemoved, 1)
    XCTAssertGreaterThan(report.estimatedBytesReclaimed, 0)
    XCTAssertEqual(resultsAfter, resultsBefore)
    XCTAssertEqual(resultsAfter.first?.chunkID, activeIDs[0])
    XCTAssertNotEqual(resultsAfter.first?.chunkID, oldIDs[0])
    XCTAssertEqual(totalAfter, 1)
    XCTAssertEqual(tombstonesAfter, 0)
  }

  func testSaveAndLoadRoundTrip() async throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("vectorkit-swift-tests-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: directory) }

    let index = try VectorIndex(dimension: 2)
    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "persisted chunk", embedding: [0, 1])]
    )
    try await index.save(to: directory)

    let manifestData = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
    let manifest = try XCTUnwrap(
      JSONSerialization.jsonObject(with: manifestData) as? [String: Any]
    )
    XCTAssertEqual(manifest["vector_encoding"] as? String, "I8ScalarQuantized")

    let loaded = try VectorIndex.load(from: directory)
    let results = try await loaded.search(embedding: [0, 1], topK: 1)
    let dimension = await loaded.dimension
    let activeChunkCount = await loaded.activeChunkCount

    XCTAssertEqual(dimension, 2)
    XCTAssertEqual(activeChunkCount, 1)
    XCTAssertEqual(results.first?.text, "persisted chunk")

    try await index.upsert(
      document: Document(id: "doc-2"),
      chunks: [ChunkInput(text: "second snapshot", embedding: [1, 0])]
    )
    try await index.save(to: directory)
    let reloaded = try VectorIndex.load(from: directory)
    let reloadedChunkCount = await reloaded.activeChunkCount
    XCTAssertEqual(reloadedChunkCount, 2)
  }

  func testSaveErrorIncludesOperationCauseAndRecoveryHint() async throws {
    let blockingFile = FileManager.default.temporaryDirectory
      .appendingPathComponent("vectorkit-blocking-file-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: blockingFile) }
    try Data("file".utf8).write(to: blockingFile)

    let index = try VectorIndex(dimension: 2)
    do {
      try await index.save(to: blockingFile.appendingPathComponent("index"))
      XCTFail("expected persistence failure")
    } catch let error as VectorKitError {
      XCTAssertTrue(error.description.contains("persistence create directory failed"))
      XCTAssertTrue(error.description.contains("parent directory is writable when saving"))
    }
  }

  func testValidateDetectsCorruptPersistedPayload() async throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("vectorkit-validation-tests-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: directory) }

    let index = try VectorIndex(dimension: 2)
    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "alpha", embedding: [1, 0])]
    )
    try await index.save(to: directory)
    try VectorIndex.validate(at: directory)

    let manifestData = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
    let manifest = try XCTUnwrap(
      JSONSerialization.jsonObject(with: manifestData) as? [String: Any]
    )
    let snapshotID = try XCTUnwrap(manifest["snapshot_id"] as? String)
    let vectorsURL =
      directory
      .appendingPathComponent(".snapshots")
      .appendingPathComponent(snapshotID)
      .appendingPathComponent("vectors.vec")
    var payload = try Data(contentsOf: vectorsURL)
    payload[0] ^= 0xff
    try payload.write(to: vectorsURL)

    do {
      try VectorIndex.validate(at: directory)
      XCTFail("expected corruption failure")
    } catch {
      guard case VectorKitError.corruptIndex(let message) = error else {
        return XCTFail("expected corrupt index error, got \(error)")
      }
      XCTAssertTrue(message.contains("SHA-256 checksum mismatch"))
    }
  }

  func testDimensionMismatchMapsToTypedError() async throws {
    let index = try VectorIndex(dimension: 2)

    do {
      _ = try await index.search(embedding: [1], topK: 1)
      XCTFail("expected dimension mismatch")
    } catch {
      guard case VectorKitError.invalidDimension(let message) = error else {
        return XCTFail("expected invalid-dimension error, got \(error)")
      }
      XCTAssertTrue(message.contains("invalid vector dimension"))
    }
  }

  func testCompositeFilters() async throws {
    let index = try VectorIndex(dimension: 2)
    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [
        ChunkInput(
          text: "match",
          embedding: [1, 0],
          metadata: ["source": .string("notes"), "stars": .integer(5)]
        ),
        ChunkInput(
          text: "miss",
          embedding: [1, 0],
          metadata: ["source": .string("web"), "stars": .integer(5)]
        ),
      ]
    )

    let filter = Filter.all([
      .equals("source", .string("notes")),
      .range("stars", lower: .integer(4), upper: .integer(5)),
    ])
    let results = try await index.search(embedding: [1, 0], topK: 2, filter: filter)

    XCTAssertEqual(results.map(\.text), ["match"])
  }

  func testConcurrentReadOnlySearchesAfterIndexing() async throws {
    let index = try VectorIndex(dimension: 2)
    try await index.upsert(
      document: Document(id: "doc-1"),
      chunks: [ChunkInput(text: "concurrent", embedding: [1, 0])]
    )

    try await withThrowingTaskGroup(of: String?.self) { group in
      for _ in 0..<8 {
        group.addTask {
          try await index.search(embedding: [1, 0], topK: 1).first?.text
        }
      }
      for try await text in group {
        XCTAssertEqual(text, "concurrent")
      }
    }
  }

  func testReadWriteGateRunsReadsConcurrently() async throws {
    let gate = AsyncReadWriteGate()
    let release = AsyncTestLatch()
    let readersStarted = expectation(description: "both readers started")
    readersStarted.expectedFulfillmentCount = 2

    async let first: Int = gate.withRead {
      readersStarted.fulfill()
      await release.wait()
      return 1
    }
    async let second: Int = gate.withRead {
      readersStarted.fulfill()
      await release.wait()
      return 2
    }

    await fulfillment(of: [readersStarted], timeout: 1)
    await release.open()
    let values = await [first, second]
    XCTAssertEqual(values, [1, 2])
  }

  func testReadWriteGateKeepsMutationExclusiveAndPrefersWaitingWriter() async throws {
    let gate = AsyncReadWriteGate()
    let releaseReader = AsyncTestLatch()
    let readerStarted = expectation(description: "reader started")
    let writerAttempted = expectation(description: "writer attempted")
    let writerStarted = expectation(description: "writer started")
    let laterReaderAttempted = expectation(description: "later reader attempted")
    let laterReaderStarted = expectation(description: "later reader started")
    let events = AsyncEventRecorder()

    let reader = Task {
      await gate.withRead {
        await events.append("reader-start")
        readerStarted.fulfill()
        await releaseReader.wait()
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
    try await Task.sleep(for: .milliseconds(50))
    let writerBlockedEvents = await events.values()
    XCTAssertEqual(writerBlockedEvents, ["reader-start"])

    let laterReader = Task {
      laterReaderAttempted.fulfill()
      await gate.withRead {
        await events.append("later-reader")
        laterReaderStarted.fulfill()
      }
    }

    await fulfillment(of: [laterReaderAttempted], timeout: 1)
    try await Task.sleep(for: .milliseconds(50))
    let blockedEvents = await events.values()
    XCTAssertEqual(blockedEvents, ["reader-start"])

    await releaseReader.open()
    await fulfillment(of: [writerStarted, laterReaderStarted], timeout: 1)
    await reader.value
    await writer.value
    await laterReader.value

    let recordedEvents = await events.values()
    XCTAssertEqual(recordedEvents, ["reader-start", "reader-end", "writer", "later-reader"])
  }

  func testBundledSocialNetworkIndexSupportsRealSearches() async throws {
    let resources = socialNetworkResourcesURL()
    let indexURL = resources.appendingPathComponent("social-network-index")
    let queryURL = resources.appendingPathComponent("social-network-query.json")

    XCTAssertTrue(
      FileManager.default.fileExists(atPath: indexURL.appendingPathComponent("manifest.json").path))
    let query = try JSONDecoder().decode(RealDataQuery.self, from: Data(contentsOf: queryURL))
    XCTAssertEqual(query.dimension, 384)
    XCTAssertEqual(query.embedding.count, query.dimension)

    let index = try VectorIndex.load(from: indexURL)
    let dimension = await index.dimension
    let activeChunkCount = await index.activeChunkCount

    XCTAssertEqual(dimension, query.dimension)
    XCTAssertEqual(activeChunkCount, 28_650)

    let vectorResults = try await index.search(embedding: query.embedding, topK: 3)
    XCTAssertEqual(vectorResults.count, 3)
    XCTAssertTrue(vectorResults[0].documentID.hasPrefix("shot:"))

    let keywordResults = try await index.keywordSearch(text: query.query, topK: 3)
    XCTAssertEqual(keywordResults.count, 3)
    XCTAssertTrue(keywordResults[0].matchedTerms.contains("mark"))

    let hybridResults = try await index.hybridSearch(
      text: query.query, embedding: query.embedding, topK: 3)
    XCTAssertEqual(hybridResults.count, 3)
    XCTAssertNotNil(hybridResults[0].vectorScore)
    XCTAssertNotNil(hybridResults[0].keywordScore)

    let filteredResults = try await index.keywordSearch(
      text: "Harvard campus at night",
      topK: 3,
      filter: .equals("kind", .string("shot"))
    )
    XCTAssertEqual(filteredResults.count, 3)
    XCTAssertTrue(filteredResults.allSatisfy { $0.documentID.hasPrefix("shot:") })
  }

  func testSemanticRetrievalDatabaseRejectsHybridAndPersistsNoBM25() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "semantic-only",
      retrieval: .init(semantic: .init(dimension: 2, encoding: .f32))
    )
    try await builder.upsert(
      RecordInput(
        record: Record(id: "rust", type: "Topic"),
        chunks: [Chunk(key: "summary", text: "native retrieval")]
      ),
      embeddings: ["summary": [1, 0]]
    )
    let database = try await builder.build()

    let semantic = try await database.retrieval.semanticSearch(embedding: [1, 0])
    XCTAssertEqual(semantic.map(\.documentID), ["rust"])
    do {
      _ = try await database.retrieval.hybridSearch(
        text: "native",
        embedding: [1, 0]
      )
      XCTFail("semantic mode must not expose BM25-backed search at runtime")
    } catch VectorKitError.retrievalCapabilityUnavailable(let message) {
      XCTAssertTrue(message.contains("hybrid"))
    }

    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("vectorkit-semantic-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: directory) }
    try await database.save(to: directory)
    try RetrievalDatabase.validate(at: directory)
    XCTAssertFalse(recursiveFileNames(in: directory).contains("bm25.bin"))

    let reopened = try RetrievalDatabase.load(from: directory)
    let reopenedHits = try await reopened.retrieval.semanticSearch(embedding: [1, 0])
    XCTAssertEqual(reopenedHits.map(\.documentID), ["rust"])
  }

  func testHybridRetrievalDatabaseRequiresEveryEmbedding() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "hybrid",
      retrieval: .init(
        semantic: .init(dimension: 2, encoding: .f32),
        extras: [.hybrid]
      )
    )
    let input = RecordInput(
      record: Record(id: "rust", type: "Topic"),
      chunks: [Chunk(key: "summary", text: "native retrieval")]
    )

    do {
      try await builder.upsert(input, embeddings: [:])
      XCTFail("retrieval upsert must require one embedding per chunk")
    } catch VectorKitError.missingEmbedding(let message) {
      XCTAssertTrue(message.contains("summary"))
    }

    try await builder.upsert(input, embeddings: ["summary": [1, 0]])
    let database = try await builder.build()
    let hits = try await database.retrieval.hybridSearch(
      text: "native retrieval",
      embedding: [1, 0]
    )
    XCTAssertEqual(hits.map(\.documentID), ["rust"])
  }

  private func recursiveFileNames(in directory: URL) -> Set<String> {
    let enumerator = FileManager.default.enumerator(
      at: directory,
      includingPropertiesForKeys: nil
    )
    return Set(enumerator?.compactMap { ($0 as? URL)?.lastPathComponent } ?? [])
  }

  private func socialNetworkResourcesURL() -> URL {
    let testFile = URL(fileURLWithPath: #filePath)
    let swiftRoot =
      testFile
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    return
      swiftRoot
      .appendingPathComponent("VectorKitIOSBench")
      .appendingPathComponent("VectorKitIOSBench")
      .appendingPathComponent("Resources")
  }
}

private actor AsyncTestLatch {
  private var isOpen = false
  private var waiters: [CheckedContinuation<Void, Never>] = []

  func wait() async {
    guard !isOpen else { return }
    await withCheckedContinuation { continuation in
      waiters.append(continuation)
    }
  }

  func open() {
    isOpen = true
    let pending = waiters
    waiters.removeAll()
    pending.forEach { $0.resume() }
  }
}

private actor AsyncEventRecorder {
  private var events: [String] = []

  func append(_ event: String) {
    events.append(event)
  }

  func values() -> [String] {
    events
  }
}
