import CRetrievalKitFFI
import EmbeddingKit
import Foundation

enum BenchHarnessError: Error, CustomStringConvertible {
    case missingValue(String)
    case unknownArgument(String)
    case unreadableConfigFile(String)
    case invalidUtf8Result
    case nullResult
    case invalidRealBenchmark(String)
    case retrievalKit(String)

    var description: String {
        switch self {
        case .missingValue(let flag):
            return "missing value for \(flag)"
        case .unknownArgument(let argument):
            return "unknown argument \(argument)"
        case .unreadableConfigFile(let path):
            return "could not read config file at \(path)"
        case .invalidUtf8Result:
            return "benchmark returned invalid UTF-8"
        case .nullResult:
            return "benchmark returned a null result pointer"
        case .invalidRealBenchmark(let message):
            return message
        case .retrievalKit(let message):
            return message
        }
    }
}

struct CommandLineOptions {
    var configJSON: String?
    var realIndexPath: String?
    var queryEmbeddingsPath: String?
    var embeddingModelDir: String?
    var compute: EmbeddingCompute = .all
    var topK: Int?
    var prettyPrint = true
    var showHelp = false
}

struct QueryEmbeddingFile: Decodable {
    var modelSlug: String?
    var model: String
    var sequenceLength: Int
    var dimension: Int
    var topK: Int
    var warmupQueries: Int
    var measuredQueries: Int
    var queryPrefix: String?
    var queries: [QueryEmbeddingRecord]

    enum CodingKeys: String, CodingKey {
        case modelSlug = "model_slug"
        case model
        case sequenceLength = "sequence_length"
        case dimension
        case topK = "top_k"
        case warmupQueries = "warmup_queries"
        case measuredQueries = "measured_queries"
        case queryPrefix = "query_prefix"
        case queries
    }
}

struct QueryEmbeddingRecord: Decodable {
    var query: String
    var embedding: [Float]
}

struct RealSearchHit {
    var documentID: String
    var score: Float
    var text: String
}

struct RealSearchSample {
    var query: String
    var firstHit: RealSearchHit?
}

struct EndToEndTiming {
    var embed: Double
    var search: Double
    var total: Double
}

struct LatencyStats {
    var min: Double
    var average: Double
    var p50: Double
    var p95: Double
    var p99: Double
    var max: Double
    var count: Int

    init(_ samples: [Double]) throws {
        guard !samples.isEmpty else {
            throw BenchHarnessError.invalidRealBenchmark("no latency samples were recorded")
        }
        let sorted = samples.sorted()
        min = sorted[0]
        max = sorted[sorted.count - 1]
        average = samples.reduce(0, +) / Double(samples.count)
        p50 = percentile(sorted, 0.50)
        p95 = percentile(sorted, 0.95)
        p99 = percentile(sorted, 0.99)
        count = samples.count
    }
}

let smallSmokeConfig = """
{
  "chunks": 128,
  "dimensions": [32],
  "queries": 8,
  "top_k": 5,
  "encodings": ["f32", "f16", "i8"],
  "metric": "cosine",
  "include_unfiltered": true,
  "include_filtered": true,
  "filter_every": 4
}
"""

func parseOptions(_ arguments: [String]) throws -> CommandLineOptions {
    var options = CommandLineOptions()
    var index = 0

    while index < arguments.count {
        let argument = arguments[index]

        switch argument {
        case "--help", "-h":
            options.showHelp = true
        case "--raw":
            options.prettyPrint = false
        case "--small-smoke":
            options.configJSON = smallSmokeConfig
        case "--config":
            guard let value = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.configJSON = value
            index += 1
        case "--config-file":
            guard let path = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            guard let contents = try? String(contentsOfFile: path, encoding: .utf8) else {
                throw BenchHarnessError.unreadableConfigFile(path)
            }
            options.configJSON = contents
            index += 1
        case "--real-index":
            guard let path = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.realIndexPath = path
            index += 1
        case "--query-embeddings":
            guard let path = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.queryEmbeddingsPath = path
            index += 1
        case "--embedding-model-dir":
            guard let path = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.embeddingModelDir = path
            index += 1
        case "--compute":
            guard let value = arguments[safe: index + 1] else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.compute = try parseEmbeddingCompute(value)
            index += 1
        case "--top-k":
            guard let value = arguments[safe: index + 1], let topK = Int(value), topK > 0 else {
                throw BenchHarnessError.missingValue(argument)
            }
            options.topK = topK
            index += 1
        default:
            throw BenchHarnessError.unknownArgument(argument)
        }

        index += 1
    }

    return options
}

