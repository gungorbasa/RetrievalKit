import XCTest
@testable import VectorKit

final class VectorKitTests: XCTestCase {
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
}
