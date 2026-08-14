import CryptoKit
import Darwin
import EmbeddingKit
import Foundation
import RetrievalKit
import RetrievalKitRuntimeDiagnostics

public enum AppleE2EBenchmark {
    public static func makeEmbedder(
        modelURL: URL,
        tokenizerURL: URL,
        backendPoolSize: Int = 1
    ) throws -> CoreMLEmbedder {
        let tokenizer = try BertWordPieceTokenizer(
            tokenizerJSON: tokenizerURL,
            sequenceLength: 256
        )
        return try CoreMLEmbedder(
            modelInfo: KnownEmbeddingModels.allMiniLML6V2,
            tokenizer: tokenizer,
            configuration: CoreMLModelConfiguration(
                modelURL: modelURL,
                compute: .all,
                backendPoolSize: backendPoolSize
            )
        )
    }

    public static func loadAndValidateQueries(
        from url: URL,
        embedder: CoreMLEmbedder
    ) throws -> QuerySuite {
        let suite = try JSONDecoder().decode(QuerySuite.self, from: Data(contentsOf: url))
        guard suite.schemaVersion == 1 else {
            throw AppleE2EBenchmarkError.invalidInput("unsupported query schema version")
        }
        guard suite.queries.count == 100 else {
            throw AppleE2EBenchmarkError.invalidInput("query suite must contain exactly 100 queries")
        }
        guard suite.schedule.count == 750 else {
            throw AppleE2EBenchmarkError.invalidInput("query schedule must contain exactly 750 entries")
        }
        let byID = Dictionary(uniqueKeysWithValues: suite.queries.map { ($0.id, $0) })
        guard byID.count == suite.queries.count else {
            throw AppleE2EBenchmarkError.invalidInput("query IDs must be unique")
        }
        guard suite.schedule.allSatisfy({ byID[$0] != nil }) else {
            throw AppleE2EBenchmarkError.invalidInput("query schedule contains an unknown ID")
        }
        guard let counter = embedder.tokenCounter else {
            throw AppleE2EBenchmarkError.invalidState("embedder does not expose a token counter")
        }
        var categoryCounts: [String: Int] = [:]
        var bucketCounts: [TokenBucket: Int] = [:]
        for query in suite.queries {
            let tokens = try counter.countTokens(in: query.text)
            guard tokens >= query.expectedTokenBucket.minimum,
                  tokens <= query.expectedTokenBucket.maximum,
                  tokens <= 256 else {
                throw AppleE2EBenchmarkError.invalidInput(
                    "\(query.id) has \(tokens) tokens outside expected "
                        + "\(query.expectedTokenBucket.minimum)-\(query.expectedTokenBucket.maximum)"
                )
            }
            categoryCounts[query.category, default: 0] += 1
            bucketCounts[query.expectedTokenBucket, default: 0] += 1
        }
        let expectedCategories = [
            "semantic_paraphrase": 40,
            "exact_name_or_identifier": 30,
            "semantic_plus_keyword": 20,
            "near_distractor_or_no_natural_match": 10,
        ]
        guard categoryCounts == expectedCategories else {
            throw AppleE2EBenchmarkError.invalidInput("query category distribution is not frozen V1")
        }
        let expectedBuckets = [
            TokenBucket(minimum: 1, maximum: 16): 20,
            TokenBucket(minimum: 17, maximum: 32): 35,
            TokenBucket(minimum: 33, maximum: 64): 25,
            TokenBucket(minimum: 65, maximum: 128): 15,
            TokenBucket(minimum: 129, maximum: 256): 5,
        ]
        guard bucketCounts == expectedBuckets else {
            throw AppleE2EBenchmarkError.invalidInput("query token-bucket distribution is not frozen V1")
        }
        return suite
    }

