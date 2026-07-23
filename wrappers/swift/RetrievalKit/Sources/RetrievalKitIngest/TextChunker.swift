import Foundation
import RetrievalKitFFI

/// A deterministic text segment produced by the shared Rust ingestion layer.
public struct TextChunk: Equatable, Sendable {
    /// Chunk text copied from the original input.
    public let text: String
    /// Inclusive UTF-8 byte offset in the original input.
    public let startByte: Int
    /// Exclusive UTF-8 byte offset in the original input.
    public let endByte: Int

    public init(text: String, startByte: Int, endByte: Int) {
        self.text = text
        self.startByte = startByte
        self.endByte = endByte
    }
}

/// Errors produced by the shared Rust text chunker or its Swift FFI boundary.
public enum TextChunkingError: Error, Equatable, CustomStringConvertible, Sendable {
    case invalidArgument(String)
    case core(String)
    case panic(String)
    case unknown(code: Int32, message: String)

    public var description: String {
        switch self {
        case .invalidArgument(let message), .core(let message), .panic(let message):
            message
        case .unknown(let code, let message):
            "RetrievalKitIngest error \(code): \(message)"
        }
    }

    fileprivate static func from(status: VkStatus) -> TextChunkingError {
        let message = status.message.map { String(cString: $0) } ?? "unknown RetrievalKitIngest FFI error"
        switch status.code {
        case 1: return .invalidArgument(message)
        case 2: return .core(message)
        case 3: return .panic(message)
        default: return .unknown(code: status.code, message: message)
        }
    }
}

/// Configures the shared Rust text chunker.
public struct TextChunker: Equatable, Sendable {
    /// Opinionated sentence-aware default used by the high-level pipeline.
    public static let pipelineDefault = try! TextChunker(
        strategy: .sentence,
        maxCharacters: 500,
        overlapCharacters: 50
    )
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
            throw TextChunkingError.invalidArgument("maxCharacters must be greater than zero")
        }
        guard overlapCharacters >= 0, overlapCharacters < maxCharacters else {
            throw TextChunkingError.invalidArgument(
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
            throw TextChunkingError.invalidArgument("could not allocate UTF-8 input")
        }
        defer { free(input) }

        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        var output = VkTextChunkBuffer(chunks: nil, count: 0)

        guard retrievalkit_chunk_text(
            input,
            strategy.ffiValue,
            maxCharacters,
            overlapCharacters,
            &output,
            &status
        ) else {
            throw TextChunkingError.from(status: status)
        }
        defer { retrievalkit_text_chunks_free(output) }

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
