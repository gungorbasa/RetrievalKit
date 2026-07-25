import Foundation
import UIKit
import RetrievalKitFFI

@MainActor
final class BenchmarkViewModel: ObservableObject {
    @Published private(set) var isRunning = false
    @Published private(set) var status = "Ready"
    @Published private(set) var summary = "Run Device on physical hardware for the validation report."
    @Published private(set) var output = "{}"
    @Published private(set) var memoryScenarioRequiresRelaunch = false

    let memoryPresets = MemoryScenarioPreset.all

    func runLaunchScenarioIfPresent() async {
        let arguments = ProcessInfo.processInfo.arguments
        let isAutomatedBenchmarkLaunch = arguments.contains("--phase4-graph-free-regression")
            || arguments.contains("--memory-scenario")
        guard isAutomatedBenchmarkLaunch else {
            return
        }
        guard await ForegroundExecutionGate.waitUntilActive(
            isActive: { UIApplication.shared.applicationState == .active }
        ) else {
            let result = """
            {"ok":false,"error":"automated benchmark did not reach foreground before timeout","foreground":false,"foreground_wait_outside_measurement":true}
            """
            output = result
            status = "Foreground timeout"
            summary = "The automated benchmark never reached the foreground."
            writeBenchmarkResultToStandardOutput(result)
            exit(2)
        }
        if arguments.contains("--phase4-graph-free-regression") {
            UIApplication.shared.isIdleTimerDisabled = true
            let result = await runGraphFreeRegression()
            writeBenchmarkResultToStandardOutput(result)
            exit(responseSucceeded(result) ? EXIT_SUCCESS : 2)
        }
        guard
            !memoryScenarioRequiresRelaunch,
            let flagIndex = arguments.firstIndex(of: "--memory-scenario"),
            arguments.indices.contains(flagIndex + 1),
            let preset = MemoryScenarioPreset.find(
                id: arguments[flagIndex + 1]
            )
        else {
            return
        }
        guard !preset.isStress || arguments.contains("--phase4-100k-preflight-safe") else {
            let status = arguments.contains("--phase4-100k-preflight-unsafe")
                ? "not_run_memory_safety"
                : "not_run_preflight_required"
            let result = stressNotRunJSON(preset: preset, status: status)
            output = prettyPrintedJSON(result)
            self.status = status
            summary = "100K is outside the V1 supported envelope and was not executed: \(status)."
            writeBenchmarkResultToStandardOutput(result)
            exit(status == "not_run_memory_safety" ? EXIT_SUCCESS : 3)
        }
        run(.memory(preset))
    }

    func run(_ mode: BenchmarkMode) {
        guard !isRunning else {
            return
        }
        if case .memory = mode {
            guard !memoryScenarioRequiresRelaunch else {
                status = "Relaunch required"
                summary = "A benchmark already ran in this process. Relaunch the app, then run the memory preset first."
                return
            }
        }
        if case .memory(let preset) = mode,
           preset.isStress,
           !ProcessInfo.processInfo.arguments.contains("--phase4-100k-preflight-safe") {
            let result = stressNotRunJSON(preset: preset, status: "not_run_preflight_required")
            output = prettyPrintedJSON(result)
            status = "Preflight required"
            summary = "Run the Phase 4b persisted-size and memory preflight before attempting the experimental 100K workload."
            return
        }
        memoryScenarioRequiresRelaunch = true

        isRunning = true
        status = "Running \(mode.title)"
        summary = "Running..."
        output = "{}"

        let configJSON = mode.configJSON(device: UIDevice.current)
        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) {
                    switch mode {
                    case .realData:
                        try runRealDataSearch()
                    case .memory:
                        try runRetrievalKitMemoryBenchmark(configJSON: configJSON)
                    default:
                        try runRetrievalKitBenchmark(configJSON: configJSON)
                    }
                }.value

