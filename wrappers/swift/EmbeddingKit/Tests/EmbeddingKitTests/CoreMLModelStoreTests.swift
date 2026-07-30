#if canImport(CoreML)
import Foundation
import XCTest
@testable import EmbeddingKit

final class CoreMLModelStoreTests: XCTestCase {
    func testColdAcquisitionThenCachedLocalOnlyLoad() async throws {
        let fixture = try ArchiveFixture()
        let downloader = FixtureDownloader(source: fixture.archive)
        let compiler = FixtureCompiler()
        let store = CoreMLModelStore(downloader: downloader, compiler: compiler)
        let cache = temporaryDirectory()

        let cold = try await store.prepare(
            artifact: fixture.artifact,
            access: .downloadIfNeeded,
            cacheDirectory: cache
        )
        let cached = try await store.prepare(
            artifact: fixture.artifact,
            access: .localOnly,
            cacheDirectory: cache
        )

        XCTAssertEqual(cold.modelURL, cached.modelURL)
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: cached.tokenizerURL.appendingPathComponent("tokenizer.json").path
        ))
        let downloadCount = await downloader.count
        let compileCount = await compiler.count
        XCTAssertEqual(downloadCount, 1)
        XCTAssertEqual(compileCount, 1)
    }

    func testLocalOnlyDoesNotAttemptNetwork() async throws {
        let fixture = try ArchiveFixture()
        let downloader = FixtureDownloader(source: fixture.archive)
        let store = CoreMLModelStore(
            downloader: downloader,
            compiler: FixtureCompiler()
        )

        do {
            _ = try await store.prepare(
                artifact: fixture.artifact,
                access: .localOnly,
                cacheDirectory: temporaryDirectory()
            )
            XCTFail("expected local-only failure")
        } catch {
            guard case EmbeddingKitError.modelUnavailable = error else {
                return XCTFail("unexpected error \(error)")
            }
        }
        let downloadCount = await downloader.count
        XCTAssertEqual(downloadCount, 0)
    }

    func testConcurrentPreparationDeduplicatesDownloadAndCompilation() async throws {
        let fixture = try ArchiveFixture()
        let downloader = FixtureDownloader(source: fixture.archive, delayNanoseconds: 20_000_000)
        let compiler = FixtureCompiler()
        let store = CoreMLModelStore(downloader: downloader, compiler: compiler)
        let cache = temporaryDirectory()

        try await withThrowingTaskGroup(of: PreparedCoreMLModel.self) { group in
            for _ in 0..<12 {
                group.addTask {
                    try await store.prepare(
                        artifact: fixture.artifact,
                        access: .downloadIfNeeded,
                        cacheDirectory: cache
                    )
                }
            }
            for try await prepared in group {
                XCTAssertTrue(FileManager.default.fileExists(atPath: prepared.modelURL.path))
            }
        }
        let downloadCount = await downloader.count
        let compileCount = await compiler.count
        XCTAssertEqual(downloadCount, 1)
        XCTAssertEqual(compileCount, 1)
    }

    func testWrongArchiveHashAndSizeAreRejectedWithoutPartialPublication() async throws {
        let fixture = try ArchiveFixture()
        let cache = temporaryDirectory()
        let wrongHash = try CoreMLModelArtifact(
            identifier: fixture.artifact.identifier,
            sourceModelRevision: fixture.artifact.sourceModelRevision,
            artifactRevision: "wrong-hash",
            archiveURL: fixture.artifact.archiveURL,
            archiveSHA256: String(repeating: "a", count: 64),
            archiveByteCount: fixture.artifact.archiveByteCount,
            manifestSHA256: fixture.artifact.manifestSHA256
        )
        let store = CoreMLModelStore(
            downloader: FixtureDownloader(source: fixture.archive),
            compiler: FixtureCompiler()
        )
        await XCTAssertThrowsErrorAsync {
            _ = try await store.prepare(
                artifact: wrongHash,
                access: .downloadIfNeeded,
                cacheDirectory: cache
            )
        }
        XCTAssertFalse(try containsPublishedArchive(cache))

        let wrongSize = try CoreMLModelArtifact(
            identifier: fixture.artifact.identifier,
            sourceModelRevision: fixture.artifact.sourceModelRevision,
            artifactRevision: "wrong-size",
            archiveURL: fixture.artifact.archiveURL,
            archiveSHA256: fixture.artifact.archiveSHA256,
            archiveByteCount: fixture.artifact.archiveByteCount + 1,
            manifestSHA256: fixture.artifact.manifestSHA256
        )
        await XCTAssertThrowsErrorAsync {
            _ = try await store.prepare(
                artifact: wrongSize,
                access: .downloadIfNeeded,
                cacheDirectory: cache
            )
        }
        XCTAssertFalse(try containsPublishedArchive(cache))
    }

    func testInterruptedDownloadIsCleanedUpAndRetrySucceeds() async throws {
        let fixture = try ArchiveFixture()
        let downloader = FixtureDownloader(source: fixture.archive, failuresRemaining: 1)
        let store = CoreMLModelStore(downloader: downloader, compiler: FixtureCompiler())
        let cache = temporaryDirectory()

        await XCTAssertThrowsErrorAsync {
            _ = try await store.prepare(
                artifact: fixture.artifact,
                access: .downloadIfNeeded,
                cacheDirectory: cache
            )
        }
        XCTAssertFalse(try containsTemporaryFile(cache))

        _ = try await store.prepare(
            artifact: fixture.artifact,
            access: .downloadIfNeeded,
            cacheDirectory: cache
        )
        let downloadCount = await downloader.count
        XCTAssertEqual(downloadCount, 2)
        XCTAssertFalse(try containsTemporaryFile(cache))
    }

    func testCorruptCompiledCacheCanBeRemovedAndRecompiled() async throws {
        let fixture = try ArchiveFixture()
        let compiler = FixtureCompiler()
        let store = CoreMLModelStore(
            downloader: FixtureDownloader(source: fixture.archive),
            compiler: compiler
        )
        let cache = temporaryDirectory()
        let first = try await store.prepare(
            artifact: fixture.artifact,
            access: .downloadIfNeeded,
            cacheDirectory: cache
        )
        try Data("corrupt".utf8).write(
            to: first.modelURL.appendingPathComponent("compiled.marker"),
            options: .atomic
        )

        try await store.removeCompiledCache(
            artifact: fixture.artifact,
            cacheDirectory: cache
        )
        let repaired = try await store.prepare(
            artifact: fixture.artifact,
            access: .localOnly,
            cacheDirectory: cache
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: repaired.modelURL.path))
        let compileCount = await compiler.count
        XCTAssertEqual(compileCount, 2)
    }

    func testCorruptOrUnexpectedExtractedCacheIsAtomicallyReplaced() async throws {
        let fixture = try ArchiveFixture()
        let store = CoreMLModelStore(
            downloader: FixtureDownloader(source: fixture.archive),
            compiler: FixtureCompiler()
        )
        let cache = temporaryDirectory()
        let first = try await store.prepare(
            artifact: fixture.artifact,
            access: .downloadIfNeeded,
            cacheDirectory: cache
        )
        let tokenizerJSON = first.tokenizerURL.appendingPathComponent("tokenizer.json")
        try Data("corrupt".utf8).write(to: tokenizerJSON, options: .atomic)
        try Data("unexpected".utf8).write(
            to: first.tokenizerURL.deletingLastPathComponent()
                .appendingPathComponent("unexpected.txt")
        )

        let repaired = try await store.prepare(
            artifact: fixture.artifact,
            access: .localOnly,
            cacheDirectory: cache
        )
        XCTAssertEqual(try Data(contentsOf: repaired.tokenizerURL
            .appendingPathComponent("tokenizer.json")), Data("{}".utf8))
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: repaired.tokenizerURL.deletingLastPathComponent()
                .appendingPathComponent("unexpected.txt").path
        ))
    }

    func testSafeExtractorRejectsTraversalLinksDuplicatesAndUnexpectedFiles() throws {
        for mutation in [
            TarMutation.traversal,
            .symbolicLink,
            .duplicate,
            .unexpected,
        ] {
            let fixture = try ArchiveFixture(mutation: mutation)
            XCTAssertThrowsError(
                try SafeTarArchive.extract(
                    archiveURL: fixture.archive,
                    to: temporaryDirectory(),
                    expectedArtifactID: fixture.artifact.identifier,
                    expectedManifestSHA256: fixture.artifact.manifestSHA256
                ),
                "mutation \(mutation) should be rejected"
            ) { error in
                guard case EmbeddingKitError.unsafeArchive = error else {
                    return XCTFail("unexpected error \(error)")
                }
            }
        }
    }

    func testProductionMetadataIsFP32ContractNotDatabaseEncoding() {
        let artifact = CoreMLProductionModels.allMiniLML6V2FP32
        XCTAssertEqual(artifact.sourceModelRevision, "c9745ed1d9f207416be6d2e6f8de32d1f16199bf")
        XCTAssertTrue(artifact.identifier.contains("fp32"))
        XCTAssertFalse(artifact.identifier.lowercased().contains("i8"))
    }

    func testLivePinnedArtifactColdCachedLocalOnlyAndInference() async throws {
        guard ProcessInfo.processInfo.environment["EMBEDDINGKIT_LIVE_MODEL_TEST"] == "1" else {
            throw XCTSkip("set EMBEDDINGKIT_LIVE_MODEL_TEST=1 to exercise the immutable public artifact")
        }
        let cache = ProcessInfo.processInfo.environment["EMBEDDINGKIT_LIVE_CACHE_DIRECTORY"]
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? temporaryDirectory()
        let start = ContinuousClock.now
        try await CoreMLEmbedder.prefetch(cacheDirectory: cache)
        let prefetchDuration = start.duration(to: .now)
        let cachedStart = ContinuousClock.now
        let embedder = try await CoreMLEmbedder.load(
            access: .localOnly,
            cacheDirectory: cache
        )
        let cachedDuration = cachedStart.duration(to: .now)
        let query = Array(repeating: "retrieval", count: 30).joined(separator: " ")
        let tokenCount = try XCTUnwrap(embedder.tokenCounter).countTokens(in: query)
        let firstStart = ContinuousClock.now
        let embedding = try await embedder.embed(query)
        let firstDuration = firstStart.duration(to: .now)

        XCTAssertEqual(embedding.count, 384)
        XCTAssertTrue(embedding.allSatisfy(\.isFinite))
        XCTAssertEqual(
            sqrt(embedding.reduce(Float(0)) { $0 + $1 * $1 }),
            1,
            accuracy: 1e-5
        )
        XCTAssertEqual(tokenCount, 32)

        let benchmark = try await EmbeddingBenchmark.run(
            embedder: embedder,
            queries: [query],
            config: try EmbeddingBenchmarkConfig(
                warmupIterations: 50,
                measuredIterations: 750,
                batchSizes: [1]
            )
        )
        let firstMilliseconds = durationMilliseconds(firstDuration)
        let prefetchMilliseconds = durationMilliseconds(prefetchDuration)
        let cachedMilliseconds = durationMilliseconds(cachedDuration)
        let warmP95 = benchmark.singleQueryLatency.p95Milliseconds
        print(
            "Core ML FP32 .all release qualification: tokens=\(tokenCount), "
                + "prefetch_ms=\(prefetchMilliseconds), "
                + "cached_load_ms=\(cachedMilliseconds), "
                + "first_inference_ms=\(firstMilliseconds), "
                + "warm_embedding_p95_ms=\(warmP95), warmups=50, measured=750"
        )
    }
}

