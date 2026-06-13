#if canImport(CoreML)
import CoreML
import Foundation

/// Core ML feature names expected by EmbeddingKit's transformer model contract.
public struct CoreMLFeatureNames: Codable, Equatable, Sendable {
    /// Core ML input feature for token IDs.
    public var inputIDs: String
    /// Core ML input feature for attention mask values.
    public var attentionMask: String
    /// Optional Core ML input feature for token type IDs.
    public var tokenTypeIDs: String?
    /// Core ML output feature containing the pooled embedding vector.
    public var embedding: String

    public init(
        inputIDs: String = "input_ids",
        attentionMask: String = "attention_mask",
        tokenTypeIDs: String? = "token_type_ids",
        embedding: String = "embedding"
    ) {
        self.inputIDs = inputIDs
        self.attentionMask = attentionMask
        self.tokenTypeIDs = tokenTypeIDs
        self.embedding = embedding
    }
}

/// Core ML model loading configuration.
public struct CoreMLModelConfiguration: Equatable, Sendable {
    /// URL to a compiled `.mlmodelc` directory or model package supported by the concrete backend.
    public var modelURL: URL
    /// Requested Core ML compute mode.
    public var compute: EmbeddingCompute
    /// Expected model feature names.
    public var featureNames: CoreMLFeatureNames

    public init(
        modelURL: URL,
        compute: EmbeddingCompute = .all,
        featureNames: CoreMLFeatureNames = CoreMLFeatureNames()
    ) {
        self.modelURL = modelURL
        self.compute = compute
        self.featureNames = featureNames
    }
}

/// Minimal backend contract used by `CoreMLEmbedder`.
///
/// Real Core ML loading and MLMultiArray conversion will live behind this boundary.
public protocol CoreMLEmbeddingBackend: Sendable {
    /// Runtime metadata reported in benchmark output.
    var runtimeInfo: EmbeddingRuntimeInfo { get }

    /// Predicts one pooled embedding from tokenized model input.
    func predictEmbedding(for input: TokenizedText) async throws -> [Float]
    /// Predicts pooled embeddings for a batch of tokenized model inputs.
    func predictEmbeddings(for inputs: [TokenizedText]) async throws -> [[Float]]
}

public extension CoreMLEmbeddingBackend {
    func predictEmbeddings(for inputs: [TokenizedText]) async throws -> [[Float]] {
        guard !inputs.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        var embeddings: [[Float]] = []
        embeddings.reserveCapacity(inputs.count)
        for input in inputs {
            embeddings.append(try await predictEmbedding(for: input))
        }
        return embeddings
    }
}

/// Core ML-backed text embedder shell.
///
/// This actor owns tokenizer/backend coordination while keeping model inference
/// isolated from callers. The default model-URL initializer intentionally uses an
/// unsupported backend until the concrete Core ML model interface is implemented.
public actor CoreMLEmbedder: TextEmbedder {
    public nonisolated let modelInfo: EmbeddingModelInfo
    public nonisolated let runtimeInfo: EmbeddingRuntimeInfo

    private let tokenizer: any TextTokenizer
    private let backend: any CoreMLEmbeddingBackend

    public init(
        modelInfo: EmbeddingModelInfo,
        tokenizer: any TextTokenizer,
        backend: any CoreMLEmbeddingBackend
    ) {
        self.modelInfo = modelInfo
        self.runtimeInfo = backend.runtimeInfo
        self.tokenizer = tokenizer
        self.backend = backend
    }

    public init(
        modelInfo: EmbeddingModelInfo,
        tokenizer: any TextTokenizer,
        configuration: CoreMLModelConfiguration
    ) {
        let backend = UnsupportedCoreMLBackend(configuration: configuration)
        self.modelInfo = modelInfo
        self.runtimeInfo = backend.runtimeInfo
        self.tokenizer = tokenizer
        self.backend = backend
    }

    public func embed(_ text: String) async throws -> [Float] {
        guard !text.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        let tokenized = try tokenizer.tokenize(text)
        let embedding = try await backend.predictEmbedding(for: tokenized)
        try validateEmbedding(embedding)
        return embedding
    }

    public func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        let tokenized = try tokenizer.tokenize(texts)
        let embeddings = try await backend.predictEmbeddings(for: tokenized)
        guard embeddings.count == texts.count else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "backend returned \(embeddings.count) embeddings for \(texts.count) inputs"
            )
        }
        for embedding in embeddings {
            try validateEmbedding(embedding)
        }
        return embeddings
    }

    private func validateEmbedding(_ embedding: [Float]) throws {
        guard embedding.count == modelInfo.dimension else {
            throw EmbeddingKitError.invalidDimension(
                expected: modelInfo.dimension,
                actual: embedding.count
            )
        }
    }
}

private struct UnsupportedCoreMLBackend: CoreMLEmbeddingBackend {
    let configuration: CoreMLModelConfiguration
    let runtimeInfo: EmbeddingRuntimeInfo

    init(configuration: CoreMLModelConfiguration) {
        self.configuration = configuration
        self.runtimeInfo = EmbeddingRuntimeInfo(
            name: "Core ML",
            requestedCompute: configuration.compute,
            actualCompute: nil
        )
    }

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        throw EmbeddingKitError.unsupportedModelInterface(
            "Core ML model loading is not implemented yet for \(configuration.modelURL.path); expected inputs \(configuration.featureNames.inputIDs), \(configuration.featureNames.attentionMask), optional \(configuration.featureNames.tokenTypeIDs ?? "none"), and output \(configuration.featureNames.embedding)"
        )
    }
}
#endif
