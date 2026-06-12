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
                    switch mode {
                    case .realData:
                        try runRealDataSearch()
                    default:
                        try runVectorKitBenchmark(configJSON: mode.configJSON)
                    }
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
    case realData
    case smallSmoke
    case deviceValidation
    case fullDefault
    case compactDefault

    var title: String {
        switch self {
        case .realData:
            return "real data"
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
        case .realData:
            return nil
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
    case missingResource(String)
    case ffi(String)

    var description: String {
        switch self {
        case .nullResult:
            return "benchmark returned a null result pointer"
        case .invalidUtf8:
            return "benchmark returned invalid UTF-8"
        case .missingResource(let name):
            return "missing bundled resource: \(name)"
        case .ffi(let message):
            return message
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

private func runRealDataSearch() throws -> String {
    guard let indexURL = Bundle.main.url(forResource: "social-network-index", withExtension: nil) else {
        throw BenchmarkError.missingResource("social-network-index")
    }
    guard let queryURL = Bundle.main.url(forResource: "social-network-query", withExtension: "json") else {
        throw BenchmarkError.missingResource("social-network-query.json")
    }

    let totalStart = DispatchTime.now()
    let query = try JSONDecoder().decode(RealDataQuery.self, from: Data(contentsOf: queryURL))
    guard query.dimension == query.embedding.count else {
        throw BenchmarkError.ffi("query embedding dimension \(query.embedding.count) does not match declared dimension \(query.dimension)")
    }

    var status = VkStatus(code: 0, message: nil)
    let loadStart = DispatchTime.now()
    let index = indexURL.path.withCString { path in
        vectorkit_index_load(path, &status)
    }
    let loadMS = elapsedMilliseconds(since: loadStart)
    defer {
        vectorkit_status_clear(&status)
    }
    guard let index else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        vectorkit_index_free(index)
    }

    let dimension = Int(vectorkit_index_dimension(index))
    let activeChunks = Int(vectorkit_index_active_chunk_count(index))
    guard dimension == query.embedding.count else {
        throw BenchmarkError.ffi("index dimension \(dimension) does not match query embedding dimension \(query.embedding.count)")
    }

    let vectorRun = try timed {
        try vectorSearch(index: index, embedding: query.embedding, topK: 5, filter: nil)
    }
    let keywordRun = try timed {
        try keywordSearch(index: index, text: query.query, topK: 5, filter: nil)
    }
    let hybridRun = try timed {
        try hybridSearch(index: index, text: query.query, embedding: query.embedding, topK: 5, filter: nil)
    }
    let filteredKeywordRun = try withKindFilter("shot") { filter in
        try timed {
            try keywordSearch(index: index, text: "Harvard campus at night", topK: 5, filter: filter)
        }
    }

    let report = RealDataResponse(
        ok: true,
        report: RealDataReport(
            indexPath: indexURL.lastPathComponent,
            query: query.query,
            model: query.model,
            dimension: dimension,
            activeChunks: activeChunks,
            loadMS: loadMS,
            totalMS: elapsedMilliseconds(since: totalStart),
            runs: [
                RealDataRun(mode: "vector", elapsedMS: vectorRun.elapsedMS, hits: vectorRun.value),
                RealDataRun(mode: "keyword", elapsedMS: keywordRun.elapsedMS, hits: keywordRun.value),
                RealDataRun(mode: "hybrid", elapsedMS: hybridRun.elapsedMS, hits: hybridRun.value),
                RealDataRun(mode: "keyword-filtered-shot", elapsedMS: filteredKeywordRun.elapsedMS, hits: filteredKeywordRun.value),
            ]
        )
    )
    let data = try JSONEncoder().encode(report)
    guard let json = String(data: data, encoding: .utf8) else {
        throw BenchmarkError.invalidUtf8
    }
    return json
}

private struct RealDataQuery: Decodable {
    var query: String
    var model: String
    var dimension: Int
    var embedding: [Float]
}

private struct RealDataResponse: Encodable {
    var ok: Bool
    var report: RealDataReport
}

private struct RealDataReport: Encodable {
    var indexPath: String
    var query: String
    var model: String
    var dimension: Int
    var activeChunks: Int
    var loadMS: Double
    var totalMS: Double
    var runs: [RealDataRun]
}

private struct RealDataRun: Encodable {
    var mode: String
    var elapsedMS: Double
    var hits: [RealDataHit]
}

private struct RealDataHit: Encodable {
    var rank: Int
    var chunkID: UInt64
    var documentID: String
    var score: Float
    var vectorScore: Float?
    var keywordScore: Float?
    var matchedTerms: [String]
    var textPreview: String
}

private func vectorSearch(
    index: OpaquePointer,
    embedding: [Float],
    topK: Int,
    filter: OpaquePointer?
) throws -> [RealDataHit] {
    var output = VkSearchResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer {
        vectorkit_status_clear(&status)
    }

    let succeeded = embedding.withUnsafeBufferPointer { buffer in
        vectorkit_index_search(
            index,
            buffer.baseAddress,
            buffer.count,
            topK,
            filter,
            &output,
            &status
        )
    }
    guard succeeded else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        vectorkit_search_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    return UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: string(hit.document_id),
            score: hit.score,
            vectorScore: hit.vector_score,
            keywordScore: nil,
            matchedTerms: [],
            textPreview: preview(string(hit.text))
        )
    }
}

private func keywordSearch(
    index: OpaquePointer,
    text: String,
    topK: Int,
    filter: OpaquePointer?
) throws -> [RealDataHit] {
    var output = VkKeywordResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer {
        vectorkit_status_clear(&status)
    }

    let succeeded = text.withCString { query in
        vectorkit_index_keyword_search(index, query, topK, filter, &output, &status)
    }
    guard succeeded else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        vectorkit_keyword_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    return UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: string(hit.document_id),
            score: hit.score,
            vectorScore: nil,
            keywordScore: hit.score,
            matchedTerms: strings(hit.matched_terms),
            textPreview: preview(string(hit.text))
        )
    }
}

