import Foundation
import RetrievalKitAppleE2EBenchmarkCore

@main
struct Main {
    static func main() async {
        do {
            let arguments = try Arguments(CommandLine.arguments)
            switch arguments.command {
            case "prepare-index":
                try await AppleE2EBenchmark.prepareIndex(
                    corpusURL: try arguments.url("corpus"),
                    outputURL: try arguments.url("output-index"),
                    modelURL: try arguments.url("model"),
                    tokenizerURL: try arguments.url("tokenizer"),
                    expectedChunks: try arguments.integer("expected-chunks")
                )
            case "prepare-quality-index":
                try await AppleE2EBenchmark.prepareIndex(
                    corpusURL: try arguments.url("corpus"),
                    outputURL: try arguments.url("output-index"),
                    modelURL: try arguments.url("model"),
                    tokenizerURL: try arguments.url("tokenizer"),
                    expectedChunks: 48,
                    contractWorkload: false
                )
            case "validate-inputs":
                let embedder = try AppleE2EBenchmark.makeEmbedder(
                    modelURL: try arguments.url("model"),
                    tokenizerURL: try arguments.url("tokenizer")
                )
                _ = try AppleE2EBenchmark.loadAndValidateQueries(
                    from: try arguments.url("queries"),
                    embedder: embedder
                )
                print("query inputs valid")
            case "run":
                let modeValue = try arguments.value("mode")
                guard let mode = SearchMode(rawValue: modeValue) else {
                    throw AppleE2EBenchmarkError.invalidArgument("invalid --mode \(modeValue)")
                }
                let classification = try arguments.value("workload-classification")
                let report = try await AppleE2EBenchmark.run(RunConfiguration(
                    contractVersion: arguments.optionalValue("contract-version") ?? "apple-end-to-end-v1",
                    queriesURL: try arguments.url("queries"),
                    indexURL: try arguments.url("index"),
                    modelURL: try arguments.url("model"),
                    tokenizerURL: try arguments.url("tokenizer"),
                    outputURL: try arguments.url("output"),
                    workloadID: try arguments.value("workload-id"),
                    workloadClassification: classification,
                    marketingEligible: classification != "stress",
                    profileID: try arguments.value("profile-id"),
                    profileClassification: try arguments.value("profile-classification"),
                    sessionID: try arguments.value("session-id"),
                    mode: mode,
                    retrievalKitRevision: try arguments.value("retrievalkit-revision")
                ))
                let summary = report.summaries["end_to_end_text_search"]!
                print("wrote \(report.samples.count) samples; p95=\(summary.p95NS) ns")
            case "compare-quality":
                let report = try await AppleE2EBenchmark.compareQuality(
                    queriesURL: try arguments.url("queries"),
                    referenceModelURL: try arguments.url("reference-model"),
                    referenceTokenizerURL: try arguments.url("reference-tokenizer"),
                    referenceIndexURL: try arguments.url("reference-index"),
                    candidateModelURL: try arguments.url("candidate-model"),
                    candidateTokenizerURL: try arguments.url("candidate-tokenizer"),
                    candidateIndexURL: try arguments.url("candidate-index"),
                    outputURL: try arguments.url("output")
                )
                print(
                    "quality passed=\(report.passed) cosine=\(report.medianCosine) "
                        + "mean_overlap=\(report.meanTop10Overlap)"
                )
            default:
                throw AppleE2EBenchmarkError.invalidArgument(
                    "command must be validate-inputs, prepare-index, prepare-quality-index, compare-quality, or run"
                )
            }
        } catch {
            FileHandle.standardError.write(Data("error: \(error)\n".utf8))
            Foundation.exit(2)
        }
    }
}

private struct Arguments {
    let command: String
    private let values: [String: String]

    init(_ arguments: [String]) throws {
        guard arguments.count >= 2 else {
            throw AppleE2EBenchmarkError.invalidArgument("missing command")
        }
        command = arguments[1]
        var parsed: [String: String] = [:]
        var index = 2
        while index < arguments.count {
            let key = arguments[index]
            guard key.hasPrefix("--"), index + 1 < arguments.count else {
                throw AppleE2EBenchmarkError.invalidArgument("arguments must be --key value pairs")
            }
            parsed[String(key.dropFirst(2))] = arguments[index + 1]
            index += 2
        }
        values = parsed
    }

    func value(_ key: String) throws -> String {
        guard let value = values[key], !value.isEmpty else {
            throw AppleE2EBenchmarkError.invalidArgument("missing --\(key)")
        }
        return value
    }

    func optionalValue(_ key: String) -> String? {
        values[key]
    }

    func url(_ key: String) throws -> URL {
        URL(fileURLWithPath: try value(key)).standardizedFileURL
    }

    func integer(_ key: String) throws -> Int {
        guard let result = Int(try value(key)) else {
            throw AppleE2EBenchmarkError.invalidArgument("--\(key) must be an integer")
        }
        return result
    }
}
