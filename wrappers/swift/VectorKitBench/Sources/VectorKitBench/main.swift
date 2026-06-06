import CVectorKitFFI
import Foundation

enum BenchHarnessError: Error, CustomStringConvertible {
    case missingValue(String)
    case unknownArgument(String)
    case unreadableConfigFile(String)
    case invalidUtf8Result
    case nullResult

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
        }
    }
}

struct CommandLineOptions {
    var configJSON: String?
    var prettyPrint = true
    var showHelp = false
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
        default:
            throw BenchHarnessError.unknownArgument(argument)
        }

        index += 1
    }

    return options
}

func runBenchmark(configJSON: String?) throws -> String {
    let resultPointer: UnsafeMutablePointer<CChar>?

    if let configJSON {
        resultPointer = configJSON.withCString { pointer in
            vectorkit_bench_synthetic_json(pointer)
        }
    } else {
        resultPointer = vectorkit_bench_synthetic_json(nil)
    }

    guard let resultPointer else {
        throw BenchHarnessError.nullResult
    }
    defer {
        vectorkit_string_free(resultPointer)
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
          vectorkit-bench [--small-smoke]
          vectorkit-bench --config '<json>'
          vectorkit-bench --config-file config.json

        options:
          --small-smoke       run a small link/smoke benchmark
          --config <json>     pass a benchmark config JSON string
          --config-file <p>   read benchmark config JSON from a file
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

    let json = try runBenchmark(configJSON: options.configJSON)
    print(formattedJSON(json, prettyPrint: options.prettyPrint))
    exit(responseSucceeded(json) ? EXIT_SUCCESS : EXIT_FAILURE)
} catch {
    fputs("error: \(error)\n", stderr)
    printUsage()
    exit(EXIT_FAILURE)
}
