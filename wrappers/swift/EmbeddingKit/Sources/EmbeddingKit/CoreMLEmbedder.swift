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

/// Shape used for token input MLMultiArrays.
public enum CoreMLTokenInputShape: String, Codable, Equatable, Sendable {
    /// One-dimensional `[sequenceLength]` token arrays.
    case sequence
    /// Two-dimensional `[1, sequenceLength]` token arrays.
    case batchSequence
}

/// Core ML model loading configuration.
public struct CoreMLModelConfiguration: Equatable, Sendable {
    /// URL to a compiled `.mlmodelc` directory or model package supported by the concrete backend.
    public var modelURL: URL
    /// Requested Core ML compute mode.
    public var compute: EmbeddingCompute
    /// Expected model feature names.
    public var featureNames: CoreMLFeatureNames
    /// Shape used when converting tokenizer output to MLMultiArray inputs.
    public var tokenInputShape: CoreMLTokenInputShape
    /// Number of Core ML model backend actors used for parallel batch inference.
    public var backendPoolSize: Int

    public init(
        modelURL: URL,
        compute: EmbeddingCompute = .all,
        featureNames: CoreMLFeatureNames = CoreMLFeatureNames(),
        tokenInputShape: CoreMLTokenInputShape = .batchSequence,
        backendPoolSize: Int = 1
    ) {
        self.modelURL = modelURL
        self.compute = compute
        self.featureNames = featureNames
        self.tokenInputShape = tokenInputShape
        self.backendPoolSize = max(1, backendPoolSize)
    }
}

/// Minimal backend contract used by `CoreMLEmbedder`.
public protocol CoreMLEmbeddingBackend: Actor {
    /// Runtime metadata reported in benchmark output.
    nonisolated var runtimeInfo: EmbeddingRuntimeInfo { get }

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

/// Core ML-backed text embedder.
///
/// This actor owns tokenizer/backend coordination while keeping model inference
/// isolated from callers.
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
    ) throws {
        let backend = try CoreMLModelBackendPool(configuration: configuration)
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

/// Pool of Core ML model backend actors for safe parallel inference.
public actor CoreMLModelBackendPool: CoreMLEmbeddingBackend {
    public nonisolated let runtimeInfo: EmbeddingRuntimeInfo

    private let backends: [CoreMLModelBackend]
    private var nextIndex = 0

    public init(configuration: CoreMLModelConfiguration) throws {
        var backends: [CoreMLModelBackend] = []
        backends.reserveCapacity(configuration.backendPoolSize)
        for _ in 0..<configuration.backendPoolSize {
            backends.append(try CoreMLModelBackend(configuration: configuration))
        }
        self.backends = backends
        self.runtimeInfo = backends[0].runtimeInfo
    }

    public func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        try await nextBackend().predictEmbedding(for: input)
    }

    public func predictEmbeddings(for inputs: [TokenizedText]) async throws -> [[Float]] {
        guard !inputs.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        let assignments = inputs.enumerated().map { offset, input in
            (offset, backendForBatchItem(offset), input)
        }
        nextIndex = (nextIndex + inputs.count) % backends.count

        var results = Array(repeating: [Float](), count: inputs.count)
        try await withThrowingTaskGroup(of: (Int, [Float]).self) { group in
            for (offset, backend, input) in assignments {
                group.addTask {
                    (offset, try await backend.predictEmbedding(for: input))
                }
            }

            for try await (offset, embedding) in group {
                results[offset] = embedding
            }
        }
        return results
    }

    private func nextBackend() -> CoreMLModelBackend {
        let backend = backends[nextIndex]
        nextIndex = (nextIndex + 1) % backends.count
        return backend
    }

    private func backendForBatchItem(_ offset: Int) -> CoreMLModelBackend {
        backends[(nextIndex + offset) % backends.count]
    }
}

