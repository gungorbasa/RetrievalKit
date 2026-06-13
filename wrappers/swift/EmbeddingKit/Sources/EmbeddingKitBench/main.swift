import EmbeddingKit
import Foundation

@main
struct EmbeddingKitBench {
    static func main() async {
        do {
            let options = try BenchOptions(arguments: Array(CommandLine.arguments.dropFirst()))
            if options.showHelp {
                print(Self.helpText)
                return
            }

            let queries = try QueryLoader.load(options: options)
            let config = try EmbeddingBenchmarkConfig(
                warmupIterations: options.warmupIterations,
                measuredIterations: options.measuredIterations,
                batchSizes: options.batchSizes
            )

            var reports: [EmbeddingBenchmarkReport] = []
            reports.reserveCapacity(options.models.count)
            for model in options.models {
                let embedder = try PrecomputedEmbedder(
                    modelInfo: model.info,
                    embeddings: FixtureEmbeddings.makeEmbeddings(
                        queries: queries,
                        dimension: model.info.dimension
                    ),
                    runtimeInfo: EmbeddingRuntimeInfo(
                        name: "Precomputed",
                        requestedCompute: .cpuOnly,
                        actualCompute: .cpuOnly
                    )
                )
                reports.append(try await EmbeddingBenchmark.run(
                    embedder: embedder,
                    queries: queries,
                    config: config
                ))
            }

            let output: String
            switch options.format {
            case .json:
                let encoder = JSONEncoder()
                encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
                output = String(data: try encoder.encode(reports), encoding: .utf8) ?? "[]"
            case .markdown:
                output = MarkdownReport.render(reports)
            }

            if let outputPath = options.outputPath {
                try output.write(to: outputPath, atomically: true, encoding: .utf8)
            } else {
                print(output)
            }
        } catch {
            fputs("embeddingkit-bench: \(error)\n", stderr)
            fputs(Self.helpText + "\n", stderr)
            Foundation.exit(1)
        }
    }

    private static let helpText = """
    Usage:
      embeddingkit-bench [options]

    Options:
      --models <list>         Comma-separated model aliases. Defaults to bge-small-en-v1.5.
                              Aliases: bge-small-en-v1.5, all-MiniLM-L6-v2,
                              arctic-xs, arctic-s, e5-small-v2, gte-small,
                              bge-base-en-v1.5, arctic-m.
      --queries <name>        Built-in query fixture. Defaults to social-network.
      --queries-file <path>   JSON query file. Supports ["query"], {"queries":[...]}, or {"query":"..."}.
      --warmup <count>        Warmup iterations excluded from stats. Defaults to 50.
      --measured <count>      Measured iterations. Defaults to 750.
      --batch-sizes <list>    Comma-separated batch sizes. Defaults to 1,8,16,32,64.
      --format <name>         markdown or json. Defaults to markdown.
      --output <path>         Write report to a file instead of stdout.
      --help                  Show this help text.
    """
}

private struct BenchOptions {
    var models: [ModelSelection] = [.bgeSmallEnV15]
    var queriesName = "social-network"
    var queriesFile: URL?
    var warmupIterations = 50
    var measuredIterations = 750
    var batchSizes = [1, 8, 16, 32, 64]
    var format = OutputFormat.markdown
    var outputPath: URL?
    var showHelp = false

    init(arguments: [String]) throws {
        var index = 0
        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "--help", "-h":
                showHelp = true
                index += 1
            case "--models":
                models = try parseRequiredValue(arguments, &index, option: argument)
                    .split(separator: ",")
                    .map { try ModelSelection(alias: String($0)) }
            case "--queries":
                queriesName = try parseRequiredValue(arguments, &index, option: argument)
            case "--queries-file":
                queriesFile = URL(fileURLWithPath: try parseRequiredValue(arguments, &index, option: argument))
            case "--warmup":
                warmupIterations = try parseInteger(arguments, &index, option: argument)
            case "--measured":
                measuredIterations = try parseInteger(arguments, &index, option: argument)
            case "--batch-sizes":
                batchSizes = try parseRequiredValue(arguments, &index, option: argument)
                    .split(separator: ",")
                    .map { value in
                        guard let parsed = Int(value), parsed > 0 else {
                            throw CLIError.invalidValue(argument, String(value))
                        }
                        return parsed
                    }
            case "--format":
                format = try OutputFormat(rawValue: parseRequiredValue(arguments, &index, option: argument))
            case "--output":
                outputPath = URL(fileURLWithPath: try parseRequiredValue(arguments, &index, option: argument))
            default:
                throw CLIError.unknownArgument(argument)
            }
        }

        guard !models.isEmpty else {
            throw CLIError.invalidValue("--models", "empty")
        }
    }
}

