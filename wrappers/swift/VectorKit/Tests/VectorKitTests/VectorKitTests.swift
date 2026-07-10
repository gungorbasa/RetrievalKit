import XCTest
@testable import VectorKit

final class VectorKitTests: XCTestCase {
    private struct RealDataQuery: Decodable {
        var query: String
        var dimension: Int
        var embedding: [Float]
    }

    func testFixedTextChunkerUsesRustImplementationAndPreservesUnicodeOffsets() throws {
        let chunker = try TextChunker(strategy: .fixed, maxCharacters: 4, overlapCharacters: 1)

        let chunks = try chunker.chunks(for: "abçdef")

        XCTAssertEqual(
            chunks,
            [
                TextChunk(text: "abçd", startByte: 0, endByte: 5),
                TextChunk(text: "def", startByte: 4, endByte: 7),
            ]
        )
    }

    func testSentenceTextChunkerPrefersNaturalBoundaries() throws {
        let chunker = try TextChunker(strategy: .sentence, maxCharacters: 25)

        let chunks = try chunker.chunks(for: "First sentence. Second sentence. Third.")

        XCTAssertEqual(chunks.map(\.text), ["First sentence.", "Second sentence. Third."])
    }

    func testTextChunkerRejectsInvalidConfiguration() {
        XCTAssertThrowsError(try TextChunker(maxCharacters: 0))
        XCTAssertThrowsError(try TextChunker(maxCharacters: 5, overlapCharacters: 5))
    }

    func testExactSearchReturnsIndexedChunk() async throws {
        let index = try VectorIndex(dimension: 3)

        try await index.upsert(
            document: Document(id: "doc-1", metadata: ["source": .string("notes")]),
            chunks: [
                ChunkInput(text: "alpha topic", embedding: [1, 0, 0]),
                ChunkInput(text: "beta topic", embedding: [0, 1, 0])
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
                ChunkInput(text: "skip", embedding: [1, 0], metadata: ["bucket": .integer(2)])
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
                ChunkInput(text: "remote public article", embedding: [0, 1, 0])
            ]
        )

        let keywordResults = try await index.keywordSearch(text: "private notes", topK: 1)
        XCTAssertEqual(keywordResults.first?.text, "local private notes")
        XCTAssertTrue(keywordResults.first?.matchedTerms.contains("private") == true)

        let hybridResults = try await index.hybridSearch(text: "private notes", embedding: [1, 0, 0], topK: 1)
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

        let loaded = try VectorIndex.load(from: directory)
        let results = try await loaded.search(embedding: [0, 1], topK: 1)
        let dimension = await loaded.dimension
        let activeChunkCount = await loaded.activeChunkCount

        XCTAssertEqual(dimension, 2)
        XCTAssertEqual(activeChunkCount, 1)
        XCTAssertEqual(results.first?.text, "persisted chunk")
    }

    func testDimensionMismatchMapsToCoreError() async throws {
        let index = try VectorIndex(dimension: 2)

        do {
            _ = try await index.search(embedding: [1], topK: 1)
            XCTFail("expected dimension mismatch")
        } catch {
            guard case VectorKitError.core(let message) = error else {
                return XCTFail("expected core error, got \(error)")
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
                )
            ]
        )

        let filter = Filter.all([
            .equals("source", .string("notes")),
            .range("stars", lower: .integer(4), upper: .integer(5))
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

    func testBundledSocialNetworkIndexSupportsRealSearches() async throws {
        let resources = socialNetworkResourcesURL()
        let indexURL = resources.appendingPathComponent("social-network-index")
        let queryURL = resources.appendingPathComponent("social-network-query.json")

        XCTAssertTrue(FileManager.default.fileExists(atPath: indexURL.appendingPathComponent("manifest.json").path))
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

        let hybridResults = try await index.hybridSearch(text: query.query, embedding: query.embedding, topK: 3)
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

    private func socialNetworkResourcesURL() -> URL {
        let testFile = URL(fileURLWithPath: #filePath)
        let swiftRoot = testFile
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return swiftRoot
            .appendingPathComponent("VectorKitIOSBench")
            .appendingPathComponent("VectorKitIOSBench")
            .appendingPathComponent("Resources")
    }
}
