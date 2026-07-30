#if canImport(CoreML)
import CoreML
import Foundation

protocol CoreMLArtifactDownloading: Sendable {
    func download(from url: URL, to temporaryURL: URL) async throws
}

struct URLSessionCoreMLArtifactDownloader: CoreMLArtifactDownloading {
    func download(from url: URL, to temporaryURL: URL) async throws {
        guard url.scheme?.lowercased() == "https" else {
            throw EmbeddingKitError.artifactVerificationFailed(
                "refusing non-HTTPS model URL"
            )
        }
        let (downloadedURL, response) = try await URLSession.shared.download(from: url)
        guard let response = response as? HTTPURLResponse,
              response.url?.scheme?.lowercased() == "https",
              (200..<300).contains(response.statusCode) else {
            throw EmbeddingKitError.backend("model download returned a non-success HTTP response")
        }
        try FileManager.default.moveItem(at: downloadedURL, to: temporaryURL)
    }
}

protocol CoreMLModelCompiling: Sendable {
    func compile(packageAt url: URL) async throws -> URL
}

struct SystemCoreMLModelCompiler: CoreMLModelCompiling {
    func compile(packageAt url: URL) async throws -> URL {
        try await Task.detached(priority: .userInitiated) {
            try MLModel.compileModel(at: url)
        }.value
    }
}

struct PreparedCoreMLModel: Sendable {
    let modelURL: URL
    let tokenizerURL: URL
}

