import EmbeddingKit
import Foundation
import RetrievalKit
import RetrievalKitIngest

/// Application-defined chunking policy consumed and validated by `Pipeline`.
public protocol DocumentChunker: Sendable {
    /// Returns ordered, non-empty slices of the original text with UTF-8 byte ranges.
    func chunks(for text: String) throws -> [TextChunk]
}

extension TextChunker: DocumentChunker {}

/// Subdivides Rust-produced chunks until each one fits an exact model token budget.
public struct TokenAwareDocumentChunker: DocumentChunker {
    public let baseChunker: TextChunker
    public let tokenCounter: any TextTokenCounter
    public let maxTokens: Int

    public init(
        baseChunker: TextChunker = .pipelineDefault,
        tokenCounter: any TextTokenCounter,
        maxTokens: Int
    ) {
        self.baseChunker = baseChunker
        self.tokenCounter = tokenCounter
        self.maxTokens = maxTokens
    }

    public func chunks(for text: String) throws -> [TextChunk] {
        try baseChunker.chunks(for: text).flatMap { try fit($0) }
    }

    private func fit(_ chunk: TextChunk) throws -> [TextChunk] {
        if try tokenCounter.countTokens(in: chunk.text) <= maxTokens {
            return [chunk]
        }

        let characterCount = chunk.text.count
        guard characterCount > 1 else {
            throw RetrievalKitPipelineError.tokenLimitTooSmall(
                maxTokens: maxTokens,
                text: chunk.text
            )
        }

        let splitter = try TextChunker(
            strategy: .sentence,
            maxCharacters: max(1, characterCount / 2),
            overlapCharacters: 0
        )
        let pieces = try splitter.chunks(for: chunk.text)
        return try pieces.flatMap { piece in
            try fit(
                TextChunk(
                    text: piece.text,
                    startByte: chunk.startByte + piece.startByte,
                    endByte: chunk.startByte + piece.endByte
                )
            )
        }
    }
}

/// Validation errors detected before the pipeline mutates the index.
public enum RetrievalKitPipelineError: Error, Equatable, CustomStringConvertible, Sendable {
    case emptyDocument(documentID: String)
    case emptyChunk(position: Int)
    case invalidChunkRange(position: Int, startByte: Int, endByte: Int, sourceByteCount: Int)
    case chunkRangeNotOnUTF8Boundary(position: Int)
    case chunkTextMismatch(position: Int)
    case chunksOutOfOrder(previousPosition: Int, position: Int)
    case tokenLimitTooSmall(maxTokens: Int, text: String)
    case embeddingCountMismatch(expected: Int, actual: Int)
    case embeddingDimensionMismatch(expected: Int, actual: Int)

    public var description: String {
        switch self {
        case .emptyDocument(let documentID):
            "Document '\(documentID)' produced no chunks. Return at least one non-empty source slice."
        case .emptyChunk(let position):
            "Chunk \(position) is empty. Return non-whitespace source text."
        case .invalidChunkRange(let position, let startByte, let endByte, let sourceByteCount):
            "Chunk \(position) has invalid UTF-8 range \(startByte)..<\(endByte); "
                + "expected 0...\(sourceByteCount) with startByte <= endByte."
        case .chunkRangeNotOnUTF8Boundary(let position):
            "Chunk \(position) range cuts through a UTF-8 character. Return offsets on character boundaries."
        case .chunkTextMismatch(let position):
            "Chunk \(position) text does not match its source byte range. "
                + "Return the exact source slice or correct the offsets."
        case .chunksOutOfOrder(let previousPosition, let position):
            "Chunk \(position) starts before chunk \(previousPosition). Return chunks in source order."
        case .tokenLimitTooSmall(let maxTokens, let text):
            "Token limit \(maxTokens) cannot fit source text '\(text)'. "
                + "Use a model with a larger input limit or a tokenizer with fewer special tokens."
        case .embeddingCountMismatch(let expected, let actual):
            "Embedding provider returned \(actual) embeddings for \(expected) chunks. "
                + "Return exactly one embedding per input text."
        case .embeddingDimensionMismatch(let expected, let actual):
            "Embedding dimension mismatch: expected \(expected), got \(actual). "
                + "Use the same embedding model for indexing and queries."
        }
    }
}

/// Summary returned after a complete document replacement succeeds.
public struct IngestionResult: Equatable, Sendable {
    public let documentID: String
    public let chunkIDs: [UInt64]

    public var chunkCount: Int { chunkIDs.count }

    public init(documentID: String, chunkIDs: [UInt64]) {
        self.documentID = documentID
        self.chunkIDs = chunkIDs
    }
}

/// Composes chunking, embedding, indexing, and text-query search.
public struct Pipeline: Sendable {
    public let index: VectorIndex
    public let embedder: any TextEmbedder
    public let chunker: any DocumentChunker

