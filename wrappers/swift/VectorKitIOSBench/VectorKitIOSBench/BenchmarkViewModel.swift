import Foundation
import VectorKitFFI

@MainActor
final class BenchmarkViewModel: ObservableObject {
    @Published private(set) var isRunning = false
    @Published private(set) var status = "Ready"
    @Published private(set) var output = "{}"

    func run(_ mode: BenchmarkMode) {
        guard !isRunning else {
            return
        }

        isRunning = true
        status = "Running \(mode.title)"
        output = "{}"

        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    try runVectorKitBenchmark(configJSON: mode.configJSON)
                }.value

                output = prettyPrintedJSON(result)
                status = responseSucceeded(result) ? "Completed \(mode.title)" : "Benchmark returned an error"
            } catch {
                output = "\(error)"
                status = "Failed"
            }

            isRunning = false
        }
    }
}

enum BenchmarkMode {
    case smallSmoke
    case fullDefault
    case compactDefault

    var title: String {
        switch self {
        case .smallSmoke:
            return "smoke"
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