    public static func prepareIndex(
        corpusURL: URL,
        outputURL: URL,
        modelURL: URL,
        tokenizerURL: URL,
        expectedChunks: Int,
        contractWorkload: Bool = true
    ) async throws {
        func progress(_ message: String) {
            FileHandle.standardError.write(Data("\(message)\n".utf8))
        }
        guard !contractWorkload || expectedChunks == 10_000 || expectedChunks == 50_000 || expectedChunks == 100_000 else {
            throw AppleE2EBenchmarkError.invalidArgument("expected chunks must be 10000, 50000, or 100000")
        }
        let embedder = try makeEmbedder(
            modelURL: modelURL,
            tokenizerURL: tokenizerURL,
            backendPoolSize: 4
        )
        progress("loaded Core ML embedder")
        let records = try readCorpus(corpusURL)
        progress("loaded \(records.count) corpus records")
        let corpusChunkCount = records.reduce(0) { $0 + $1.chunks.count }
        guard corpusChunkCount == expectedChunks else {
            throw AppleE2EBenchmarkError.invalidInput(
                "corpus has \(corpusChunkCount) chunks, expected \(expectedChunks)"
            )
        }
        guard !FileManager.default.fileExists(atPath: outputURL.path) else {
            throw AppleE2EBenchmarkError.invalidState("output index directory already exists")
        }
        let index = try VectorIndex(dimension: 384, metric: .cosine, encoding: .i8ScalarQuantized)
        progress("created empty I8 vector index")
        // Preparation is excluded from query timing and uses a small backend
        // pool so building the three frozen corpora remains practical.
        let recordBatchSize = 32
        for batchStart in stride(from: 0, to: records.count, by: recordBatchSize) {
            let batchEnd = min(batchStart + recordBatchSize, records.count)
            let batch = records[batchStart..<batchEnd]
            guard !contractWorkload || batch.allSatisfy({ $0.chunks.count == 4 }) else {
                throw AppleE2EBenchmarkError.invalidInput("every record must have four chunks")
            }
            let embeddings = try await embedder.embed(batch.flatMap { $0.chunks.map(\.text) })
            var embeddingOffset = 0
            for record in batch {
                let recordEmbeddings = embeddings[
                    embeddingOffset..<(embeddingOffset + record.chunks.count)
                ]
                embeddingOffset += record.chunks.count
                let metadata = record.metadata.mapValues(MetadataValue.string)
                let chunks = zip(record.chunks, recordEmbeddings).map { chunk, embedding in
                    ChunkInput(
                        text: chunk.text,
                        embedding: embedding,
                        metadata: ["chunk_key": .string(chunk.chunkKey)]
                    )
                }
                try await index.upsert(
                    document: Document(
                        id: DocumentID(record.recordID),
                        text: record.text,
                        metadata: metadata
                    ),
                    chunks: chunks
                )
            }
            if batchEnd.isMultiple(of: 256) || batchEnd == records.count {
                progress("prepared \(batchEnd)/\(records.count) records")
            }
        }
        guard await index.activeChunkCount == expectedChunks else {
            throw AppleE2EBenchmarkError.invalidState("prepared index active chunk count mismatch")
        }
        try await index.save(to: outputURL, includeBM25: true)
        try VectorIndex.validate(at: outputURL)
        let reloaded = try VectorIndex.load(from: outputURL)
        guard await reloaded.dimension == 384, await reloaded.activeChunkCount == expectedChunks else {
            throw AppleE2EBenchmarkError.invalidState("reloaded index invariants failed")
        }
    }

