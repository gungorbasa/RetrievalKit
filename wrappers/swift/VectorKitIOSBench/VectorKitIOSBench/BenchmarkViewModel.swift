import Foundation
import UIKit
import VectorKitFFI

@MainActor
final class BenchmarkViewModel: ObservableObject {
    @Published private(set) var isRunning = false
    @Published private(set) var status = "Ready"
    @Published private(set) var summary = "Run Device on physical hardware for the validation report."
    @Published private(set) var output = "{}"

    func run(_ mode: BenchmarkMode) {
        guard !isRunning else {
            return
        }

        isRunning = true
        status = "Running \(mode.title)"
        summary = "Running..."
        output = "{}"

        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    try runVectorKitBenchmark(configJSON: mode.configJSON)
                }.value

                output = prettyPrintedJSON(result)
                summary = benchmarkSummary(result, mode: mode)
                status = responseSucceeded(result) ? "Completed \(mode.title)" : "Benchmark returned an error"
            } catch {
                output = "\(error)"
                summary = "\(error)"
                status = "Failed"
            }

            isRunning = false
        }
    }
}

enum BenchmarkMode {
    case smallSmoke
    case deviceValidation
    case fullDefault
    case compactDefault

    var title: String {
        switch self {
        case .smallSmoke:
            return "smoke"
        case .deviceValidation:
            return "device"
        case .fullDefault:
            return "default"
        case .compactDefault:
            return "compact"
        }
    }

    var configJSON: String? {
        switch self {
        case .smallSmoke:
            return """
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
        case .deviceValidation:
            return """
            {
              "chunks": 24000,
              "dimensions": [384, 768],
              "queries": 200,
              "top_k": 10,
              "encodings": ["i8"],
              "metric": "cosine",
              "include_unfiltered": true,
              "include_filtered": true,
              "include_persistence": true,
              "include_recall": false,
              "persist_bm25": true,
              "filter_every": 10
            }
            """
        case .fullDefault:
            return nil
        case .compactDefault:
            return """
            {
              "persist_bm25": false
            }
            """
        }
    }
}

enum BenchmarkError: Error, CustomStringConvertible {
    case nullResult
    case invalidUtf8

    var description: String {
        switch self {
        case .nullResult:
            return "benchmark returned a null result pointer"
        case .invalidUtf8:
            return "benchmark returned invalid UTF-8"
        }
    }
}

private func runVectorKitBenchmark(configJSON: String?) throws -> String {
    let resultPointer: UnsafeMutablePointer<CChar>?

    if let configJSON {
        resultPointer = configJSON.withCString { pointer in
            vectorkit_bench_synthetic_json(pointer)
        }
    } else {
        resultPointer = vectorkit_bench_synthetic_json(nil)
    }

    guard let resultPointer else {
        throw BenchmarkError.nullResult
    }
    defer {
        vectorkit_string_free(resultPointer)
    }

    guard let result = String(validatingCString: resultPointer) else {
        throw BenchmarkError.invalidUtf8
    }

    return result
}

private func prettyPrintedJSON(_ json: String) -> String {
    guard
        let data = json.data(using: .utf8),
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

private func responseSucceeded(_ json: String) -> Bool {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let ok = object["ok"] as? Bool
    else {
        return false
    }

    return ok
}

@MainActor
private func benchmarkSummary(_ json: String, mode: BenchmarkMode) -> String {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let ok = object["ok"] as? Bool,
        ok,
        let report = object["report"] as? [String: Any],
        let runs = report["runs"] as? [[String: Any]]
    else {
        return "No successful benchmark report."
    }

    let device = UIDevice.current
    var lines = [
        "\(mode.title) on \(device.model), \(device.systemName) \(device.systemVersion)"
    ]

    if let capabilities = report["capabilities"] as? [String: Any] {
        let simsimd = capabilities["simsimd"] as? String ?? "unknown"
        let dotprod = capabilities["aarch64_dotprod"] as? Bool ?? false
        lines.append("capabilities: simsimd=\(simsimd), dotprod=\(dotprod)")
    }

    for run in runs {
        let dimension = run["dimension"] as? Int ?? 0
        let encoding = run["encoding"] as? String ?? "unknown"
        let filterLabel: String
        if let filterEvery = run["filter_every"] as? Int {
            filterLabel = "filter=1/\(filterEvery)"
        } else {
            filterLabel = "unfiltered"
        }

        let avgMS = run["avg_ms"] as? Double ?? 0
        let p95MS = run["p95_ms"] as? Double ?? 0
        let persistence = run["persistence"] as? [String: Any]
        let loadMS = persistence?["load_ms"] as? Double ?? 0
        let fileSizes = persistence?["file_sizes"] as? [String: Any]
        let persistedMiB = bytesToMiB(fileSizes?["total_bytes"] as? Double)
        let memoryAfterLoad = persistence?["memory_after_load"] as? [String: Any]
        let rssMiB = bytesToMiB(memoryAfterLoad?["resident_bytes"] as? Double)

        lines.append(
            "\(dimension)d \(encoding) \(filterLabel): avg \(format(avgMS)) ms, p95 \(format(p95MS)) ms, load \(format(loadMS)) ms, persisted \(format(persistedMiB)) MiB, rss-after-load \(format(rssMiB)) MiB"
        )
    }

    return lines.joined(separator: "\n")
}

private func bytesToMiB(_ bytes: Double?) -> Double {
    guard let bytes else {
        return 0
    }

    return bytes / 1_048_576
}

private func format(_ value: Double) -> String {
    String(format: "%.3f", value)
}