func parseEmbeddingCompute(_ value: String) throws -> EmbeddingCompute {
    switch value {
    case "all":
        return .all
    case "cpu-only":
        return .cpuOnly
    case "cpu-and-gpu":
        return .cpuAndGPU
    case "cpu-and-neural-engine":
        return .cpuAndNeuralEngine
    default:
        throw BenchHarnessError.invalidRealBenchmark(
            "unsupported --compute '\(value)', expected all, cpu-only, cpu-and-gpu, or cpu-and-neural-engine"
        )
    }
}

func runRealIndexSearch(options: CommandLineOptions) throws -> String {
    guard let realIndexPath = options.realIndexPath else {
        throw BenchHarnessError.invalidRealBenchmark("--real-index is required")
    }
    guard let queryEmbeddingsPath = options.queryEmbeddingsPath else {
        throw BenchHarnessError.invalidRealBenchmark("--query-embeddings is required")
    }

    let queryData = try Data(contentsOf: URL(fileURLWithPath: queryEmbeddingsPath))
    let queryFile = try JSONDecoder().decode(QueryEmbeddingFile.self, from: queryData)
    guard queryFile.queries.count >= queryFile.warmupQueries + queryFile.measuredQueries else {
        throw BenchHarnessError.invalidRealBenchmark(
            "query file has \(queryFile.queries.count) queries, expected at least "
                + "\(queryFile.warmupQueries + queryFile.measuredQueries)"
        )
    }
    guard queryFile.queries.allSatisfy({ $0.embedding.count == queryFile.dimension }) else {
        throw BenchHarnessError.invalidRealBenchmark("one or more query embeddings have the wrong dimension")
    }

    let topK = options.topK ?? queryFile.topK
    let index = try loadIndex(path: realIndexPath)
    defer { retrievalkit_index_free(index) }

    let indexDimension = Int(retrievalkit_index_dimension(index))
    let activeChunks = Int(retrievalkit_index_active_chunk_count(index))
    guard indexDimension == queryFile.dimension else {
        throw BenchHarnessError.invalidRealBenchmark(
            "index dimension \(indexDimension) does not match query dimension \(queryFile.dimension)"
        )
    }

    for query in queryFile.queries.prefix(queryFile.warmupQueries) {
        _ = try search(index: index, embedding: query.embedding, topK: topK)
    }

    var samples: [Double] = []
    samples.reserveCapacity(queryFile.measuredQueries)
    var hitCount = 0
    var resultSamples: [RealSearchSample] = []
    resultSamples.reserveCapacity(5)

    let measured = queryFile.queries.dropFirst(queryFile.warmupQueries).prefix(queryFile.measuredQueries)
    for query in measured {
        let started = DispatchTime.now().uptimeNanoseconds
        let hits = try search(index: index, embedding: query.embedding, topK: topK)
        let elapsed = Double(DispatchTime.now().uptimeNanoseconds - started) / 1_000_000
        samples.append(elapsed)
        hitCount += hits.count
        if resultSamples.count < 5 {
            resultSamples.append(RealSearchSample(query: query.query, firstHit: hits.first))
        }
    }

    let stats = try LatencyStats(samples)
    return markdownReport(
        model: queryFile.model,
        sequenceLength: queryFile.sequenceLength,
        indexPath: realIndexPath,
        queryEmbeddingsPath: queryEmbeddingsPath,
        chunks: activeChunks,
        dimension: indexDimension,
        topK: topK,
        warmupQueries: queryFile.warmupQueries,
        measuredQueries: queryFile.measuredQueries,
        hitCount: hitCount,
        stats: stats,
        samples: resultSamples
    )
}