/// Core ML model backend used by `CoreMLEmbedder`.
///
/// `MLModel` remains actor-isolated here. Parallelism is provided by
/// `CoreMLModelBackendPool`, which creates multiple backend actors.
public actor CoreMLModelBackend: CoreMLEmbeddingBackend {
    public nonisolated let runtimeInfo: EmbeddingRuntimeInfo

    private let model: MLModel
    private let featureNames: CoreMLFeatureNames
    private let tokenInputShape: CoreMLTokenInputShape

    public init(configuration: CoreMLModelConfiguration) throws {
        let mappedCompute = coreMLComputeUnits(for: configuration.compute)
        let modelConfiguration = MLModelConfiguration()
        modelConfiguration.computeUnits = mappedCompute.computeUnits

        self.model = try MLModel(
            contentsOf: configuration.modelURL,
            configuration: modelConfiguration
        )
        self.featureNames = configuration.featureNames
        self.tokenInputShape = configuration.tokenInputShape
        self.runtimeInfo = EmbeddingRuntimeInfo(
            name: "Core ML",
            requestedCompute: configuration.compute,
            actualCompute: mappedCompute.actualCompute
        )
    }

    public func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        let provider = try makeFeatureProvider(for: input)
        let output = try predict(provider)
        return try readEmbedding(from: output)
    }

    private func predict(_ provider: MLFeatureProvider) throws -> MLFeatureProvider {
        try model.prediction(from: provider, options: MLPredictionOptions())
    }

    private func makeFeatureProvider(for input: TokenizedText) throws -> MLFeatureProvider {
        var features: [String: MLFeatureValue] = [
            featureNames.inputIDs: try featureValue(from: input.inputIDs),
            featureNames.attentionMask: try featureValue(from: input.attentionMask),
        ]

        if let tokenTypeIDs = input.tokenTypeIDs, let tokenTypeFeature = featureNames.tokenTypeIDs {
            features[tokenTypeFeature] = try featureValue(from: tokenTypeIDs)
        }

        return try MLDictionaryFeatureProvider(dictionary: features)
    }

    private func featureValue(from values: [Int32]) throws -> MLFeatureValue {
        let shape: [NSNumber]
        switch tokenInputShape {
        case .sequence:
            shape = [NSNumber(value: values.count)]
        case .batchSequence:
            shape = [NSNumber(value: 1), NSNumber(value: values.count)]
        }

        let array = try MLMultiArray(
            shape: shape,
            dataType: .int32
        )
        for index in values.indices {
            array[index] = NSNumber(value: values[index])
        }
        return MLFeatureValue(multiArray: array)
    }

    private func readEmbedding(from output: MLFeatureProvider) throws -> [Float] {
        guard let feature = output.featureValue(for: featureNames.embedding) else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "missing Core ML output feature '\(featureNames.embedding)'"
            )
        }
        guard let array = feature.multiArrayValue else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "Core ML output feature '\(featureNames.embedding)' is not an MLMultiArray"
            )
        }
        return try flattenFloatArray(array)
    }
}

private func flattenFloatArray(_ array: MLMultiArray) throws -> [Float] {
    let count = array.count
    guard count > 0 else {
        throw EmbeddingKitError.unsupportedModelInterface("Core ML embedding output is empty")
    }

    switch array.dataType {
    case .float32, .double, .float16, .int32:
        return (0..<count).map { Float(truncating: array[$0]) }
    default:
        throw EmbeddingKitError.unsupportedModelInterface(
            "unsupported Core ML embedding output data type \(array.dataType)"
        )
    }
}

private struct MappedCoreMLCompute {
    var computeUnits: MLComputeUnits
    var actualCompute: EmbeddingCompute
}

private func coreMLComputeUnits(for compute: EmbeddingCompute) -> MappedCoreMLCompute {
    switch compute {
    case .cpuOnly:
        MappedCoreMLCompute(computeUnits: .cpuOnly, actualCompute: .cpuOnly)
    case .cpuAndGPU:
        MappedCoreMLCompute(computeUnits: .cpuAndGPU, actualCompute: .cpuAndGPU)
    case .cpuAndNeuralEngine:
        if #available(iOS 16.0, macOS 13.0, *) {
            MappedCoreMLCompute(
                computeUnits: .cpuAndNeuralEngine,
                actualCompute: .cpuAndNeuralEngine
            )
        } else {
            MappedCoreMLCompute(computeUnits: .all, actualCompute: .all)
        }
    case .all, .auto, .unknown:
        MappedCoreMLCompute(computeUnits: .all, actualCompute: .all)
    }
}
#endif
