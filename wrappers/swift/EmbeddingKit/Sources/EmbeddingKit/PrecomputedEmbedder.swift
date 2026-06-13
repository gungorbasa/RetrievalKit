import Foundation

/// Deterministic embedder backed by caller-provided vectors.
///
/// This provider is intended for tests, fixtures, and benchmark harness validation.
public struct PrecomputedEmbedder: TextEmbedder {
    public let modelInfo: EmbeddingModelInfo
    public let runtimeInfo: EmbeddingRuntimeInfo

    private let embeddings: [String: [Float]]

    public init(
        modelInfo: EmbeddingModelInfo,
        embeddings: [String: [Float]],
        runtimeInfo: EmbeddingRuntimeInfo = EmbeddingRuntimeInfo(
            name: "Precomputed",
            requestedCompute: .cpuOnly,
            actualCompute: .cpuOnly
        )
    ) throws {
        guard !embeddings.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        for vector in embeddings.values {
            guard vector.count == modelInfo.dimension else {
                throw EmbeddingKitError.invalidDimension(
                    expected: modelInfo.dimension,
                    actual: vector.count
                )
            }
        }

        self.modelInfo = modelInfo
        self.runtimeInfo = runtimeInfo
        self.embeddings = embeddings
    }

    public func embed(_ text: String) async throws -> [Float] {
        guard !text.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        guard let embedding = embeddings[text] else {
            throw EmbeddingKitError.missingPrecomputedEmbedding(text)
        }
        return embedding
    }

    public func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        return try texts.map { text in
            guard !text.isEmpty else {
                throw EmbeddingKitError.emptyInput
            }
            guard let embedding = embeddings[text] else {
                throw EmbeddingKitError.missingPrecomputedEmbedding(text)
            }
            return embedding
        }
    }
}