                output = prettyPrintedJSON(result)
                summary = benchmarkSummary(result, mode: mode)
                if !responseSucceeded(result) {
                    status = "Benchmark returned an error"
                } else if case .memory = mode, !memoryBudgetsPassed(result) {
                    status = "Budget exceeded"
                } else {
                    status = "Completed \(mode.title)"
                }

                if case .memory = mode,
                   ProcessInfo.processInfo.arguments.contains("--memory-scenario") {
                    writeBenchmarkResultToStandardOutput(result)
                    exit(memoryBudgetsPassed(result) ? EXIT_SUCCESS : 2)
                }
            } catch {
                output = "\(error)"
                summary = "\(error)"
                status = "Failed"
            }

            isRunning = false
        }
    }

    private func runGraphFreeRegression() async -> String {
        let arguments = ProcessInfo.processInfo.arguments
        guard let encoding = value(after: "--phase4-encoding", in: arguments),
              ["f32", "i8"].contains(encoding),
              let sessionID = value(after: "--phase4-session", in: arguments),
              let product = value(after: "--phase4-product", in: arguments),
              ["baseline", "candidate"].contains(product) else {
            return prettyPrintedJSON(
                "{\"ok\":false,\"error\":\"encoding, session, and baseline/candidate product are required\"}"
            )
        }
        let config: [String: Any] = [
            "encoding": encoding,
            "session_id": sessionID,
            "product": product
        ]
        let device = UIDevice.current
        device.isBatteryMonitoringEnabled = true
        let thermalStart = graphFreeThermalState(ProcessInfo.processInfo.thermalState)
        let batteryStart = Double(device.batteryLevel)
        guard let data = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys]),
              let configJSON = String(data: data, encoding: .utf8) else {
            return "{\"ok\":false,\"error\":\"graph-free config serialization failed\"}"
        }
        guard let runnerResponse = await Task.detached(priority: .userInitiated, operation: {
            invokeGraphFreeRegression(configJSON)
        }).value else {
            return "{\"ok\":false,\"error\":\"graph-free runner returned null\"}"
        }
        guard let responseData = runnerResponse.data(using: .utf8),
              var response = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            return "{\"ok\":false,\"error\":\"graph-free runner returned malformed JSON\"}"
        }
        response["device_role"] = value(after: "--phase4-device-role", in: arguments) ?? "unregistered"
        response["environment"] = graphFreeEnvironment(
            thermalStart: thermalStart,
            batteryStart: batteryStart
        )
        guard let encoded = try? JSONSerialization.data(
            withJSONObject: response,
            options: [.prettyPrinted, .sortedKeys]
        ) else {
            return "{\"ok\":false,\"error\":\"graph-free response serialization failed\"}"
        }
        return String(decoding: encoded, as: UTF8.self)
    }
}

private func invokeGraphFreeRegression(_ configJSON: String) -> String? {
    let pointer = configJSON.withCString {
        retrievalkit_phase4_graph_free_regression_json($0)
    }
    guard let pointer else {
        return nil
    }
    defer { retrievalkit_string_free(pointer) }
    return String(cString: pointer)
}

private func value(after flag: String, in arguments: [String]) -> String? {
    guard let index = arguments.firstIndex(of: flag), arguments.indices.contains(index + 1) else {
        return nil
    }
    return arguments[index + 1]
}

@MainActor
private func graphFreeEnvironment(thermalStart: String, batteryStart: Double) -> [String: Any] {
    let device = UIDevice.current
    device.isBatteryMonitoringEnabled = true
    #if targetEnvironment(simulator)
    let physical = false
    #else
    let physical = true
    #endif
    #if DEBUG
    let configuration = "debug"
    #else
    let configuration = "release"
    #endif
    return [
        "build_configuration": configuration,
        "physical_device": physical,
        "simulator": !physical,
        "device_identifier": graphFreeHardwareIdentifier(),
        "hardware_model": graphFreeSysctlString("hw.model"),
        "os_version": device.systemVersion,
        "os_build": ProcessInfo.processInfo.operatingSystemVersionString,
        "physical_memory_bytes": ProcessInfo.processInfo.physicalMemory,
        "process_id": Int(ProcessInfo.processInfo.processIdentifier),
        "one_scenario_per_fresh_process": true,
        "thermal_state_start": thermalStart,
        "thermal_state_end": graphFreeThermalState(ProcessInfo.processInfo.thermalState),
        "battery_level_start": batteryStart,
        "battery_level_end": Double(device.batteryLevel),
        "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
        "foreground": UIApplication.shared.applicationState == .active,
        "network_disabled": true
    ]
}

