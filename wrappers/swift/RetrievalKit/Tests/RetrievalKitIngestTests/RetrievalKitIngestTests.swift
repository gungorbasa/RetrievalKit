import XCTest
@testable import RetrievalKitIngest

final class RetrievalKitIngestTests: XCTestCase {
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
}
