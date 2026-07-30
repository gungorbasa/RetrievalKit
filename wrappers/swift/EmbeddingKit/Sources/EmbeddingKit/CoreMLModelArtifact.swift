#if canImport(CoreML)
import Foundation

/// Network policy used while constructing or explicitly prefetching a Core ML embedder.
public enum CoreMLModelAccess: Sendable {
    /// Use a verified cached artifact, downloading it over HTTPS when absent.
    case downloadIfNeeded
    /// Require a verified cached artifact and perform no network request.
    case localOnly
}

/// Immutable identity and archive layout for a downloadable Core ML model.
///
/// Model precision describes inference weights. It is independent from
/// RetrievalKit's database vector encoding (including `I8ScalarQuantized`).
public struct CoreMLModelArtifact: Equatable, Sendable {
    public let identifier: String
    public let sourceModelRevision: String
    public let artifactRevision: String
    public let archiveURL: URL
    public let archiveSHA256: String
    public let archiveByteCount: Int64
    public let manifestSHA256: String

    init(
        identifier: String,
        sourceModelRevision: String,
        artifactRevision: String,
        archiveURL: URL,
        archiveSHA256: String,
        archiveByteCount: Int64,
        manifestSHA256: String
    ) throws {
        guard archiveURL.scheme?.lowercased() == "https", archiveURL.host != nil else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "Core ML artifacts must use an absolute HTTPS URL"
            )
        }
        guard Self.isSHA256(archiveSHA256), Self.isSHA256(manifestSHA256) else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "artifact hashes must be lowercase 64-character SHA-256 values"
            )
        }
        guard archiveByteCount > 0 else {
            throw EmbeddingKitError.unsupportedModelInterface(
                "artifact archiveByteCount must be greater than zero"
            )
        }
        self.identifier = identifier
        self.sourceModelRevision = sourceModelRevision
        self.artifactRevision = artifactRevision
        self.archiveURL = archiveURL
        self.archiveSHA256 = archiveSHA256
        self.archiveByteCount = archiveByteCount
        self.manifestSHA256 = manifestSHA256
    }

    var cacheKey: String {
        "\(identifier)-\(artifactRevision)-\(archiveSHA256)"
            .replacingOccurrences(
                of: "[^A-Za-z0-9._-]",
                with: "-",
                options: .regularExpression
            )
    }

    private static func isSHA256(_ value: String) -> Bool {
        value.count == 64 && value.allSatisfy { $0.isHexDigit && !$0.isUppercase }
    }
}

/// Canonical production Core ML model profiles.
public enum CoreMLProductionModels {
    /// FP32 `all-MiniLM-L6-v2`, fixed at 256 tokens with 384 normalized F32 output values.
    ///
    /// The archive was verified after a clean HTTPS re-download from this
    /// immutable commit. Never change this URL to a mutable branch such as `main`.
    public static let allMiniLML6V2FP32: CoreMLModelArtifact = try! CoreMLModelArtifact(
        identifier: "sentence-transformers--all-MiniLM-L6-v2--c9745ed1d9f207416be6d2e6f8de32d1f16199bf--coreml-fp32-fixed256-v1",
        sourceModelRevision: "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
        artifactRevision: "405818d6afef1aaf2fc8da67da6caf20b55f0a28",
        archiveURL: URL(
            string: "https://huggingface.co/gungorbasa/retrievalkit-minilm/resolve/405818d6afef1aaf2fc8da67da6caf20b55f0a28/all-MiniLM-L6-v2-coreml-fp32-v1.tar"
        )!,
        archiveSHA256: "e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f",
        archiveByteCount: 90_664_960,
        manifestSHA256: "085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60"
    )
}
#endif