func runTextIndexSearch(options: CommandLineOptions) async throws -> String {
    guard let realIndexPath = options.realIndexPath else {
        throw BenchHarnessError.invalidRealBenchmark("--real-index is required")
    }
    guard let queryEmbeddingsPath = options.queryEmbeddingsPath else {
        throw BenchHarnessError.invalidRealBenchmark("--query-embeddings is required as query text source")
    }
    guard let embeddingModelDir = options.embeddingModelDir else {
        throw BenchHarnessError.invalidRealBenchmark("--embedding-model-dir is required")
    }

    let queryData = try Data(contentsOf: URL(fileURLWithPath: queryEmbeddingsPath))
    let queryFile = try JSONDecoder().decode(QueryEmbeddingFile.self, from: queryData)
    guard queryFile.queries.count >= queryFile.warmupQueries + queryFile.measuredQueries else {
        throw BenchHarnessError.invalidRealBenchmark(
            "query file has \(queryFile.queries.count) queries, expected at least "
                + "\(queryFile.warmupQueries + queryFile.measuredQueries)"
        )
    }

    let topK = options.topK ?? queryFile.topK
    let index = try loadIndex(path: realIndexPath)
    defer { retrievalkit_index_free(index) }

    let indexDimension = Int(retrievalkit_index_dimension(index))
    let activeChunks = Int(retrievalkit_index_active_chunk_count(index))
    guard indexDimension == queryFile.dimension else {
        throw BenchHarnessError.invalidRealBenchmark(
            "index dimension \(indexDimension) does not match query dimension \(queryFile.dimension)"
        )
    }

    let modelInitStart = DispatchTime.now().uptimeNanoseconds
    let embedder = try makeCoreMLEmbedder(
        modelDir: embeddingModelDir,
        model: queryFile.model,
        dimension: queryFile.dimension,
        sequenceLength: queryFile.sequenceLength,
        compute: options.compute
    )
    let modelInitMs = elapsedMilliseconds(since: modelInitStart)

    let queryPrefix = queryFile.queryPrefix ?? ""
    for query in queryFile.queries.prefix(queryFile.warmupQueries) {
        let embedding = try await embedder.embed(queryPrefix + query.query)
        _ = try search(index: index, embedding: embedding, topK: topK)
    }

    var embedSamples: [Double] = []
    var searchSamples: [Double] = []
    var totalSamples: [Double] = []
    embedSamples.reserveCapacity(queryFile.measuredQueries)
    searchSamples.reserveCapacity(queryFile.measuredQueries)
    totalSamples.reserveCapacity(queryFile.measuredQueries)

    var hitCount = 0
    var resultSamples: [RealSearchSample] = []
    resultSamples.reserveCapacity(5)

    let measured = queryFile.queries.dropFirst(queryFile.warmupQueries).prefix(queryFile.measuredQueries)
    for query in measured {
        let totalStart = DispatchTime.now().uptimeNanoseconds
        let embedStart = DispatchTime.now().uptimeNanoseconds
        let embedding = try await embedder.embed(queryPrefix + query.query)
        let embedMs = elapsedMilliseconds(since: embedStart)

        let searchStart = DispatchTime.now().uptimeNanoseconds
        let hits = try search(index: index, embedding: embedding, topK: topK)
        let searchMs = elapsedMilliseconds(since: searchStart)
        let totalMs = elapsedMilliseconds(since: totalStart)

        embedSamples.append(embedMs)
        searchSamples.append(searchMs)
        totalSamples.append(totalMs)
        hitCount += hits.count
        if resultSamples.count < 5 {
            resultSamples.append(RealSearchSample(query: query.query, firstHit: hits.first))
        }
    }

    return textMarkdownReport(
        model: queryFile.model,
        modelSlug: queryFile.modelSlug,
        sequenceLength: queryFile.sequenceLength,
        indexPath: realIndexPath,
        queryTextPath: queryEmbeddingsPath,
        embeddingModelDir: embeddingModelDir,
        chunks: activeChunks,
        dimension: indexDimension,
        topK: topK,
        warmupQueries: queryFile.warmupQueries,
        measuredQueries: queryFile.measuredQueries,
        hitCount: hitCount,
        modelInitMs: modelInitMs,
        embedStats: try LatencyStats(embedSamples),
        searchStats: try LatencyStats(searchSamples),
        totalStats: try LatencyStats(totalSamples),
        samples: resultSamples
    )
}