    public static func run(_ configuration: RunConfiguration) async throws -> BenchmarkReport {
        let embedder = try makeEmbedder(
            modelURL: configuration.modelURL,
            tokenizerURL: configuration.tokenizerURL
        )
        let suite = try loadAndValidateQueries(from: configuration.queriesURL, embedder: embedder)
        let queries = Dictionary(uniqueKeysWithValues: suite.queries.map { ($0.id, $0) })
        let index = try VectorIndex.load(from: configuration.indexURL)
        guard await index.dimension == 384 else {
            throw AppleE2EBenchmarkError.invalidState("index dimension is not 384")
        }
        let activeChunks = await index.activeChunkCount
        guard [10_000, 50_000, 100_000].contains(activeChunks) else {
            throw AppleE2EBenchmarkError.invalidState("index active chunk count is not a contract workload")
        }
        let capturedEnvironment = try environment(configuration: configuration, embedder: embedder)

        for queryID in suite.schedule.prefix(50) {
            try checkAbort(configuration)
            guard let query = queries[queryID] else { preconditionFailure("validated query missing") }
            _ = try await execute(query: query, mode: configuration.mode, embedder: embedder, index: index)
        }

        var samples: [RawSample] = []
        samples.reserveCapacity(750)
        for (ordinal, queryID) in suite.schedule.enumerated() {
            try checkAbort(configuration)
            guard let query = queries[queryID] else { preconditionFailure("validated query missing") }
            let measurement = try await execute(
                query: query,
                mode: configuration.mode,
                embedder: embedder,
                index: index
            )
            samples.append(RawSample(
                ordinal: ordinal,
                queryID: queryID,
                startClockNS: measurement.start,
                endClockNS: measurement.end,
                embeddingNS: measurement.embedding,
                retrievalNS: measurement.retrieval,
                endToEndNS: measurement.endToEnd,
                resultCount: measurement.identities.count,
                topResultIdentity: measurement.identities.first,
                resultIdentityDigest: identityDigest(measurement.identities)
            ))
        }

        let report = BenchmarkReport(
            schemaVersion: 1,
            contractVersion: configuration.contractVersion,
            workloadID: configuration.workloadID,
            workloadClassification: configuration.workloadClassification,
            marketingEligible: configuration.marketingEligible,
            supportedV1CapacityChanged: false,
            profileID: configuration.profileID,
            profileClassification: configuration.profileClassification,
            sessionID: configuration.sessionID,
            searchMode: configuration.mode,
            topK: 10,
            warmupCount: 50,
            samples: samples,
            summaries: [
                "embedding_total": try StageSummary.calculate(samples.map(\.embeddingNS)),
                "retrieval_total": try StageSummary.calculate(samples.map(\.retrievalNS)),
                "end_to_end_text_search": try StageSummary.calculate(samples.map(\.endToEndNS)),
            ],
            environment: capturedEnvironment,
            iphoneValidity: configuration.iphoneValidityProvider?()
        )
        try writeAtomically(report, to: configuration.outputURL)
        return report
    }

    public static func compareQuality(
        queriesURL: URL,
        referenceModelURL: URL,
        referenceTokenizerURL: URL,
        referenceIndexURL: URL,
        candidateModelURL: URL,
        candidateTokenizerURL: URL,
        candidateIndexURL: URL,
        outputURL: URL
    ) async throws -> QualityReport {
        let reference = try makeEmbedder(
            modelURL: referenceModelURL,
            tokenizerURL: referenceTokenizerURL
        )
        let candidate = try makeEmbedder(
            modelURL: candidateModelURL,
            tokenizerURL: candidateTokenizerURL
        )
        let suite = try JSONDecoder().decode(
            ProviderQualitySuite.self,
            from: Data(contentsOf: queriesURL)
        )
        guard suite.schemaVersion == 1, suite.queries.count == 42, suite.diagnostics.count == 4 else {
            throw AppleE2EBenchmarkError.invalidInput("provider conformance fixture population drifted")
        }
        let referenceIndex = try VectorIndex.load(from: referenceIndexURL)
        let candidateIndex = try VectorIndex.load(from: candidateIndexURL)
        var samples: [QualitySample] = []
        samples.reserveCapacity(suite.queries.count)
        for query in suite.queries {
            async let referenceEmbeddingTask = reference.embed(query.text)
            async let candidateEmbeddingTask = candidate.embed(query.text)
            let (referenceEmbedding, candidateEmbedding) = try await (
                referenceEmbeddingTask, candidateEmbeddingTask
            )
            async let referenceResultsTask = referenceIndex.search(
                embedding: referenceEmbedding, topK: 10
            )
            async let candidateResultsTask = candidateIndex.search(
                embedding: candidateEmbedding, topK: 10
            )
            let (referenceResults, candidateResults) = try await (
                referenceResultsTask, candidateResultsTask
            )
            let referenceIDs = referenceResults.map { "\($0.documentID):\($0.chunkID)" }
            let candidateIDs = candidateResults.map { "\($0.documentID):\($0.chunkID)" }
            let overlap = Set(referenceIDs).intersection(Set(candidateIDs)).count
            samples.append(QualitySample(
                queryID: query.id,
                cosine: embeddingCosine(referenceEmbedding, candidateEmbedding),
                top10Overlap: Double(overlap) / 10.0,
                exactTop10: referenceIDs == candidateIDs
            ))
        }
        let cosines = samples.map(\.cosine).sorted()
        let overlaps = samples.map(\.top10Overlap)
        let middle = cosines.count / 2
        let medianCosine = cosines.count.isMultiple(of: 2)
            ? (cosines[middle - 1] + cosines[middle]) / 2
            : cosines[middle]
        let meanOverlap = overlaps.reduce(0, +) / Double(overlaps.count)
        let minimumOverlap = overlaps.min() ?? 0
        let exactRate = Double(samples.filter(\.exactTop10).count) / Double(samples.count)
        let report = QualityReport(
            schemaVersion: 1,
            queryCount: samples.count,
            medianCosine: medianCosine,
            meanTop10Overlap: meanOverlap,
            minimumTop10Overlap: minimumOverlap,
            exactTop10Rate: exactRate,
            passed: medianCosine >= 0.995 && meanOverlap >= 0.95 && minimumOverlap >= 0.80,
            samples: samples
        )
        try writeAtomically(report, to: outputURL)
        return report
    }