private func hybridSearch(
    index: OpaquePointer,
    text: String,
    embedding: [Float],
    topK: Int,
    filter: OpaquePointer?
) throws -> [RealDataHit] {
    var output = VkHybridResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    let options = VkHybridOptions(
        vector_top_k: 50,
        keyword_top_k: 50,
        fusion_type: 0,
        vector_weight: 0.6,
        keyword_weight: 0.4,
        rrf_k: 0
    )
    defer {
        vectorkit_status_clear(&status)
    }

    let succeeded = text.withCString { query in
        embedding.withUnsafeBufferPointer { buffer in
            vectorkit_index_hybrid_search(
                index,
                query,
                buffer.baseAddress,
                buffer.count,
                topK,
                filter,
                options,
                &output,
                &status
            )
        }
    }
    guard succeeded else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        vectorkit_hybrid_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    return UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: string(hit.document_id),
            score: hit.score,
            vectorScore: hit.has_vector_score ? hit.vector_score : nil,
            keywordScore: hit.has_keyword_score ? hit.keyword_score : nil,
            matchedTerms: strings(hit.matched_terms),
            textPreview: preview(string(hit.text))
        )
    }
}

private func withKindFilter<T>(_ kind: String, body: (OpaquePointer) throws -> T) throws -> T {
    var status = VkStatus(code: 0, message: nil)
    let filter = "kind".withCString { field in
        kind.withCString { value in
            vectorkit_filter_equals(
                field,
                VkMetadataValue(
                    value_type: 0,
                    string_value: value,
                    integer_value: 0,
                    float_value: 0,
                    bool_value: false
                ),
                &status
            )
        }
    }
    defer {
        vectorkit_status_clear(&status)
    }
    guard let filter else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        vectorkit_filter_free(filter)
    }
    return try body(filter)
}

private func timed<T>(_ body: () throws -> T) rethrows -> (value: T, elapsedMS: Double) {
    let start = DispatchTime.now()
    let value = try body()
    return (value, elapsedMilliseconds(since: start))
}

private func elapsedMilliseconds(since start: DispatchTime) -> Double {
    let elapsed = DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds
    return Double(elapsed) / 1_000_000
}

private func statusDescription(_ status: VkStatus) -> String {
    status.message.map { String(cString: $0) } ?? "unknown VectorKit FFI error"
}

private func string(_ pointer: UnsafeMutablePointer<CChar>?) -> String {
    pointer.map { String(cString: $0) } ?? ""
}

private func strings(_ array: VkStringArray) -> [String] {
    guard let values = array.values else {
        return []
    }
    return UnsafeBufferPointer(start: values, count: array.count).map { pointer in
        pointer.map { String(cString: $0) } ?? ""
    }
}

private func preview(_ text: String, maxLength: Int = 240) -> String {
    guard text.count > maxLength else {
        return text
    }
    return String(text.prefix(maxLength)) + "..."
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
    if mode == .realData {
        return realDataSummary(json)
    }

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

private func realDataSummary(_ json: String) -> String {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let ok = object["ok"] as? Bool,
        ok,
        let report = object["report"] as? [String: Any],
        let activeChunks = report["activeChunks"] as? Int,
        let dimension = report["dimension"] as? Int,
        let loadMS = report["loadMS"] as? Double,
        let runs = report["runs"] as? [[String: Any]]
    else {
        return "No successful real-data report."
    }

    var lines = [
        "real index: \(activeChunks) chunks, \(dimension)d",
        "load: \(format(loadMS)) ms",
        "query: \(report["query"] as? String ?? "unknown")"
    ]

    for run in runs {
        let mode = run["mode"] as? String ?? "unknown"
        let elapsedMS = run["elapsedMS"] as? Double ?? 0
        let hits = run["hits"] as? [[String: Any]] ?? []
        let first = hits.first?["documentID"] as? String ?? "no hit"
        lines.append("\(mode): \(hits.count) hits in \(format(elapsedMS)) ms, first=\(first)")
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