actor CoreMLModelStore {
    static let shared = CoreMLModelStore()

    private var preparations: [String: Task<PreparedCoreMLModel, Error>] = [:]
    private let downloader: any CoreMLArtifactDownloading
    private let compiler: any CoreMLModelCompiling

    init(
        downloader: any CoreMLArtifactDownloading = URLSessionCoreMLArtifactDownloader(),
        compiler: any CoreMLModelCompiling = SystemCoreMLModelCompiler()
    ) {
        self.downloader = downloader
        self.compiler = compiler
    }

    func prepare(
        artifact: CoreMLModelArtifact,
        access: CoreMLModelAccess,
        cacheDirectory: URL?
    ) async throws -> PreparedCoreMLModel {
        let root = try cacheDirectory ?? Self.defaultCacheDirectory()
        let key = "\(root.path)|\(artifact.cacheKey)"
        if let task = preparations[key] {
            return try await task.value
        }

        let downloader = self.downloader
        let compiler = self.compiler
        let task = Task<PreparedCoreMLModel, Error> {
            try await Self.prepareUnshared(
                artifact: artifact,
                access: access,
                root: root,
                downloader: downloader,
                compiler: compiler
            )
        }
        preparations[key] = task
        do {
            let prepared = try await task.value
            preparations[key] = nil
            return prepared
        } catch {
            preparations[key] = nil
            throw error
        }
    }

    func removeCompiledCache(
        artifact: CoreMLModelArtifact,
        cacheDirectory: URL?
    ) throws {
        let root = try cacheDirectory ?? Self.defaultCacheDirectory()
        let compiled = Self.compiledDirectory(root: root, artifact: artifact)
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: compiled.path) {
            try fileManager.removeItem(at: compiled)
        }
    }

    private static func prepareUnshared(
        artifact: CoreMLModelArtifact,
        access: CoreMLModelAccess,
        root: URL,
        downloader: any CoreMLArtifactDownloading,
        compiler: any CoreMLModelCompiling
    ) async throws -> PreparedCoreMLModel {
        let fileManager = FileManager.default
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        let artifactRoot = root.appendingPathComponent(artifact.cacheKey, isDirectory: true)
        try fileManager.createDirectory(at: artifactRoot, withIntermediateDirectories: true)
        let archive = artifactRoot.appendingPathComponent("artifact.tar")
        try await acquireArchive(
            artifact,
            archive: archive,
            access: access,
            downloader: downloader,
            fileManager: fileManager
        )

        let extracted = artifactRoot.appendingPathComponent("extracted", isDirectory: true)
        let manifest: CoreMLArchiveManifest
        if let cached = try cachedManifest(
            at: extracted,
            artifact: artifact,
            fileManager: fileManager
        ) {
            manifest = cached
        } else {
            if fileManager.fileExists(atPath: extracted.path) {
                try fileManager.removeItem(at: extracted)
            }
            let temporary = artifactRoot.appendingPathComponent(
                ".extract-\(UUID().uuidString)",
                isDirectory: true
            )
            defer { try? fileManager.removeItem(at: temporary) }
            manifest = try SafeTarArchive.extract(
                archiveURL: archive,
                to: temporary,
                expectedArtifactID: artifact.identifier,
                expectedManifestSHA256: artifact.manifestSHA256
            )
            try publishAtomically(
                temporary: temporary,
                destination: extracted,
                fileManager: fileManager
            )
        }

        let package = extracted.appendingPathComponent(manifest.modelPath, isDirectory: true)
        let tokenizer = extracted.appendingPathComponent(
            manifest.tokenizerPath,
            isDirectory: true
        )
        let compiled = compiledDirectory(root: root, artifact: artifact)
        if !fileManager.fileExists(atPath: compiled.path) {
            let compiledTemporary = try await compiler.compile(packageAt: package)
            defer { try? fileManager.removeItem(at: compiledTemporary) }
            try publishAtomically(
                temporary: compiledTemporary,
                destination: compiled,
                fileManager: fileManager
            )
        }
        return PreparedCoreMLModel(modelURL: compiled, tokenizerURL: tokenizer)
    }

    private static func acquireArchive(
        _ artifact: CoreMLModelArtifact,
        archive: URL,
        access: CoreMLModelAccess,
        downloader: any CoreMLArtifactDownloading,
        fileManager: FileManager
    ) async throws {
        if fileManager.fileExists(atPath: archive.path) {
            do {
                try verifyArchive(archive, artifact: artifact, fileManager: fileManager)
                return
            } catch {
                try fileManager.removeItem(at: archive)
            }
        }
        guard access == .downloadIfNeeded else {
            throw EmbeddingKitError.modelUnavailable(
                "verified \(artifact.identifier) is not cached (localOnly)"
            )
        }

        let temporary = archive.deletingLastPathComponent().appendingPathComponent(
            ".download-\(UUID().uuidString)"
        )
        defer { try? fileManager.removeItem(at: temporary) }
        do {
            try await downloader.download(from: artifact.archiveURL, to: temporary)
            try verifyArchive(temporary, artifact: artifact, fileManager: fileManager)
            try publishAtomically(
                temporary: temporary,
                destination: archive,
                fileManager: fileManager
            )
        } catch {
            try? fileManager.removeItem(at: temporary)
            throw error
        }
    }

    private static func verifyArchive(
        _ archive: URL,
        artifact: CoreMLModelArtifact,
        fileManager: FileManager
    ) throws {
        let attributes = try fileManager.attributesOfItem(atPath: archive.path)
        let size = (attributes[.size] as? NSNumber)?.int64Value
        guard size == artifact.archiveByteCount else {
            throw EmbeddingKitError.artifactVerificationFailed(
                "archive size mismatch: expected \(artifact.archiveByteCount), got \(size ?? -1)"
            )
        }
        guard try SHA256Digest.hex(ofFile: archive) == artifact.archiveSHA256 else {
            throw EmbeddingKitError.artifactVerificationFailed("archive SHA-256 mismatch")
        }
    }

    private static func cachedManifest(
        at extracted: URL,
        artifact: CoreMLModelArtifact,
        fileManager: FileManager
    ) throws -> CoreMLArchiveManifest? {
        let url = extracted.appendingPathComponent(SafeTarArchive.manifestPath)
        guard fileManager.fileExists(atPath: url.path) else { return nil }
        guard let data = try? Data(contentsOf: url) else { return nil }
        guard SHA256Digest.hex(of: data) == artifact.manifestSHA256,
              let manifest = try? JSONDecoder().decode(CoreMLArchiveManifest.self, from: data),
              manifest.schemaVersion == 1,
              manifest.artifactID == artifact.identifier else {
            return nil
        }
        let expected = Dictionary(
            uniqueKeysWithValues: manifest.files.map { ($0.path, $0) }
        )
        guard let enumerator = fileManager.enumerator(
            at: extracted,
            includingPropertiesForKeys: [
                .isRegularFileKey,
                .isDirectoryKey,
                .isSymbolicLinkKey,
            ],
            options: []
        ) else { return nil }
        var actualFiles = Set<String>()
        while let entry = enumerator.nextObject() as? URL {
            guard let values = try? entry.resourceValues(
                forKeys: [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey]
            ), values.isSymbolicLink != true else {
                return nil
            }
            let relative = String(entry.path.dropFirst(extracted.path.count + 1))
            if values.isDirectory == true {
                let isExpectedParent = expected.keys.contains {
                    $0.hasPrefix(relative + "/")
                }
                guard isExpectedParent else { return nil }
                continue
            }
            guard values.isRegularFile == true else { return nil }
            actualFiles.insert(relative)
        }
        let expectedFiles = Set(expected.keys).union([SafeTarArchive.manifestPath])
        guard actualFiles == expectedFiles else { return nil }
        for (relative, record) in expected {
            let file = extracted.appendingPathComponent(relative)
            guard let attributes = try? fileManager.attributesOfItem(atPath: file.path),
                  (attributes[.size] as? NSNumber)?.int64Value == record.size,
                  (try? SHA256Digest.hex(ofFile: file)) == record.sha256 else {
                return nil
            }
        }
        return manifest
    }

    private static func compiledDirectory(
        root: URL,
        artifact: CoreMLModelArtifact
    ) -> URL {
        let compatibility = [
            ProcessInfo.processInfo.operatingSystemVersionString,
            Bundle(identifier: "com.apple.CoreML")?
                .object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "unknown-coreml",
        ].joined(separator: "-").replacingOccurrences(
            of: "[^A-Za-z0-9._-]",
            with: "-",
            options: .regularExpression
        )
        return root
            .appendingPathComponent(artifact.cacheKey, isDirectory: true)
            .appendingPathComponent("compiled-\(compatibility).mlmodelc", isDirectory: true)
    }

    private static func publishAtomically(
        temporary: URL,
        destination: URL,
        fileManager: FileManager
    ) throws {
        do {
            try fileManager.moveItem(at: temporary, to: destination)
        } catch {
            // A concurrent process may have won the atomic rename.
            if fileManager.fileExists(atPath: destination.path) {
                return
            }
            throw error
        }
    }

    private static func defaultCacheDirectory() throws -> URL {
        let fileManager = FileManager.default
        guard let caches = fileManager.urls(for: .cachesDirectory, in: .userDomainMask).first else {
            throw EmbeddingKitError.backend("the OS caches directory is unavailable")
        }
        return caches
            .appendingPathComponent("dev.retrievalkit.EmbeddingKit", isDirectory: true)
            .appendingPathComponent("CoreML", isDirectory: true)
    }
}
#endif
