import Foundation

/// Errors surfaced by EmbeddingKit provider and benchmark helpers.
public enum EmbeddingKitError: Error, Equatable, CustomStringConvertible, Sendable {
    /// The caller provided no input text.
    case emptyInput
    /// A model or embedding vector reported an invalid dimension.
    case invalidDimension(expected: Int, actual: Int)
    /// The precomputed provider has no vector for the requested text.
    case missingPrecomputedEmbedding(String)
    /// Benchmark settings are internally inconsistent.
    case invalidBenchmarkConfiguration(String)
    /// A provider cannot run because the model inputs or outputs do not match EmbeddingKit's contract.
    case unsupportedModelInterface(String)
    /// Provider-specific backend failure.
    case backend(String)
    /// A verified model artifact is not present and network access is disabled.
    case modelUnavailable(String)
    /// A downloaded or cached artifact did not match its immutable identity.
    case artifactVerificationFailed(String)
    /// A model archive contains an entry that cannot be extracted safely.
    case unsafeArchive(String)

    public var description: String {
        switch self {
        case .emptyInput:
            "input text cannot be empty"
        case .invalidDimension(let expected, let actual):
            "embedding dimension mismatch: expected \(expected), got \(actual)"
        case .missingPrecomputedEmbedding(let text):
            "missing precomputed embedding for text '\(text)'"
        case .invalidBenchmarkConfiguration(let message):
            "invalid benchmark configuration: \(message)"
        case .unsupportedModelInterface(let message):
            "unsupported model interface: \(message)"
        case .backend(let message):
            "embedding backend error: \(message)"
        case .modelUnavailable(let message):
            "embedding model unavailable: \(message)"
        case .artifactVerificationFailed(let message):
            "embedding artifact verification failed: \(message)"
        case .unsafeArchive(let message):
            "unsafe embedding artifact archive: \(message)"
        }
    }
}