private func graphFreeHardwareIdentifier() -> String {
    var systemInfo = utsname()
    uname(&systemInfo)
    return withUnsafePointer(to: &systemInfo.machine) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: 1) { String(cString: $0) }
    }
}

private func graphFreeSysctlString(_ name: String) -> String {
    var size = 0
    guard sysctlbyname(name, nil, &size, nil, 0) == 0, size > 0 else {
        return "unknown"
    }
    var value = [CChar](repeating: 0, count: size)
    guard sysctlbyname(name, &value, &size, nil, 0) == 0 else {
        return "unknown"
    }
    let bytes = value.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return String(decoding: bytes, as: UTF8.self)
}

private func graphFreeThermalState(_ state: ProcessInfo.ThermalState) -> String {
    switch state {
    case .nominal: "nominal"
    case .fair: "fair"
    case .serious: "serious"
    case .critical: "critical"
    @unknown default: "unknown"
    }
}

private func writeBenchmarkResultToStandardOutput(_ result: String) {
    guard let data = "\(result)\n".data(using: .utf8) else {
        return
    }
    FileHandle.standardOutput.write(data)
    try? FileHandle.standardOutput.synchronize()
}

enum BenchmarkMode: Equatable {
    case realData
    case memory(MemoryScenarioPreset)
    case smallSmoke
    case deviceValidation
    case fullDefault
    case compactDefault