private enum OutputFormat: String {
    case markdown
    case json

    init(rawValue: String) throws {
        switch rawValue {
        case "markdown", "md":
            self = .markdown
        case "json":
            self = .json
        default:
            throw CLIError.invalidValue("--format", rawValue)
        }
    }
}

private enum ModelSelection {
    case bgeSmallEnV15
    case allMiniLML6V2
    case arcticXS
    case arcticS
    case e5SmallV2
    case gteSmall
    case bgeBaseEnV15
    case arcticM

    var info: EmbeddingModelInfo {
        switch self {
        case .bgeSmallEnV15:
            KnownEmbeddingModels.bgeSmallEnV15
        case .allMiniLML6V2:
            KnownEmbeddingModels.allMiniLML6V2
        case .arcticXS:
            KnownEmbeddingModels.snowflakeArcticEmbedXS
        case .arcticS:
            KnownEmbeddingModels.snowflakeArcticEmbedS
        case .e5SmallV2:
            KnownEmbeddingModels.e5SmallV2
        case .gteSmall:
            KnownEmbeddingModels.gteSmall
        case .bgeBaseEnV15:
            KnownEmbeddingModels.bgeBaseEnV15
        case .arcticM:
            KnownEmbeddingModels.snowflakeArcticEmbedM
        }
    }

    init(alias: String) throws {
        switch alias {
        case "bge-small-en-v1.5", "BAAI/bge-small-en-v1.5":
            self = .bgeSmallEnV15
        case "all-MiniLM-L6-v2", "sentence-transformers/all-MiniLM-L6-v2":
            self = .allMiniLML6V2
        case "arctic-xs", "snowflake/snowflake-arctic-embed-xs":
            self = .arcticXS
        case "arctic-s", "snowflake/snowflake-arctic-embed-s":
            self = .arcticS
        case "e5-small-v2", "intfloat/e5-small-v2":
            self = .e5SmallV2
        case "gte-small", "thenlper/gte-small":
            self = .gteSmall
        case "bge-base-en-v1.5", "BAAI/bge-base-en-v1.5":
            self = .bgeBaseEnV15
        case "arctic-m", "snowflake/snowflake-arctic-embed-m":
            self = .arcticM
        default:
            throw CLIError.invalidValue("--models", alias)
        }
    }
}

private enum QueryLoader {
    static func load(options: BenchOptions) throws -> [String] {
        if let queriesFile = options.queriesFile {
            return try loadFile(queriesFile)
        }

        switch options.queriesName {
        case "social-network":
            return socialNetworkQueries
        default:
            throw CLIError.invalidValue("--queries", options.queriesName)
        }
    }

    private static func loadFile(_ url: URL) throws -> [String] {
        let data = try Data(contentsOf: url)
        if let array = try? JSONDecoder().decode([String].self, from: data) {
            return try validate(array)
        }
        if let object = try? JSONDecoder().decode(QueryListFile.self, from: data) {
            return try validate(object.queries)
        }
        if let object = try? JSONDecoder().decode(SingleQueryFile.self, from: data) {
            return try validate([object.query])
        }
        throw CLIError.invalidValue("--queries-file", "unsupported JSON shape")
    }

    private static func validate(_ queries: [String]) throws -> [String] {
        let trimmed = queries.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        guard !trimmed.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        return trimmed
    }

    private struct QueryListFile: Decodable {
        var queries: [String]
    }

    private struct SingleQueryFile: Decodable {
        var query: String
    }

    private static let socialNetworkQueries = [
        "Mark and Erica arguing in a dim bar",
        "shots on the Harvard campus at night",
        "Mark coding Facemash in his dorm room",
        "Eduardo confronting Mark about the company",
        "Sean Parker meeting the Facebook team",
        "students reacting to the site launch",
        "the Winklevoss twins rowing on the river",
        "a legal deposition about Facebook ownership",
        "party scenes with loud music and crowded rooms",
        "a quiet emotional moment after an argument",
        "Dustin and Chris discussing growth at Facebook",
        "Harvard students using laptops in dorm rooms",
    ]
}

