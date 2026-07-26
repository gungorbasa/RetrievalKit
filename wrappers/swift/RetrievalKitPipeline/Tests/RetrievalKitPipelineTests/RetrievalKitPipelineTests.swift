import EmbeddingKit
import XCTest
import RetrievalKit
@testable import RetrievalKitPipeline

final class RetrievalKitPipelineTests: XCTestCase {
    func testAddChunksEmbedsAndIndexesDocumentThenSearchesText() async throws {
        let index = try VectorIndex(dimension: 3)
        let embedder = try FakeEmbedder(
            dimension: 3,
            embeddings: [
                "alpha topic.": [1, 0, 0],
                "beta topic.": [0, 1, 0],
                "find alpha": [1, 0, 0],
            ]
        )
        let pipeline = Pipeline(
            index: index,
            embedder: embedder,
            chunker: try TextChunker(strategy: .sentence, maxCharacters: 13)
        )

        let result = try await pipeline.add(
            document: Document(id: "doc-1", text: "alpha topic. beta topic.")
        )
        let hits = try await pipeline.search("find alpha", topK: 1)

        XCTAssertEqual(result.documentID, "doc-1")
        XCTAssertEqual(result.chunkCount, 2)
        XCTAssertEqual(hits.first?.documentID, "doc-1")
        XCTAssertEqual(hits.first?.text, "alpha topic.")
    }

    func testEmbeddingFailureLeavesExistingDocumentUnchanged() async throws {
        let index = try VectorIndex(dimension: 2)
        try await index.upsert(
            document: Document(id: "doc-1"),
            chunks: [ChunkInput(text: "existing", embedding: [1, 0])]
        )
        let pipeline = Pipeline(
            index: index,
            embedder: try FailingEmbedder(dimension: 2),
            chunker: try TextChunker(maxCharacters: 20)
        )

        await XCTAssertThrowsErrorAsync {
            _ = try await pipeline.add(document: Document(id: "doc-1", text: "replacement"))
        }
        let hits = try await index.search(embedding: [1, 0], topK: 1)

        XCTAssertEqual(hits.first?.text, "existing")
    }

    func testRejectsModelDimensionBeforeEmbeddingOrMutation() async throws {
        let index = try VectorIndex(dimension: 2)
        let pipeline = Pipeline(
            index: index,
            embedder: try FakeEmbedder(dimension: 3, embeddings: [:]),
            chunker: try TextChunker(maxCharacters: 20)
        )

        await XCTAssertThrowsErrorAsync {
            _ = try await pipeline.add(document: Document(id: "doc-1", text: "content"))
        } verify: { error in
            XCTAssertEqual(
                error as? RetrievalKitPipelineError,
                .embeddingDimensionMismatch(expected: 2, actual: 3)
            )
        }
        let count = await index.activeChunkCount
        XCTAssertEqual(count, 0)
    }

    func testRejectsEmptyDocumentWithoutDeletingExistingChunks() async throws {
        let index = try VectorIndex(dimension: 2)
        try await index.upsert(
            document: Document(id: "doc-1"),
            chunks: [ChunkInput(text: "existing", embedding: [1, 0])]
        )
        let pipeline = Pipeline(
            index: index,
            embedder: try FakeEmbedder(dimension: 2, embeddings: [:]),
            chunker: try TextChunker(maxCharacters: 20)
        )

        await XCTAssertThrowsErrorAsync {
            _ = try await pipeline.add(document: Document(id: "doc-1", text: " \n "))
        }
        let count = await index.activeChunkCount
        XCTAssertEqual(count, 1)
    }

    func testAcceptsApplicationDefinedChunker() async throws {
        let index = try VectorIndex(dimension: 2)
        let embedder = try FakeEmbedder(
            dimension: 2,
            embeddings: ["custom first": [1, 0], "custom second": [0, 1]]
        )
        let pipeline = Pipeline(
            index: index,
            embedder: embedder,
            chunker: CustomChunker()
        )

        let result = try await pipeline.add(
            document: Document(id: "doc-custom", text: "custom first custom second")
        )
        let first = try await index.search(embedding: [1, 0], topK: 1)

        XCTAssertEqual(result.chunkCount, 2)
        XCTAssertEqual(first.first?.text, "custom first")
    }

    func testDefaultInitializerUsesBuiltInRustChunker() async throws {
        let index = try VectorIndex(dimension: 2)
        let embedder = try FakeEmbedder(
            dimension: 2,
            embeddings: ["default document": [1, 0]]
        )
        let pipeline = Pipeline(index: index, embedder: embedder)

        let result = try await pipeline.add(
            document: Document(id: "default", text: "default document")
        )

        XCTAssertEqual(result.chunkCount, 1)
    }