    var title: String {
        switch self {
        case .realData:
            return "real data"
        case .memory(let preset):
            return preset.id
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

    @MainActor
    func configJSON(device: UIDevice) -> String? {
        switch self {
        case .realData:
            return nil
        case .memory(let preset):
            return preset.configJSON(device: device)
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

struct MemoryScenarioPreset: Identifiable, Equatable {
    let id: String
    let chunks: Int
    let dimension: Int
    let encoding: String
    let workload: String
    let tombstoneRatio: Double

    var isStress: Bool { chunks == 100_000 }

    var title: String {
        "\(chunks / 1000)K · \(dimension)d · \(encoding.uppercased()) · \(workload == "hybrid" ? "hybrid" : "vector") · t\(Int(tombstoneRatio * 100))"
    }

    @MainActor
    func configJSON(device: UIDevice) -> String? {
        #if DEBUG
        let buildConfiguration = "debug"
        #else
        let buildConfiguration = "release"
        #endif
        var budgets: [String: Any] = isStress ? [:] : [
            "max_search_p95_ms": chunks <= 24_000 ? 10.0 : 20.0
        ]
        if chunks == 24_000 && dimension == 384 && encoding == "i8" {
            budgets["max_persisted_mib"] = 20.0
        }
        if chunks == 24_000 && dimension == 384 && encoding == "i8" && workload == "hybrid" {
            budgets["max_peak_rss_mib"] = 140.0
            budgets["max_peak_delta_mib"] = 96.0
            budgets["max_compaction_peak_increase_mib"] = 8.0
        }
        if chunks == 24_000 && dimension == 768 && encoding == "i8" && workload == "hybrid" {
            budgets["max_peak_rss_mib"] = 184.0
            budgets["max_peak_delta_mib"] = 136.0
            budgets["max_persisted_mib"] = 20.0
            budgets["max_compaction_peak_increase_mib"] = 8.0
        }
        if chunks == 50_000 && dimension == 384 && encoding == "i8" && workload == "hybrid" {
            budgets["max_peak_rss_mib"] = 224.0
            budgets["max_peak_delta_mib"] = 180.0
            budgets["max_persisted_mib"] = 24.0
            budgets["max_search_p95_ms"] = 16.0
            budgets["max_compaction_peak_increase_mib"] = 40.0
        }

        var config: [String: Any] = [
            "scenario_id": id,
            "chunks": chunks,
            "dimension": dimension,
            "encoding": encoding,
            "workload": workload,
            "queries": isStress ? 1_000 : 50,
            "warmup_queries": isStress ? 100 : 3,
            "top_k": 10,
            "vector_candidates": 50,
            "keyword_candidates": 50,
            "tombstone_ratio": tombstoneRatio,
            "sample_interval_ms": 1,
            "budgets": budgets,
            "environment": [
                "device_model": device.model,
                "os_version": "\(device.systemName) \(device.systemVersion)",
                "build_configuration": buildConfiguration,
                "app_version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown",
                "device_identifier": hardwareIdentifier(),
                "physical_device": isPhysicalDevice,
                "simulator": !isPhysicalDevice,
                "thermal_state_start": thermalStateName(ProcessInfo.processInfo.thermalState),
                "thermal_state_end": thermalStateName(ProcessInfo.processInfo.thermalState),
                "power_state": powerState(device),
                "battery_level_start": batteryLevel(device),
                "battery_level_end": batteryLevel(device),
                "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
                "free_storage_bytes": freeStorageBytes(),
                "foreground": UIApplication.shared.applicationState == .active,
                "network_disabled": true,
                "process_id": Int(ProcessInfo.processInfo.processIdentifier),
                "one_scenario_per_fresh_process": true,
                "graph_state_creations": 0,
                "graph_file_opens": 0,
                "graph_dispatches": 0
            ]
        ]
        if isStress {
            config["workload_id"] = "100k-384d-v3-stress"
            config["classification"] = "stress"
            config["marketing_eligible"] = false
        }
        guard
            let data = try? JSONSerialization.data(withJSONObject: config, options: [.sortedKeys]),
            let json = String(data: data, encoding: .utf8)
        else {
            return nil
        }
        return json
    }

    static let all: [Self] = {
        var presets: [Self] = []
        for chunks in [24_000, 50_000] {
            for dimension in [384, 768] {
                for encoding in ["f32", "f16", "i8"] {
                    for workload in ["vector_only", "hybrid"] {
                        presets.append(Self(
                            id: "\(chunks / 1000)k-\(dimension)d-\(encoding)-\(workload)-t25",
                            chunks: chunks,
                            dimension: dimension,
                            encoding: encoding,
                            workload: workload,
                            tombstoneRatio: 0.25
                        ))
                    }
                }
            }
        }
        for ratio in [0.10, 0.50] {
            presets.append(Self(
                id: "24k-384d-i8-hybrid-t\(Int(ratio * 100))",
                chunks: 24_000,
                dimension: 384,
                encoding: "i8",
                workload: "hybrid",
                tombstoneRatio: ratio
            ))
        }
        for encoding in ["f32", "i8"] {
            presets.append(Self(
                id: "100k-384d-v3-stress-\(encoding)",
                chunks: 100_000,
                dimension: 384,
                encoding: encoding,
                workload: "hybrid",
                tombstoneRatio: 0.01
            ))
        }
        return presets
    }()

    static func find(id: String) -> Self? {
        if id == "smoke" {
            return Self(
                id: "smoke",
                chunks: 128,
                dimension: 32,
                encoding: "i8",
                workload: "hybrid",
                tombstoneRatio: 0.25
            )
        }
        return all.first { $0.id == id }
    }
}

private func stressNotRunJSON(preset: MemoryScenarioPreset, status: String) -> String {
    let row: [String: Any] = [
        "ok": true,
        "report": [
            "schema_version": 1,
            "workload_id": "100k-384d-v3-stress",
            "scenario_id": preset.id,
            "classification": "stress",
            "marketing_eligible": false,
            "active_chunks": 100_000,
            "status": status,
            "supported_v1_capacity_changed": false
        ]
    ]
    guard let data = try? JSONSerialization.data(withJSONObject: row, options: [.sortedKeys]),
          let json = String(data: data, encoding: .utf8) else {
        return "{\"ok\":false,\"error\":\"failed to serialize 100K preflight row\"}"
    }
    return json
}

private var isPhysicalDevice: Bool {
    #if targetEnvironment(simulator)
    false
    #else
    true
    #endif
}

private func hardwareIdentifier() -> String {
    var systemInfo = utsname()
    uname(&systemInfo)
    return withUnsafePointer(to: &systemInfo.machine) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: 1) { String(cString: $0) }
    }
}

private func thermalStateName(_ state: ProcessInfo.ThermalState) -> String {
    switch state {
    case .nominal: "nominal"
    case .fair: "fair"
    case .serious: "serious"
    case .critical: "critical"
    @unknown default: "unknown"
    }
}

@MainActor
private func batteryLevel(_ device: UIDevice) -> Double {
    device.isBatteryMonitoringEnabled = true
    return Double(device.batteryLevel)
}

@MainActor
private func powerState(_ device: UIDevice) -> String {
    device.isBatteryMonitoringEnabled = true
    return switch device.batteryState {
    case .charging: "charging"
    case .full: "full"
    case .unplugged: "battery"
    case .unknown: "unknown"
    @unknown default: "unknown"
    }
}

private func freeStorageBytes() -> Int64 {
    let values = try? URL(fileURLWithPath: NSHomeDirectory()).resourceValues(
        forKeys: [.volumeAvailableCapacityForImportantUsageKey]
    )
    return values?.volumeAvailableCapacityForImportantUsage ?? -1
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

private func runRetrievalKitBenchmark(configJSON: String?) throws -> String {
    let resultPointer: UnsafeMutablePointer<CChar>?

    if let configJSON {
        resultPointer = configJSON.withCString { pointer in
            retrievalkit_bench_synthetic_json(pointer)
        }
    } else {
        resultPointer = retrievalkit_bench_synthetic_json(nil)
    }

    guard let resultPointer else {
        throw BenchmarkError.nullResult
    }
    defer {
        retrievalkit_string_free(resultPointer)
    }

    guard let result = String(validatingCString: resultPointer) else {
        throw BenchmarkError.invalidUtf8
    }

    return result
}

private func runRetrievalKitMemoryBenchmark(configJSON: String?) throws -> String {
    let resultPointer: UnsafeMutablePointer<CChar>?
    if let configJSON {
        resultPointer = configJSON.withCString { pointer in
            retrievalkit_bench_memory_json(pointer)
        }
    } else {
        resultPointer = retrievalkit_bench_memory_json(nil)
    }
    guard let resultPointer else {
        throw BenchmarkError.nullResult
    }
    defer { retrievalkit_string_free(resultPointer) }
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

    var status = RetrievalKitStatus(code: 0, message: nil)
    let loadStart = DispatchTime.now()
    let index = indexURL.path.withCString { path in
        retrievalkit_index_load(path, &status)
    }
    let loadMS = elapsedMilliseconds(since: loadStart)
    defer {
        retrievalkit_status_clear(&status)
    }
    guard let index else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        retrievalkit_index_free(index)
    }

    let dimension = Int(retrievalkit_index_dimension(index))
    let activeChunks = Int(retrievalkit_index_active_chunk_count(index))
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
    var output = RetrievalKitSearchResultBuffer(
        hits: nil, count: 0, utf8: nil, utf8_len: 0, metadata: nil, metadata_count: 0)
    var status = RetrievalKitStatus(code: 0, message: nil)
    defer {
        retrievalkit_status_clear(&status)
    }

    let succeeded = embedding.withUnsafeBufferPointer { buffer in
        retrievalkit_index_search(
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
        retrievalkit_search_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    let decoder = PackedResultDecoder(output)
    return try UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: try decoder.string(hit.document_id),
            score: hit.score,
            vectorScore: hit.vector_score,
            keywordScore: nil,
            matchedTerms: [],
            textPreview: preview(try decoder.string(hit.text))
        )
    }
}

private func keywordSearch(
    index: OpaquePointer,
    text: String,
    topK: Int,
    filter: OpaquePointer?
) throws -> [RealDataHit] {
    var output = RetrievalKitKeywordResultBuffer(
        hits: nil, count: 0, utf8: nil, utf8_len: 0,
        matched_terms: nil, matched_terms_count: 0,
        metadata: nil, metadata_count: 0)
    var status = RetrievalKitStatus(code: 0, message: nil)
    defer {
        retrievalkit_status_clear(&status)
    }

    let succeeded = text.withCString { query in
        retrievalkit_index_keyword_search(index, query, topK, filter, &output, &status)
    }
    guard succeeded else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        retrievalkit_keyword_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    let decoder = PackedResultDecoder(output)
    return try UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: try decoder.string(hit.document_id),
            score: hit.score,
            vectorScore: nil,
            keywordScore: hit.score,
            matchedTerms: try decoder.strings(
                start: hit.matched_terms_start, count: hit.matched_terms_count),
            textPreview: preview(try decoder.string(hit.text))
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
    var output = RetrievalKitHybridResultBuffer(
        hits: nil, count: 0, utf8: nil, utf8_len: 0,
        matched_terms: nil, matched_terms_count: 0,
        metadata: nil, metadata_count: 0, alpha: 0)
    var status = RetrievalKitStatus(code: 0, message: nil)
    let options = RetrievalKitHybridQueryOptions(
        vector_top_k: 50,
        keyword_top_k: 50,
        alpha: 0.6
    )
    defer {
        retrievalkit_status_clear(&status)
    }

    let succeeded = text.withCString { query in
        embedding.withUnsafeBufferPointer { buffer in
            retrievalkit_index_hybrid_search_alpha(
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
        retrievalkit_hybrid_results_free(output)
    }
    guard let hits = output.hits else {
        return []
    }

    let decoder = PackedResultDecoder(output)
    return try UnsafeBufferPointer(start: hits, count: output.count).enumerated().map { offset, hit in
        RealDataHit(
            rank: offset + 1,
            chunkID: hit.chunk_id,
            documentID: try decoder.string(hit.document_id),
            score: hit.score,
            vectorScore: hit.has_vector_score ? hit.vector_score : nil,
            keywordScore: hit.has_keyword_score ? hit.keyword_score : nil,
            matchedTerms: try decoder.strings(
                start: hit.matched_terms_start, count: hit.matched_terms_count),
            textPreview: preview(try decoder.string(hit.text))
        )
    }
}

private func withKindFilter<T>(_ kind: String, body: (OpaquePointer) throws -> T) throws -> T {
    var status = RetrievalKitStatus(code: 0, message: nil)
    let filter = "kind".withCString { field in
        kind.withCString { value in
            retrievalkit_filter_equals(
                field,
                RetrievalKitMetadataValue(
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
        retrievalkit_status_clear(&status)
    }
    guard let filter else {
        throw BenchmarkError.ffi(statusDescription(status))
    }
    defer {
        retrievalkit_filter_free(filter)
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

private func statusDescription(_ status: RetrievalKitStatus) -> String {
    status.message.map { String(cString: $0) } ?? "unknown RetrievalKit FFI error"
}

private struct PackedResultDecoder {
    private let utf8: UnsafePointer<UInt8>?
    private let utf8Count: Int
    private let matchedTerms: UnsafePointer<RetrievalKitUtf8Range>?
    private let matchedTermsCount: Int

    init(_ output: RetrievalKitSearchResultBuffer) {
        utf8 = output.utf8
        utf8Count = output.utf8_len
        matchedTerms = nil
        matchedTermsCount = 0
    }

    init(_ output: RetrievalKitKeywordResultBuffer) {
        utf8 = output.utf8
        utf8Count = output.utf8_len
        matchedTerms = output.matched_terms
        matchedTermsCount = output.matched_terms_count
    }

    init(_ output: RetrievalKitHybridResultBuffer) {
        utf8 = output.utf8
        utf8Count = output.utf8_len
        matchedTerms = output.matched_terms
        matchedTermsCount = output.matched_terms_count
    }

    func string(_ range: RetrievalKitUtf8Range) throws -> String {
        guard
            utf8Count >= 0,
            range.offset >= 0,
            range.length >= 0,
            range.offset <= utf8Count,
            range.length <= utf8Count - range.offset
        else {
            throw BenchmarkError.ffi("native result contains an invalid UTF-8 range")
        }
        guard range.length > 0 else {
            return ""
        }
        guard let utf8 else {
            throw BenchmarkError.ffi("native result UTF-8 arena is missing")
        }
        let bytes = UnsafeBufferPointer(
            start: utf8.advanced(by: range.offset), count: range.length)
        guard let value = String(bytes: bytes, encoding: .utf8) else {
            throw BenchmarkError.ffi("native result contains invalid UTF-8")
        }
        return value
    }

    func strings(start: Int, count: Int) throws -> [String] {
        guard
            matchedTermsCount >= 0,
            start >= 0,
            count >= 0,
            start <= matchedTermsCount,
            count <= matchedTermsCount - start
        else {
            throw BenchmarkError.ffi("native result contains an invalid matched-term range")
        }
        guard count > 0 else {
            return []
        }
        guard let matchedTerms else {
            throw BenchmarkError.ffi("native result matched-term ranges are missing")
        }
        return try UnsafeBufferPointer(
            start: matchedTerms.advanced(by: start), count: count
        ).map {
            try string($0)
        }
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

private func memoryBudgetsPassed(_ json: String) -> Bool {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let report = object["report"] as? [String: Any],
        let budgets = report["budgets"] as? [String: Any],
        let passed = budgets["passed"] as? Bool
    else {
        return false
    }
    return passed
}

@MainActor
private func benchmarkSummary(_ json: String, mode: BenchmarkMode) -> String {
    if mode == .realData {
        return realDataSummary(json)
    }

    if case .memory = mode {
        return memoryBenchmarkSummary(json)
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

@MainActor
private func memoryBenchmarkSummary(_ json: String) -> String {
    guard
        let data = json.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        object["ok"] as? Bool == true,
        let report = object["report"] as? [String: Any],
        let scenario = report["scenario"] as? [String: Any],
        let budgets = report["budgets"] as? [String: Any]
    else {
        return "No successful memory benchmark report."
    }

    let peakMiB = bytesToMiB(report["peak_rss_bytes"] as? Double)
    let deltaMiB = bytesToMiB(report["peak_delta_bytes"] as? Double)
    let files = report["persisted_file_sizes"] as? [String: Any]
    let persistedMiB = bytesToMiB(files?["total_bytes"] as? Double)
    let search = report["post_load_search"] as? [String: Any]
    let p95 = search?["p95_ms"] as? Double ?? 0
    let passed = budgets["passed"] as? Bool ?? false
    var lines = [
        "\(scenario["scenario_id"] as? String ?? "unknown") on \(UIDevice.current.model), \(UIDevice.current.systemName) \(UIDevice.current.systemVersion)",
        String(format: "peak RSS %.2f MiB · delta %.2f MiB · persisted %.2f MiB", peakMiB, deltaMiB, persistedMiB),
        String(format: "post-load search p95 %.3f ms · budgets %@", p95, passed ? "PASS" : "FAIL")
    ]
    if let violations = budgets["violations"] as? [String], !violations.isEmpty {
        lines.append(contentsOf: violations.map { "• \($0)" })
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