private enum FixtureEmbeddings {
    static func makeEmbeddings(queries: [String], dimension: Int) -> [String: [Float]] {
        Dictionary(uniqueKeysWithValues: queries.map { query in
            (query, vector(for: query, dimension: dimension))
        })
    }

    private static func vector(for text: String, dimension: Int) -> [Float] {
        var values: [Float] = []
        values.reserveCapacity(dimension)
        for index in 0..<dimension {
            let hash = fnv1a64("\(text)#\(index)")
            let scaled = Float(hash % 2_000_001) / 1_000_000 - 1
            values.append(scaled)
        }

        let norm = sqrt(values.reduce(Float(0)) { $0 + $1 * $1 })
        guard norm > 0 else {
            return values
        }
        return values.map { $0 / norm }
    }

    private static func fnv1a64(_ text: String) -> UInt64 {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in text.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x100000001b3
        }
        return hash
    }
}

private enum MarkdownReport {
    static func render(_ reports: [EmbeddingBenchmarkReport]) -> String {
        var lines: [String] = []
        lines.append("# EmbeddingKit Benchmark")
        lines.append("")
        lines.append("| Model | Runtime | Compute | Dim | Queries | P50 | P95 | P99 | Mean |")
        lines.append("|---|---|---|---:|---:|---:|---:|---:|---:|")
        for report in reports {
            lines.append([
                report.modelInfo.identifier,
                report.runtimeInfo.name,
                report.runtimeInfo.actualCompute?.rawValue ?? report.runtimeInfo.requestedCompute.rawValue,
                String(report.modelInfo.dimension),
                String(report.queryCount),
                formatMilliseconds(report.singleQueryLatency.p50Milliseconds),
                formatMilliseconds(report.singleQueryLatency.p95Milliseconds),
                formatMilliseconds(report.singleQueryLatency.p99Milliseconds),
                formatMilliseconds(report.singleQueryLatency.meanMilliseconds),
            ].joined(separator: " | ").asMarkdownTableRow())
        }

        lines.append("")
        lines.append("## Batch Throughput")
        lines.append("")
        lines.append("| Model | Batch | P50 | P95 | P99 | Mean | Texts/sec |")
        lines.append("|---|---:|---:|---:|---:|---:|---:|")
        for report in reports {
            for result in report.batchResults {
                lines.append([
                    report.modelInfo.identifier,
                    String(result.batchSize),
                    formatMilliseconds(result.latency.p50Milliseconds),
                    formatMilliseconds(result.latency.p95Milliseconds),
                    formatMilliseconds(result.latency.p99Milliseconds),
                    formatMilliseconds(result.latency.meanMilliseconds),
                    formatNumber(result.textsPerSecond),
                ].joined(separator: " | ").asMarkdownTableRow())
            }
        }

        lines.append("")
        lines.append("Warmup iterations: \(reports.first?.warmupIterations ?? 0)")
        lines.append("Measured iterations: \(reports.first?.measuredIterations ?? 0)")
        lines.append("")
        return lines.joined(separator: "\n")
    }

    private static func formatMilliseconds(_ value: Double) -> String {
        String(format: "%.3f ms", value)
    }

    private static func formatNumber(_ value: Double) -> String {
        String(format: "%.1f", value)
    }
}

private enum CLIError: Error, CustomStringConvertible {
    case missingValue(String)
    case invalidValue(String, String)
    case unknownArgument(String)

    var description: String {
        switch self {
        case .missingValue(let option):
            "missing value for \(option)"
        case .invalidValue(let option, let value):
            "invalid value for \(option): \(value)"
        case .unknownArgument(let argument):
            "unknown argument: \(argument)"
        }
    }
}

private func parseRequiredValue(
    _ arguments: [String],
    _ index: inout Int,
    option: String
) throws -> String {
    let valueIndex = index + 1
    guard valueIndex < arguments.count else {
        throw CLIError.missingValue(option)
    }
    index += 2
    return arguments[valueIndex]
}

private func parseInteger(
    _ arguments: [String],
    _ index: inout Int,
    option: String
) throws -> Int {
    let value = try parseRequiredValue(arguments, &index, option: option)
    guard let parsed = Int(value) else {
        throw CLIError.invalidValue(option, value)
    }
    return parsed
}

private extension String {
    func asMarkdownTableRow() -> String {
        "| \(self) |"
    }
}