    private static func embeddingCosine(_ left: [Float], _ right: [Float]) -> Double {
        precondition(left.count == right.count)
        var dot = 0.0
        var leftNorm = 0.0
        var rightNorm = 0.0
        for (lhs, rhs) in zip(left, right) {
            let leftValue = Double(lhs)
            let rightValue = Double(rhs)
            dot += leftValue * rightValue
            leftNorm += leftValue * leftValue
            rightNorm += rightValue * rightValue
        }
        return dot / sqrt(leftNorm * rightNorm)
    }

    private static func checkAbort(_ configuration: RunConfiguration) throws {
        if let reason = configuration.abortCheck?() {
            throw AppleE2EBenchmarkError.invalidState("benchmark validity abort: \(reason)")
        }
    }

    private struct Measurement {
        let start: UInt64
        let end: UInt64
        let embedding: UInt64
        let retrieval: UInt64
        let endToEnd: UInt64
        let identities: [String]
    }

    private static func execute(
        query: BenchmarkQuery,
        mode: SearchMode,
        embedder: CoreMLEmbedder,
        index: VectorIndex
    ) async throws -> Measurement {
        let totalStart = DispatchTime.now().uptimeNanoseconds
        let embeddingStart = DispatchTime.now().uptimeNanoseconds
        let embedding = try await embedder.embed(query.text)
        let embeddingEnd = DispatchTime.now().uptimeNanoseconds
        let retrievalStart = DispatchTime.now().uptimeNanoseconds
        let identities: [String]
        switch mode {
        case .vector:
            let results = try await index.search(embedding: embedding, topK: 10)
            identities = results.map { "\($0.documentID):\($0.chunkID)" }
        case .weightedHybrid:
            let results = try await index.hybridSearch(
                text: query.text,
                embedding: embedding,
                topK: 10,
                alpha: 0.6,
                options: HybridOptions(vectorTopK: 50, keywordTopK: 50)
            )
            identities = results.map { "\($0.documentID):\($0.chunkID)" }
        }
        let retrievalEnd = DispatchTime.now().uptimeNanoseconds
        let totalEnd = DispatchTime.now().uptimeNanoseconds
        guard identities.count == 10 else {
            throw AppleE2EBenchmarkError.invalidState("search returned \(identities.count) results, expected 10")
        }
        return Measurement(
            start: totalStart,
            end: totalEnd,
            embedding: embeddingEnd - embeddingStart,
            retrieval: retrievalEnd - retrievalStart,
            endToEnd: totalEnd - totalStart,
            identities: identities
        )
    }

    private static func readCorpus(_ url: URL) throws -> [CorpusRecord] {
        let data = try Data(contentsOf: url, options: [.mappedIfSafe])
        var records: [CorpusRecord] = []
        for line in data.split(separator: 0x0A) where !line.isEmpty {
            records.append(try JSONDecoder().decode(CorpusRecord.self, from: Data(line)))
        }
        return records
    }