func makeCoreMLEmbedder(
    modelDir: String,
    model: String,
    dimension: Int,
    sequenceLength: Int,
    compute: EmbeddingCompute
) throws -> CoreMLEmbedder {
    let modelDirectory = URL(fileURLWithPath: modelDir)
    let tokenizer = try BertWordPieceTokenizer(
        tokenizerDirectory: modelDirectory.appendingPathComponent("tokenizer"),
        sequenceLength: sequenceLength
    )
    let modelInfo = try EmbeddingModelInfo(
        identifier: model,
        dimension: dimension,
        maxInputTokens: sequenceLength,
        producesNormalizedEmbeddings: true,
        recommendedMetric: .cosine
    )
    let modelURL = try firstModelPackage(in: modelDirectory)
    return try CoreMLEmbedder(
        modelInfo: modelInfo,
        tokenizer: tokenizer,
        configuration: CoreMLModelConfiguration(
            modelURL: modelURL,
            compute: compute
        )
    )
}

func firstModelPackage(in directory: URL) throws -> URL {
    let contents = try FileManager.default.contentsOfDirectory(
        at: directory,
        includingPropertiesForKeys: nil
    )
    if let compiled = contents.first(where: { $0.pathExtension == "mlmodelc" }) {
        return compiled
    }
    if let package = contents.first(where: { $0.pathExtension == "mlpackage" }) {
        return package
    }
    throw BenchHarnessError.invalidRealBenchmark("no .mlpackage or .mlmodelc found in \(directory.path)")
}

func loadIndex(path: String) throws -> OpaquePointer {
    var status = VkStatus(code: 0, message: nil)
    defer { retrievalkit_status_clear(&status) }
    guard let index = path.withCString({ retrievalkit_index_load($0, &status) }) else {
        throw BenchHarnessError.retrievalKit(statusMessage(status))
    }
    return index
}

func search(index: OpaquePointer, embedding: [Float], topK: Int) throws -> [RealSearchHit] {
    var output = VkSearchResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer { retrievalkit_status_clear(&status) }

    let succeeded = embedding.withUnsafeBufferPointer { buffer in
        retrievalkit_index_search(
            index,
            buffer.baseAddress,
            buffer.count,
            topK,
            nil,
            &output,
            &status
        )
    }
    guard succeeded else {
        throw BenchHarnessError.retrievalKit(statusMessage(status))
    }
    defer { retrievalkit_search_results_free(output) }
    guard let hits = output.hits else {
        return []
    }

    return UnsafeBufferPointer(start: hits, count: output.count).map { hit in
        RealSearchHit(
            documentID: String(cString: hit.document_id),
            score: hit.score,
            text: String(cString: hit.text)
        )
    }
}

func statusMessage(_ status: VkStatus) -> String {
    status.message.map { String(cString: $0) } ?? "unknown RetrievalKit error"
}