    /// Creates a pipeline with opinionated sentence-aware Rust chunking.
    public init(index: VectorIndex, embedder: any TextEmbedder) {
        let chunker: any DocumentChunker
        if let tokenCounter = embedder.tokenCounter,
           let maxTokens = embedder.modelInfo.maxInputTokens {
            chunker = TokenAwareDocumentChunker(
                tokenCounter: tokenCounter,
                maxTokens: maxTokens
            )
        } else {
            chunker = TextChunker.pipelineDefault
        }
        self.init(index: index, embedder: embedder, chunker: chunker)
    }

    /// Creates a pipeline with an application-defined chunking policy.
    public init(
        index: VectorIndex,
        embedder: any TextEmbedder,
        chunker: any DocumentChunker
    ) {
        self.index = index
        self.embedder = embedder
        self.chunker = chunker
    }

    /// Chunks and embeds the entire document before atomically replacing its indexed chunks.
    public func add(document: Document) async throws -> IngestionResult {
        let expectedDimension = await index.dimension
        try validateModelDimension(expected: expectedDimension)

        let textChunks = try chunker.chunks(for: document.text)
        guard !textChunks.isEmpty else {
            throw RetrievalKitPipelineError.emptyDocument(documentID: document.id)
        }
        try validateChunks(textChunks, source: document.text)

        let embeddings = try await embedder.embed(textChunks.map(\.text))
        guard embeddings.count == textChunks.count else {
            throw RetrievalKitPipelineError.embeddingCountMismatch(
                expected: textChunks.count,
                actual: embeddings.count
            )
        }

        let chunkInputs = try zip(textChunks, embeddings).enumerated().map { index, pair in
            let (textChunk, embedding) = pair
            try validateEmbeddingDimension(embedding, expected: expectedDimension)
            return ChunkInput(
                text: textChunk.text,
                embedding: embedding,
                metadata: [
                    "retrievalkit.chunk.index": .integer(Int64(index)),
                    "retrievalkit.chunk.start_byte": .integer(Int64(textChunk.startByte)),
                    "retrievalkit.chunk.end_byte": .integer(Int64(textChunk.endByte)),
                ]
            )
        }

        let chunkIDs = try await index.upsert(document: document, chunks: chunkInputs)
        return IngestionResult(documentID: document.id, chunkIDs: chunkIDs)
    }

    /// Embeds text and performs hybrid vector plus BM25 search.
    public func search(
        _ text: String,
        topK: Int = 10,
        filter: Filter? = nil,
        options: HybridOptions = .default
    ) async throws -> [HybridResult] {
        let expectedDimension = await index.dimension
        try validateModelDimension(expected: expectedDimension)
        let embedding = try await embedder.embed(text)
        try validateEmbeddingDimension(embedding, expected: expectedDimension)
        return try await index.hybridSearch(
            text: text,
            embedding: embedding,
            topK: topK,
            filter: filter,
            options: options
        )
    }

    private func validateModelDimension(expected: Int) throws {
        guard embedder.modelInfo.dimension == expected else {
            throw RetrievalKitPipelineError.embeddingDimensionMismatch(
                expected: expected,
                actual: embedder.modelInfo.dimension
            )
        }
    }

    private func validateEmbeddingDimension(_ embedding: [Float], expected: Int) throws {
        guard embedding.count == expected else {
            throw RetrievalKitPipelineError.embeddingDimensionMismatch(
                expected: expected,
                actual: embedding.count
            )
        }
    }

    private func validateChunks(_ chunks: [TextChunk], source: String) throws {
        let sourceBytes = source.utf8
        var previousStartByte: Int?

        for (position, chunk) in chunks.enumerated() {
            guard !chunk.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                throw RetrievalKitPipelineError.emptyChunk(position: position)
            }
            guard chunk.startByte >= 0,
                  chunk.endByte >= chunk.startByte,
                  chunk.endByte <= sourceBytes.count else {
                throw RetrievalKitPipelineError.invalidChunkRange(
                    position: position,
                    startByte: chunk.startByte,
                    endByte: chunk.endByte,
                    sourceByteCount: sourceBytes.count
                )
            }
            if let previousStartByte, chunk.startByte < previousStartByte {
                throw RetrievalKitPipelineError.chunksOutOfOrder(
                    previousPosition: position - 1,
                    position: position
                )
            }

            let startUTF8 = sourceBytes.index(sourceBytes.startIndex, offsetBy: chunk.startByte)
            let endUTF8 = sourceBytes.index(sourceBytes.startIndex, offsetBy: chunk.endByte)
            guard let start = String.Index(startUTF8, within: source),
                  let end = String.Index(endUTF8, within: source) else {
                throw RetrievalKitPipelineError.chunkRangeNotOnUTF8Boundary(position: position)
            }
            guard String(source[start..<end]) == chunk.text else {
                throw RetrievalKitPipelineError.chunkTextMismatch(position: position)
            }
            previousStartByte = chunk.startByte
        }
    }
}