    private static func identityDigest(_ identities: [String]) -> String {
        SHA256.hash(data: Data(identities.joined(separator: "\n").utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func sha256File(_ url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let data = try handle.read(upToCount: 1024 * 1024), !data.isEmpty {
            hasher.update(data: data)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func canonicalTreeStats(_ root: URL) throws -> (bytes: UInt64, sha256: String) {
        let normalizedRoot = root.resolvingSymlinksInPath().standardizedFileURL
        guard let enumerator = FileManager.default.enumerator(
            at: normalizedRoot,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]
        ) else {
            throw AppleE2EBenchmarkError.invalidInput("cannot enumerate \(root.path)")
        }
        var files: [(relative: String, url: URL, bytes: UInt64)] = []
        while let url = enumerator.nextObject() as? URL {
            let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
            if values.isSymbolicLink == true {
                throw AppleE2EBenchmarkError.invalidInput("symbolic link in evidence tree: \(url.path)")
            }
            if values.isRegularFile == true {
                let normalizedURL = url.resolvingSymlinksInPath().standardizedFileURL
                let prefix = normalizedRoot.path.hasSuffix("/")
                    ? normalizedRoot.path : normalizedRoot.path + "/"
                guard normalizedURL.path.hasPrefix(prefix) else {
                    throw AppleE2EBenchmarkError.invalidInput("file escaped evidence tree")
                }
                files.append((
                    String(normalizedURL.path.dropFirst(prefix.count)),
                    normalizedURL,
                    UInt64(values.fileSize ?? 0)
                ))
            }
        }
        var hasher = SHA256()
        var total: UInt64 = 0
        for file in files.sorted(by: { $0.relative < $1.relative }) {
            let line = "\(file.relative)\0\(file.bytes)\0\(try sha256File(file.url))\n"
            hasher.update(data: Data(line.utf8))
            total += file.bytes
        }
        return (total, hasher.finalize().map { String(format: "%02x", $0) }.joined())
    }

    private static func writeAtomically<T: Encodable>(_ value: T, to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(value) + Data("\n".utf8)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let temporary = url.deletingLastPathComponent()
            .appendingPathComponent(".\(url.lastPathComponent).\(UUID().uuidString).tmp")
        try data.write(to: temporary, options: .withoutOverwriting)
        try FileManager.default.moveItem(at: temporary, to: url)
    }

    private static func environment(
        configuration: RunConfiguration,
        embedder: CoreMLEmbedder
    ) throws -> BenchmarkEnvironment {
        #if os(iOS)
        let platform = "iphone"
        #else
        let platform = "mac"
        #endif
        let capabilities = try RetrievalKitRuntimeDiagnostics.capabilities()
        let modelTree = try canonicalTreeStats(configuration.modelURL)
        let indexTree = try canonicalTreeStats(configuration.indexURL)
        return BenchmarkEnvironment(
            platform: platform,
            hardware: machineIdentifier(),
            operatingSystem: ProcessInfo.processInfo.operatingSystemVersionString,
            architecture: "arm64",
            processID: ProcessInfo.processInfo.processIdentifier,
            debuggerAttached: debuggerAttached(),
            graphLinked: false,
            onnxRuntimeLinked: false,
            retrievalKitRevision: configuration.retrievalKitRevision,
            modelPath: configuration.modelURL.path,
            indexPath: configuration.indexURL.path,
            runtimeName: embedder.runtimeInfo.name,
            requestedCompute: embedder.runtimeInfo.requestedCompute.rawValue,
            selectedSIMDBackend: capabilities.simsimd,
            aarch64Dotprod: capabilities.aarch64Dotprod,
            compiledModelTreeSHA256: modelTree.sha256,
            indexTreeSHA256: indexTree.sha256,
            indexTreeBytes: indexTree.bytes,
            querySuiteSHA256: try sha256File(configuration.queriesURL)
        )
    }

    private static func machineIdentifier() -> String {
        #if os(macOS)
        var size = 0
        guard sysctlbyname("hw.model", nil, &size, nil, 0) == 0, size > 0 else {
            return "unknown"
        }
        var bytes = [CChar](repeating: 0, count: size)
        guard sysctlbyname("hw.model", &bytes, &size, nil, 0) == 0 else {
            return "unknown"
        }
        let count = bytes.firstIndex(of: 0) ?? bytes.endIndex
        return String(decoding: bytes[..<count].map(UInt8.init(bitPattern:)), as: UTF8.self)
        #else
        var system = utsname()
        uname(&system)
        return withUnsafePointer(to: &system.machine) {
            $0.withMemoryRebound(to: CChar.self, capacity: 1) { String(cString: $0) }
        }
        #endif
    }

    private static func debuggerAttached() -> Bool {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        let nameCount = UInt32(name.count)
        let result = name.withUnsafeMutableBufferPointer { pointer in
            sysctl(pointer.baseAddress, nameCount, &info, &size, nil, 0)
        }
        return result == 0 && (info.kp_proc.p_flag & P_TRACED) != 0
    }
}