func markdownReport(
    model: String,
    sequenceLength: Int,
    indexPath: String,
    queryEmbeddingsPath: String,
    chunks: Int,
    dimension: Int,
    topK: Int,
    warmupQueries: Int,
    measuredQueries: Int,
    hitCount: Int,
    stats: LatencyStats,
    samples: [RealSearchSample]
) -> String {
    var lines: [String] = []
    lines.append("# RetrievalKit MiniLM Real Search")
    lines.append("")
    lines.append("| Model | Seq | Chunks | Dim | Top K | Warmup | Measured | Avg Hits |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    lines.append(
        "| `\(model)` | \(sequenceLength) | \(chunks) | \(dimension) | \(topK) | "
            + "\(warmupQueries) | \(measuredQueries) | \(format(Double(hitCount) / Double(measuredQueries))) |"
    )
    lines.append("")
    lines.append("| Phase | Avg | P50 | P95 | P99 | Min | Max | Samples |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    lines.append(
        "| Swift exact vector search | \(format(stats.average)) ms | \(format(stats.p50)) ms | "
            + "\(format(stats.p95)) ms | \(format(stats.p99)) ms | \(format(stats.min)) ms | "
            + "\(format(stats.max)) ms | \(stats.count) |"
    )
    lines.append("")
    lines.append("| Query | Top Document | Score | Top Text |")
    lines.append("|---|---|---:|---|")
    for sample in samples {
        if let hit = sample.firstHit {
            lines.append(
                "| \(cell(sample.query)) | `\(cell(hit.documentID))` | \(format(Double(hit.score))) | "
                    + "\(cell(shorten(hit.text))) |"
            )
        } else {
            lines.append("| \(cell(sample.query)) |  |  |  |")
        }
    }
    lines.append("")
    lines.append("Index: `\(indexPath)`")
    lines.append("Queries: `\(queryEmbeddingsPath)`")
    return lines.joined(separator: "\n")
}

