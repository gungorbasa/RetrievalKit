import XCTest
@testable import VectorKit

final class VectorKitTests: XCTestCase {
    func testExactSearchReturnsIndexedChunk() throws {
        let index = try VectorIndex(dimension: 3)

        try index.upsert(
            document: Document(id: "doc-1", metadata: ["source": .string("notes")]),
            chunks: [
                ChunkInput(text: "alpha topic", embedding: [1, 0, 0]),
                ChunkInput(text: "beta topic", embedding: [0, 1, 0])
            ]
        )

        let results = try index.search(embedding: [1, 0, 0], topK: 1)

        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results[0].documentID, "doc-1")
        XCTAssertEqual(results[0].text, "alpha topic")
    }

    func testMetadataFilterRestrictsSearchResults() throws {
        let index = try VectorIndex(dimension: 2)

        try index.upsert(
            document: Document(id: "doc-1"),
            chunks: [
                ChunkInput(text: "keep", embedding: [1, 0], metadata: ["bucket": .integer(1)]),
                ChunkInput(text: "skip", embedding: [1, 0], metadata: ["bucket": .integer(2)])
            ]
        )

        let filter = try Filter.equals("bucket", .integer(2))
        let results = try index.search(embedding: [1, 0], topK: 2, filter: filter)

        XCTAssertEqual(results.map(\.text), ["skip"])
    }

    func testKeywordAndHybridSearchReturnTextAndTrace() throws {
        let index = try VectorIndex(dimension: 3)

        try index.upsert(
            document: Document(id: "doc-1"),
            chunks: [
                ChunkInput(text: "local private notes", embedding: [1, 0, 0]),
                ChunkInput(text: "remote public article", embedding: [0, 1, 0])
            ]
        )

        let keywordResults = try index.keywordSearch(text: "private notes", topK: 1)
        XCTAssertEqual(keywordResults.first?.text, "local private notes")
        XCTAssertTrue(keywordResults.first?.matchedTerms.contains("private") == true)

        let hybridResults = try index.hybridSearch(text: "private notes", embedding: [1, 0, 0], topK: 1)
        XCTAssertEqual(hybridResults.first?.text, "local private notes")
        XCTAssertNotNil(hybridResults.first?.vectorScore)
        XCTAssertNotNil(hybridResults.first?.keywordScore)
        XCTAssertEqual(hybridResults.first?.trace.filterMatched, true)
    }

    func testDeleteRemovesDocumentFromResults() throws {
        let index = try VectorIndex(dimension: 2)

        try index.upsert(
            document: Document(id: "doc-1"),
            chunks: [ChunkInput(text: "delete me", embedding: [1, 0])]
        )

        XCTAssertEqual(try index.deleteDocument(id: "doc-1"), 1)
        XCTAssertTrue(try index.search(embedding: [1, 0], topK: 1).isEmpty)
    }

    func testSaveAndLoadRoundTrip() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vectorkit-swift-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }

        let index = try VectorIndex(dimension: 2)
        try index.upsert(
            document: Document(id: "doc-1"),
            chunks: [ChunkInput(text: "persisted chunk", embedding: [0, 1])]
        )
        try index.save(to: directory)

        let loaded = try VectorIndex.load(from: directory)
        let results = try loaded.search(embedding: [0, 1], topK: 1)

        XCTAssertEqual(loaded.dimension, 2)
        XCTAssertEqual(loaded.activeChunkCount, 1)
        XCTAssertEqual(results.first?.text, "persisted chunk")
    }
}
