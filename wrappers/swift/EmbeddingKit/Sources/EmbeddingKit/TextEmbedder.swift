import Foundation

/// Provider-neutral text embedding interface.
public protocol TextEmbedder: Sendable {
    /// Stable model metadata. Indexing and querying must use compatible metadata.
    var modelInfo: EmbeddingModelInfo { get }
    /// Runtime and compute metadata for diagnostics and benchmarks.
    var runtimeInfo: EmbeddingRuntimeInfo { get }

    /// Embeds a single text string.
    func embed(_ text: String) async throws -> [Float]
    /// Embeds a batch of text strings.
    func embed(_ texts: [String]) async throws -> [[Float]]
}

public extension TextEmbedder {
    /// Default batch implementation for providers that only implement single-text embedding.
    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        var embeddings: [[Float]] = []
        embeddings.reserveCapacity(texts.count)
        for text in texts {
            embeddings.append(try await embed(text))
        }
        return embeddings
    }
}
