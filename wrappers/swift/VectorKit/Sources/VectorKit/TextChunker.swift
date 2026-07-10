import Foundation
import VectorKitFFI

/// A deterministic text segment produced by the shared Rust ingestion layer.
public struct TextChunk: Equatable, Sendable {
    /// Chunk text copied from the original input.
    public let text: String
    /// Inclusive UTF-8 byte offset in the original input.
    public let startByte: Int
    /// Exclusive UTF-8 byte offset in the original input.
    public let endByte: Int
}

/// Configures the shared Rust text chunker.
public struct TextChunker: Equatable, Sendable {
    public enum Strategy: Equatable, Sendable {
        /// Split exactly at the configured Unicode-character limit.
        case fixed
        /// Prefer sentence endings, then whitespace, before the limit.
        case sentence

        fileprivate var ffiValue: UInt32 {
            switch self {
            case .fixed: 0
            case .sentence: 1
            }
        }
    }

    public let strategy: Strategy
    public let maxCharacters: Int
    public let overlapCharacters: Int

    public init(
        strategy: Strategy = .sentence,
        maxCharacters: Int,
        overlapCharacters: Int = 0
    ) throws {
        guard maxCharacters > 0 else {
            throw VectorKitError.invalidArgument("maxCharacters must be greater than zero")
        }
        guard overlapCharacters >= 0, overlapCharacters < maxCharacters else {
            throw VectorKitError.invalidArgument(
                "overlapCharacters must be non-negative and smaller than maxCharacters"
            )
        }
        self.strategy = strategy
        self.maxCharacters = maxCharacters
        self.overlapCharacters = overlapCharacters
    }

    /// Splits text while preserving UTF-8 byte offsets into the original input.
    public func chunks(for text: String) throws -> [TextChunk] {
        let input = strdup(text)
        guard let input else {
            throw VectorKitError.invalidArgument("could not allocate UTF-8 input")
        }
        defer { free(input) }

        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        var output = VkTextChunkBuffer(chunks: nil, count: 0)

        guard vectorkit_chunk_text(
            input,
            strategy.ffiValue,
            maxCharacters,
            overlapCharacters,
            &output,
            &status
        ) else {
            throw VectorKitError.from(status: status)
        }
        defer { vectorkit_text_chunks_free(output) }

        guard let chunks = output.chunks else { return [] }
        return UnsafeBufferPointer(start: chunks, count: output.count).map { chunk in
            TextChunk(
                text: chunk.text.map { String(cString: $0) } ?? "",
                startByte: chunk.start_byte,
                endByte: chunk.end_byte
            )
        }
    }
}