func textMarkdownReport(
    model: String,
    modelSlug: String?,
    sequenceLength: Int,
    indexPath: String,
    queryTextPath: String,
    embeddingModelDir: String,
    chunks: Int,
    dimension: Int,
    topK: Int,
    warmupQueries: Int,
    measuredQueries: Int,
    hitCount: Int,
    modelInitMs: Double,
    embedStats: LatencyStats,
    searchStats: LatencyStats,
    totalStats: LatencyStats,
    samples: [RealSearchSample]
) -> String {
    var lines: [String] = []
    lines.append("# RetrievalKit Text-to-Search Benchmark")
    lines.append("")
    lines.append("| Model | Slug | Seq | Chunks | Dim | Top K | Warmup | Measured | Avg Hits | Model Init |")
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    lines.append(
        "| `\(model)` | `\(modelSlug ?? "")` | \(sequenceLength) | \(chunks) | \(dimension) | "
            + "\(topK) | \(warmupQueries) | \(measuredQueries) | "
            + "\(format(Double(hitCount) / Double(measuredQueries))) | \(format(modelInitMs)) ms |"
    )
    lines.append("")
    lines.append("| Phase | Avg | P50 | P95 | P99 | Min | Max | Samples |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    lines.append(statsRow("Text embedding", embedStats))
    lines.append(statsRow("Vector search", searchStats))
    lines.append(statsRow("End-to-end text search", totalStats))
    lines.append("")
    lines.append("| Query | Top Document | Score | Top Text |")
    lines.append("|---|---|---:|---|")
    for sample in samples {
        if let hit = sample.firstHit {
            lines.append(
                "| \(cell(sample.query)) | `\(cell(hit.documentID))` | \(format(Double(hit.score))) | "
                    + "\(cell(shorten(hit.text))) |"
            )
        } else {
            lines.append("| \(cell(sample.query)) |  |  |  |")
        }
    }
    lines.append("")
    lines.append("Index: `\(indexPath)`")
    lines.append("Query text source: `\(queryTextPath)`")
    lines.append("Embedding model dir: `\(embeddingModelDir)`")
    return lines.joined(separator: "\n")
}

func statsRow(_ phase: String, _ stats: LatencyStats) -> String {
    "| \(phase) | \(format(stats.average)) ms | \(format(stats.p50)) ms | "
        + "\(format(stats.p95)) ms | \(format(stats.p99)) ms | \(format(stats.min)) ms | "
        + "\(format(stats.max)) ms | \(stats.count) |"
}

func percentile(_ sorted: [Double], _ quantile: Double) -> Double {
    if sorted.count == 1 {
        return sorted[0]
    }
    let raw = quantile * Double(sorted.count - 1)
    let lower = Int(raw.rounded(.down))
    let upper = Int(raw.rounded(.up))
    if lower == upper {
        return sorted[lower]
    }
    let fraction = raw - Double(lower)
    return sorted[lower] * (1 - fraction) + sorted[upper] * fraction
}

func format(_ value: Double) -> String {
    String(format: "%.3f", value)
}

func elapsedMilliseconds(since started: UInt64) -> Double {
    Double(DispatchTime.now().uptimeNanoseconds - started) / 1_000_000
}

func shorten(_ value: String, maxLength: Int = 120) -> String {
    let normalized = value.replacingOccurrences(of: "\n", with: " ")
        .split(separator: " ")
        .joined(separator: " ")
    guard normalized.count > maxLength else {
        return normalized
    }
    let end = normalized.index(normalized.startIndex, offsetBy: maxLength)
    return String(normalized[..<end]) + "..."
}

func cell(_ value: String) -> String {
    value.replacingOccurrences(of: "|", with: "\\|")
}

func runBenchmark(configJSON: String?) throws -> String {
    let resultPointer: UnsafeMutablePointer<CChar>?

    if let configJSON {
        resultPointer = configJSON.withCString { pointer in
            retrievalkit_bench_synthetic_json(pointer)
        }
    } else {
        resultPointer = retrievalkit_bench_synthetic_json(nil)
    }

    guard let resultPointer else {
        throw BenchHarnessError.nullResult
    }
    defer {
        retrievalkit_string_free(resultPointer)
    }

    guard let result = String(validatingCString: resultPointer) else {
        throw BenchHarnessError.invalidUtf8Result
    }

    return result
}

func formattedJSON(_ json: String, prettyPrint: Bool) -> String {
    guard prettyPrint, let data = json.data(using: .utf8) else {
        return json
    }

    guard
        let object = try? JSONSerialization.jsonObject(with: data),
        let prettyData = try? JSONSerialization.data(
            withJSONObject: object,
            options: [.prettyPrinted, .sortedKeys]
        ),
        let pretty = String(data: prettyData, encoding: .utf8)
    else {
        return json
    }

    return pretty
}

func responseSucceeded(_ json: String) -> Bool {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let ok = object["ok"] as? Bool
    else {
        return false
    }

    return ok
}

func printUsage() {
    print(
        """
        usage:
          retrievalkit-bench [--small-smoke]
          retrievalkit-bench --config '<json>'
          retrievalkit-bench --config-file config.json
          retrievalkit-bench --real-index <dir> --query-embeddings <json>
          retrievalkit-bench --real-index <dir> --query-embeddings <json> --embedding-model-dir <dir>

        options:
          --small-smoke       run a small link/smoke benchmark
          --config <json>     pass a benchmark config JSON string
          --config-file <p>   read benchmark config JSON from a file
          --real-index <dir>  load a persisted RetrievalKit index and run real searches
          --query-embeddings <json>
                              query embeddings generated for --real-index, or query text source
                              when --embedding-model-dir is set
          --embedding-model-dir <dir>
                              run text -> tokenizer -> Core ML embedding -> search
          --compute <kind>    all, cpu-only, cpu-and-gpu, or cpu-and-neural-engine; default all
          --top-k <n>         override query fixture top_k for --real-index
          --raw               print compact JSON instead of pretty JSON
          --help, -h          show this help

        With no config, the Rust FFI default runs:
          24K chunks, dimensions 384 and 768, f32/f16/i8, unfiltered and filtered.
        """
    )
}

extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

do {
    let options = try parseOptions(Array(CommandLine.arguments.dropFirst()))
    if options.showHelp {
        printUsage()
        exit(EXIT_SUCCESS)
    }

    if options.embeddingModelDir != nil {
        print(try await runTextIndexSearch(options: options))
        exit(EXIT_SUCCESS)
    } else if options.realIndexPath != nil || options.queryEmbeddingsPath != nil {
        print(try runRealIndexSearch(options: options))
        exit(EXIT_SUCCESS)
    } else {
        let json = try runBenchmark(configJSON: options.configJSON)
        print(formattedJSON(json, prettyPrint: options.prettyPrint))
        exit(responseSucceeded(json) ? EXIT_SUCCESS : EXIT_FAILURE)
    }
} catch {
    fputs("error: \(error)\n", stderr)
    printUsage()
    exit(EXIT_FAILURE)
}