    func testDefaultPipelineSubdividesChunksToEmbeddingTokenLimit() async throws {
        let index = try VectorIndex(dimension: 2)
        let embedder = try TokenCountingEmbedder(dimension: 2, maxInputTokens: 4)
        let pipeline = Pipeline(index: index, embedder: embedder)

        let result = try await pipeline.add(
            document: Document(id: "token-aware", text: "one two three four five six")
        )

        XCTAssertGreaterThan(result.chunkCount, 1)
        XCTAssertTrue(embedder.embeddedTexts.allSatisfy { text in
            (try? embedder.tokenCounter?.countTokens(in: text)) ?? 999 <= 4
        })
    }

    func testRejectsInvalidCustomChunkBeforeEmbeddingOrMutation() async throws {
        let index = try VectorIndex(dimension: 2)
        let pipeline = Pipeline(
            index: index,
            embedder: try FakeEmbedder(dimension: 2, embeddings: [:]),
            chunker: InvalidRangeChunker()
        )

        await XCTAssertThrowsErrorAsync {
            _ = try await pipeline.add(document: Document(id: "invalid", text: "hello"))
        } verify: { error in
            XCTAssertEqual(
                error as? RetrievalKitPipelineError,
                .invalidChunkRange(position: 0, startByte: 99, endByte: 1, sourceByteCount: 5)
            )
        }
        let count = await index.activeChunkCount
        XCTAssertEqual(count, 0)
    }
}

private struct CustomChunker: DocumentChunker {
    func chunks(for text: String) throws -> [TextChunk] {
        [
            TextChunk(text: "custom first", startByte: 0, endByte: 12),
            TextChunk(text: "custom second", startByte: 13, endByte: 26),
        ]
    }
}

private struct InvalidRangeChunker: DocumentChunker {
    func chunks(for text: String) throws -> [TextChunk] {
        [TextChunk(text: "hello", startByte: 99, endByte: 1)]
    }
}

private struct FakeEmbedder: TextEmbedder {
    let modelInfo: EmbeddingModelInfo
    let runtimeInfo = EmbeddingRuntimeInfo(name: "fake")
    let embeddings: [String: [Float]]

    init(dimension: Int, embeddings: [String: [Float]]) throws {
        modelInfo = try EmbeddingModelInfo(identifier: "fake", dimension: dimension)
        self.embeddings = embeddings
    }

    func embed(_ text: String) async throws -> [Float] {
        guard let embedding = embeddings[text] else { throw FakeError.missing(text) }
        return embedding
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        try texts.map { text in
            guard let embedding = embeddings[text] else { throw FakeError.missing(text) }
            return embedding
        }
    }
}

private struct FailingEmbedder: TextEmbedder {
    let modelInfo: EmbeddingModelInfo
    let runtimeInfo = EmbeddingRuntimeInfo(name: "failing")

    init(dimension: Int) throws {
        modelInfo = try EmbeddingModelInfo(identifier: "failing", dimension: dimension)
    }

    func embed(_ text: String) async throws -> [Float] {
        throw FakeError.intentional
    }
}

private final class TokenCountingEmbedder: TextEmbedder, @unchecked Sendable {
    let modelInfo: EmbeddingModelInfo
    let runtimeInfo = EmbeddingRuntimeInfo(name: "token-counting")
    let tokenCounter: (any TextTokenCounter)? = WordTokenCounter()
    private(set) var embeddedTexts: [String] = []

    init(dimension: Int, maxInputTokens: Int) throws {
        modelInfo = try EmbeddingModelInfo(
            identifier: "token-counting",
            dimension: dimension,
            maxInputTokens: maxInputTokens
        )
    }

    func embed(_ text: String) async throws -> [Float] {
        embeddedTexts.append(text)
        return [1, 0]
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        embeddedTexts.append(contentsOf: texts)
        return texts.map { _ in [1, 0] }
    }
}

private struct WordTokenCounter: TextTokenCounter {
    func countTokens(in text: String) throws -> Int {
        text.split(whereSeparator: \.isWhitespace).count + 2
    }
}

private enum FakeError: Error {
    case missing(String)
    case intentional
}

private func XCTAssertThrowsErrorAsync<T>(
    _ expression: () async throws -> T,
    verify: (Error) -> Void = { _ in },
    file: StaticString = #filePath,
    line: UInt = #line
) async {
    do {
        _ = try await expression()
        XCTFail("expected error", file: file, line: line)
    } catch {
        verify(error)
    }
}