private actor FixtureDownloader: CoreMLArtifactDownloading {
    let source: URL
    let delayNanoseconds: UInt64
    var failuresRemaining: Int
    var count = 0

    init(source: URL, delayNanoseconds: UInt64 = 0, failuresRemaining: Int = 0) {
        self.source = source
        self.delayNanoseconds = delayNanoseconds
        self.failuresRemaining = failuresRemaining
    }

    func download(from url: URL, to temporaryURL: URL) async throws {
        count += 1
        if delayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: delayNanoseconds)
        }
        if failuresRemaining > 0 {
            failuresRemaining -= 1
            try Data("partial".utf8).write(to: temporaryURL)
            throw EmbeddingKitError.backend("simulated interruption")
        }
        try FileManager.default.copyItem(at: source, to: temporaryURL)
    }
}

private actor FixtureCompiler: CoreMLModelCompiling {
    var count = 0

    func compile(packageAt url: URL) async throws -> URL {
        count += 1
        let directory = temporaryDirectory().appendingPathComponent(
            "fixture.mlmodelc",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data("compiled".utf8).write(
            to: directory.appendingPathComponent("compiled.marker")
        )
        return directory
    }
}

private enum TarMutation: CustomStringConvertible {
    case none
    case traversal
    case symbolicLink
    case duplicate
    case unexpected

    var description: String {
        switch self {
        case .none: "none"
        case .traversal: "traversal"
        case .symbolicLink: "symbolicLink"
        case .duplicate: "duplicate"
        case .unexpected: "unexpected"
        }
    }
}

private struct ArchiveFixture {
    let archive: URL
    let artifact: CoreMLModelArtifact

    init(mutation: TarMutation = .none) throws {
        let artifactID = "sentence-transformers/all-MiniLM-L6-v2-coreml-fp32"
        let normalFiles: [(String, Data)] = [
            ("model.mlpackage/Manifest.json", Data("{}".utf8)),
            ("model.mlpackage/Data/com.apple.CoreML/model.mlmodel", Data("model".utf8)),
            ("tokenizer/tokenizer.json", Data("{}".utf8)),
            ("LICENSE", Data("license".utf8)),
            ("NOTICE", Data("notice".utf8)),
        ]
        let records = normalFiles.map {
            CoreMLArchiveManifest.File(
                path: $0.0,
                size: Int64($0.1.count),
                sha256: SHA256Digest.hex(of: $0.1)
            )
        }
        let tree = records.sorted { $0.path < $1.path }.map {
            "\($0.path)\0\($0.size)\0\($0.sha256)\n"
        }.joined()
        let manifest = CoreMLArchiveManifest(
            schemaVersion: 1,
            artifactID: artifactID,
            modelPath: "model.mlpackage",
            tokenizerPath: "tokenizer",
            canonicalTreeSHA256: SHA256Digest.hex(of: Data(tree.utf8)),
            files: records
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let manifestData = try encoder.encode(manifest)
        var entries = [(SafeTarArchive.manifestPath, manifestData, UInt8(48))]
        entries.append(contentsOf: normalFiles.map { ($0.0, $0.1, UInt8(48)) })
        switch mutation {
        case .none:
            break
        case .traversal:
            entries.append(("../outside", Data(), 48))
        case .symbolicLink:
            entries.append(("unsafe-link", Data(), 50))
        case .duplicate:
            entries.append(entries[1])
        case .unexpected:
            entries.append(("unexpected.txt", Data("no".utf8), 48))
        }

        let archiveData = makeTar(entries)
        archive = temporaryDirectory().appendingPathComponent("fixture.tar")
        try archiveData.write(to: archive)
        artifact = try CoreMLModelArtifact(
            identifier: artifactID,
            sourceModelRevision: "source-revision",
            artifactRevision: UUID().uuidString,
            archiveURL: URL(string: "https://example.invalid/fixture.tar")!,
            archiveSHA256: SHA256Digest.hex(of: archiveData),
            archiveByteCount: Int64(archiveData.count),
            manifestSHA256: SHA256Digest.hex(of: manifestData)
        )
    }
}

private func makeTar(_ entries: [(String, Data, UInt8)]) -> Data {
    var archive = Data()
    for (path, payload, type) in entries {
        var header = [UInt8](repeating: 0, count: 512)
        writeTarString(path, to: &header, offset: 0, count: 100)
        writeTarOctal(0o644, to: &header, offset: 100, count: 8)
        writeTarOctal(0, to: &header, offset: 108, count: 8)
        writeTarOctal(0, to: &header, offset: 116, count: 8)
        writeTarOctal(payload.count, to: &header, offset: 124, count: 12)
        writeTarOctal(0, to: &header, offset: 136, count: 12)
        for index in 148..<156 { header[index] = 32 }
        header[156] = type
        writeTarString("ustar", to: &header, offset: 257, count: 6)
        writeTarString("00", to: &header, offset: 263, count: 2)
        let checksum = header.reduce(0) { $0 + Int($1) }
        let checksumText = String(format: "%06o", checksum)
        writeTarString(checksumText, to: &header, offset: 148, count: 6)
        header[154] = 0
        header[155] = 32
        archive.append(contentsOf: header)
        archive.append(payload)
        let padding = (512 - payload.count % 512) % 512
        archive.append(Data(repeating: 0, count: padding))
    }
    archive.append(Data(repeating: 0, count: 1024))
    return archive
}

private func writeTarString(
    _ value: String,
    to bytes: inout [UInt8],
    offset: Int,
    count: Int
) {
    for (index, byte) in value.utf8.prefix(count).enumerated() {
        bytes[offset + index] = byte
    }
}

private func writeTarOctal(
    _ value: Int,
    to bytes: inout [UInt8],
    offset: Int,
    count: Int
) {
    let text = String(format: "%0*o", count - 1, value)
    writeTarString(text, to: &bytes, offset: offset, count: count - 1)
}

private func temporaryDirectory() -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try! FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

private func containsPublishedArchive(_ root: URL) throws -> Bool {
    guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)
    else { return false }
    return enumerator.compactMap { $0 as? URL }.contains { $0.lastPathComponent == "artifact.tar" }
}

private func containsTemporaryFile(_ root: URL) throws -> Bool {
    guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)
    else { return false }
    return enumerator.compactMap { $0 as? URL }.contains {
        $0.lastPathComponent.hasPrefix(".download-")
            || $0.lastPathComponent.hasPrefix(".extract-")
    }
}

private func XCTAssertThrowsErrorAsync(
    _ expression: () async throws -> Void,
    file: StaticString = #filePath,
    line: UInt = #line
) async {
    do {
        try await expression()
        XCTFail("expected error", file: file, line: line)
    } catch {
        // Expected.
    }
}

private func durationMilliseconds(_ duration: Duration) -> Double {
    let components = duration.components
    return Double(components.seconds) * 1_000
        + Double(components.attoseconds) / 1_000_000_000_000_000
}
#endif
