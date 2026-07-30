import Foundation

/// Similarity metric expected by embeddings produced by a model.
public enum EmbeddingSimilarityMetric: String, Codable, Equatable, Sendable {
    /// Use cosine similarity. RetrievalKit normalizes vectors when the index uses cosine.
    case cosine
    /// Use raw dot product similarity.
    case dotProduct
}

/// Requested compute mode for hardware-backed embedding providers.
public enum EmbeddingCompute: String, Codable, Equatable, Sendable {
    /// Let the provider choose the best available runtime.
    case auto
    /// Force CPU execution when supported.
    case cpuOnly
    /// Allow CPU and GPU execution when supported.
    case cpuAndGPU
    /// Allow CPU and Neural Engine execution when supported.
    case cpuAndNeuralEngine
    /// Allow every provider-supported compute unit.
    case all
    /// Used when the provider cannot report compute details.
    case unknown
}

/// Runtime metadata captured with embedding benchmark results.
public struct EmbeddingRuntimeInfo: Codable, Equatable, Sendable {
    /// Runtime name, for example Core ML, FastEmbed, ONNX Runtime, or Precomputed.
    public var name: String
    /// Optional runtime version.
    public var version: String?
    /// Requested compute mode.
    public var requestedCompute: EmbeddingCompute
    /// Compute mode actually used, when the provider can report it.
    public var actualCompute: EmbeddingCompute?

    public init(
        name: String,
        version: String? = nil,
        requestedCompute: EmbeddingCompute = .auto,
        actualCompute: EmbeddingCompute? = nil
    ) {
        self.name = name
        self.version = version
        self.requestedCompute = requestedCompute
        self.actualCompute = actualCompute
    }
}

/// Stable metadata that must match between indexed document embeddings and query embeddings.
public struct EmbeddingModelInfo: Codable, Equatable, Sendable {
    /// Provider-facing model identifier, for example `BAAI/bge-small-en-v1.5`.
    public var identifier: String
    /// Optional model revision, checksum, or app-managed version.
    public var revision: String?
    /// Output embedding dimension.
    public var dimension: Int
    /// Optional model input token limit.
    public var maxInputTokens: Int?
    /// Whether the provider already returns L2-normalized embeddings.
    public var producesNormalizedEmbeddings: Bool
    /// Similarity metric recommended for vectors from this model.
    public var recommendedMetric: EmbeddingSimilarityMetric

    public init(
        identifier: String,
        revision: String? = nil,
        dimension: Int,
        maxInputTokens: Int? = nil,
        producesNormalizedEmbeddings: Bool = false,
        recommendedMetric: EmbeddingSimilarityMetric = .cosine
    ) throws {
        guard dimension > 0 else {
            throw EmbeddingKitError.invalidDimension(expected: 1, actual: dimension)
        }
        if let maxInputTokens, maxInputTokens <= 0 {
            throw EmbeddingKitError.unsupportedModelInterface(
                "maxInputTokens must be greater than zero"
            )
        }
        self.identifier = identifier
        self.revision = revision
        self.dimension = dimension
        self.maxInputTokens = maxInputTokens
        self.producesNormalizedEmbeddings = producesNormalizedEmbeddings
        self.recommendedMetric = recommendedMetric
    }
}

/// Known model metadata used by benchmarks and examples.
public enum KnownEmbeddingModels {
    public static let bgeSmallEnV15 = try! EmbeddingModelInfo(
        identifier: "BAAI/bge-small-en-v1.5",
        dimension: 384,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let allMiniLML6V2 = try! EmbeddingModelInfo(
        identifier: "sentence-transformers/all-MiniLM-L6-v2",
        revision: "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
        dimension: 384,
        maxInputTokens: 256,
        producesNormalizedEmbeddings: true,
        recommendedMetric: .cosine
    )

    public static let snowflakeArcticEmbedXS = try! EmbeddingModelInfo(
        identifier: "snowflake/snowflake-arctic-embed-xs",
        dimension: 384,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let snowflakeArcticEmbedS = try! EmbeddingModelInfo(
        identifier: "snowflake/snowflake-arctic-embed-s",
        dimension: 384,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let e5SmallV2 = try! EmbeddingModelInfo(
        identifier: "intfloat/e5-small-v2",
        dimension: 384,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let gteSmall = try! EmbeddingModelInfo(
        identifier: "thenlper/gte-small",
        dimension: 384,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let bgeBaseEnV15 = try! EmbeddingModelInfo(
        identifier: "BAAI/bge-base-en-v1.5",
        dimension: 768,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )

    public static let snowflakeArcticEmbedM = try! EmbeddingModelInfo(
        identifier: "snowflake/snowflake-arctic-embed-m",
        dimension: 768,
        maxInputTokens: 512,
        recommendedMetric: .cosine
    )
}
