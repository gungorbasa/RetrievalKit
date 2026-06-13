import Foundation

/// Benchmark configuration shared by embedding providers.
public struct EmbeddingBenchmarkConfig: Codable, Equatable, Sendable {
    public var warmupIterations: Int
    public var measuredIterations: Int
    public var batchSizes: [Int]

    public init(
        warmupIterations: Int = 50,
        measuredIterations: Int = 750,
        batchSizes: [Int] = [1, 8, 16, 32, 64]
    ) throws {
        guard warmupIterations >= 0 else {
            throw EmbeddingKitError.invalidBenchmarkConfiguration(
                "warmupIterations must be greater than or equal to zero"
            )
        }
        guard measuredIterations > 0 else {
            throw EmbeddingKitError.invalidBenchmarkConfiguration(
                "measuredIterations must be greater than zero"
            )
        }
        guard !batchSizes.isEmpty, batchSizes.allSatisfy({ $0 > 0 }) else {
            throw EmbeddingKitError.invalidBenchmarkConfiguration(
                "batchSizes must contain positive values"
            )
        }

        self.warmupIterations = warmupIterations
        self.measuredIterations = measuredIterations
        self.batchSizes = Array(Set(batchSizes)).sorted()
    }
}

/// Latency distribution in milliseconds.
public struct EmbeddingLatencyStats: Codable, Equatable, Sendable {
    public var minMilliseconds: Double
    public var meanMilliseconds: Double
    public var p50Milliseconds: Double
    public var p95Milliseconds: Double
    public var p99Milliseconds: Double
    public var maxMilliseconds: Double
    public var sampleCount: Int

    public init(samplesMilliseconds samples: [Double]) throws {
        guard !samples.isEmpty else {
            throw EmbeddingKitError.invalidBenchmarkConfiguration("latency samples cannot be empty")
        }

        let sorted = samples.sorted()
        let total = samples.reduce(0, +)
        self.minMilliseconds = sorted[0]
        self.meanMilliseconds = total / Double(samples.count)
        self.p50Milliseconds = percentile(sorted, 0.50)
        self.p95Milliseconds = percentile(sorted, 0.95)
        self.p99Milliseconds = percentile(sorted, 0.99)
        self.maxMilliseconds = sorted[sorted.count - 1]
        self.sampleCount = samples.count
    }
}

/// Batch benchmark result for one batch size.
public struct EmbeddingBatchBenchmarkResult: Codable, Equatable, Sendable {
    public var batchSize: Int
    public var latency: EmbeddingLatencyStats
    public var textsPerSecond: Double
}

/// Complete benchmark report for one embedder and one query set.
public struct EmbeddingBenchmarkReport: Codable, Equatable, Sendable {
    public var modelInfo: EmbeddingModelInfo
    public var runtimeInfo: EmbeddingRuntimeInfo
    public var queryCount: Int
    public var warmupIterations: Int
    public var measuredIterations: Int
    public var singleQueryLatency: EmbeddingLatencyStats
    public var batchResults: [EmbeddingBatchBenchmarkResult]
}

/// Runs deterministic embedding latency benchmarks.
public enum EmbeddingBenchmark {
    public static func run(
        embedder: any TextEmbedder,
        queries: [String],
        config: EmbeddingBenchmarkConfig = try! EmbeddingBenchmarkConfig()
    ) async throws -> EmbeddingBenchmarkReport {
        guard !queries.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }

        for iteration in 0..<config.warmupIterations {
            _ = try await embedder.embed(queries[iteration % queries.count])
        }

        var singleQuerySamples: [Double] = []
        singleQuerySamples.reserveCapacity(config.measuredIterations)
        for iteration in 0..<config.measuredIterations {
            let query = queries[iteration % queries.count]
            let started = DispatchTime.now()
            _ = try await embedder.embed(query)
            singleQuerySamples.append(elapsedMilliseconds(since: started))
        }

        var batchResults: [EmbeddingBatchBenchmarkResult] = []
        batchResults.reserveCapacity(config.batchSizes.count)
        for batchSize in config.batchSizes {
            batchResults.append(try await runBatch(
                embedder: embedder,
                queries: queries,
                batchSize: batchSize,
                warmupIterations: config.warmupIterations,
                measuredIterations: config.measuredIterations
            ))
        }

        return try EmbeddingBenchmarkReport(
            modelInfo: embedder.modelInfo,
            runtimeInfo: embedder.runtimeInfo,
            queryCount: queries.count,
            warmupIterations: config.warmupIterations,
            measuredIterations: config.measuredIterations,
            singleQueryLatency: EmbeddingLatencyStats(samplesMilliseconds: singleQuerySamples),
            batchResults: batchResults
        )
    }

    private static func runBatch(
        embedder: any TextEmbedder,
        queries: [String],
        batchSize: Int,
        warmupIterations: Int,
        measuredIterations: Int
    ) async throws -> EmbeddingBatchBenchmarkResult {
        for iteration in 0..<warmupIterations {
            _ = try await embedder.embed(batch(queries, size: batchSize, offset: iteration * batchSize))
        }

        var samples: [Double] = []
        samples.reserveCapacity(measuredIterations)
        for iteration in 0..<measuredIterations {
            let started = DispatchTime.now()
            _ = try await embedder.embed(batch(queries, size: batchSize, offset: iteration * batchSize))
            samples.append(elapsedMilliseconds(since: started))
        }

        let latency = try EmbeddingLatencyStats(samplesMilliseconds: samples)
        let meanSeconds = latency.meanMilliseconds / 1_000
        let throughput = meanSeconds > 0 ? Double(batchSize) / meanSeconds : 0
        return EmbeddingBatchBenchmarkResult(
            batchSize: batchSize,
            latency: latency,
            textsPerSecond: throughput
        )
    }

    private static func batch(_ queries: [String], size: Int, offset: Int) -> [String] {
        (0..<size).map { queries[(offset + $0) % queries.count] }
    }

    private static func elapsedMilliseconds(since started: DispatchTime) -> Double {
        let elapsedNanoseconds = DispatchTime.now().uptimeNanoseconds - started.uptimeNanoseconds
        return Double(elapsedNanoseconds) / 1_000_000
    }
}

private func percentile(_ sortedSamples: [Double], _ percentile: Double) -> Double {
    if sortedSamples.count == 1 {
        return sortedSamples[0]
    }

    let rawIndex = percentile * Double(sortedSamples.count - 1)
    let lowerIndex = Int(rawIndex.rounded(.down))
    let upperIndex = Int(rawIndex.rounded(.up))
    if lowerIndex == upperIndex {
        return sortedSamples[lowerIndex]
    }

    let fraction = rawIndex - Double(lowerIndex)
    return sortedSamples[lowerIndex] * (1 - fraction) + sortedSamples[upperIndex] * fraction
}
