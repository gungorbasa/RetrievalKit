import Foundation

public enum AppleE2EBenchmarkError: Error, CustomStringConvertible, Sendable {
    case invalidArgument(String)
    case invalidInput(String)
    case invalidState(String)

    public var description: String {
        switch self {
        case .invalidArgument(let message), .invalidInput(let message), .invalidState(let message):
            return message
        }
    }
}

public struct QuerySuite: Codable, Sendable {
    public let schemaVersion: Int
    public let seed: Int
    public let queries: [BenchmarkQuery]
    public let schedule: [String]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case seed, queries, schedule
    }
}

public struct BenchmarkQuery: Codable, Sendable {
    public let id: String
    public let category: String
    public let text: String
    public let expectedTokenBucket: TokenBucket
    public let targetRecordID: String?

    enum CodingKeys: String, CodingKey {
        case id, category, text
        case expectedTokenBucket = "expected_token_bucket"
        case targetRecordID = "target_record_id"
    }
}

public struct TokenBucket: Codable, Equatable, Hashable, Sendable {
    public let minimum: Int
    public let maximum: Int
}

public struct CorpusRecord: Codable, Sendable {
    public let recordID: String
    public let text: String
    public let metadata: [String: String]
    public let chunks: [CorpusChunk]

    enum CodingKeys: String, CodingKey {
        case recordID = "record_id"
        case text, metadata, chunks
    }
}

public struct CorpusChunk: Codable, Sendable {
    public let chunkKey: String
    public let text: String

    enum CodingKeys: String, CodingKey {
        case chunkKey = "chunk_key"
        case text
    }
}

public enum SearchMode: String, Codable, Sendable {
    case vector
    case weightedHybrid = "weighted_hybrid"
}

public struct StageSummary: Codable, Equatable, Sendable {
    public let count: Int
    public let minimumNS: UInt64
    public let maximumNS: UInt64
    public let meanNS: UInt64
    public let p50NS: UInt64
    public let p95NS: UInt64
    public let p99NS: UInt64

    enum CodingKeys: String, CodingKey {
        case count
        case minimumNS = "minimum_ns"
        case maximumNS = "maximum_ns"
        case meanNS = "mean_ns"
        case p50NS = "p50_ns"
        case p95NS = "p95_ns"
        case p99NS = "p99_ns"
    }

    public static func calculate(_ values: [UInt64]) throws -> StageSummary {
        guard !values.isEmpty else {
            throw AppleE2EBenchmarkError.invalidInput("cannot summarize an empty sample set")
        }
        let sorted = values.sorted()
        func nearestRank(_ percentile: Double) -> UInt64 {
            let rank = max(1, Int(ceil(percentile * Double(sorted.count))))
            return sorted[rank - 1]
        }
        let sum = values.reduce(0) { partial, value in partial + Double(value) }
        return StageSummary(
            count: values.count,
            minimumNS: sorted[0],
            maximumNS: sorted[sorted.count - 1],
            meanNS: UInt64((sum / Double(values.count)).rounded()),
            p50NS: nearestRank(0.50),
            p95NS: nearestRank(0.95),
            p99NS: nearestRank(0.99)
        )
    }
}

public struct RawSample: Codable, Sendable {
    public let ordinal: Int
    public let queryID: String
    public let startClockNS: UInt64
    public let endClockNS: UInt64
    public let embeddingNS: UInt64
    public let retrievalNS: UInt64
    public let endToEndNS: UInt64
    public let resultCount: Int
    public let topResultIdentity: String?
    public let resultIdentityDigest: String

    enum CodingKeys: String, CodingKey {
        case ordinal
        case queryID = "query_id"
        case startClockNS = "start_clock_ns"
        case endClockNS = "end_clock_ns"
        case embeddingNS = "embedding_total_ns"
        case retrievalNS = "retrieval_total_ns"
        case endToEndNS = "end_to_end_text_search_ns"
        case resultCount = "result_count"
        case topResultIdentity = "top_result_identity"
        case resultIdentityDigest = "result_identity_digest"
    }
}

public struct BenchmarkEnvironment: Codable, Sendable {
    public let platform: String
    public let hardware: String
    public let operatingSystem: String
    public let architecture: String
    public let processID: Int32
    public let debuggerAttached: Bool
    public let graphLinked: Bool
    public let onnxRuntimeLinked: Bool
    public let retrievalKitRevision: String
    public let modelPath: String
    public let indexPath: String
    public let runtimeName: String
    public let requestedCompute: String
    public let selectedSIMDBackend: String
    public let aarch64Dotprod: Bool
    public let compiledModelTreeSHA256: String
    public let indexTreeSHA256: String
    public let indexTreeBytes: UInt64
    public let querySuiteSHA256: String

    enum CodingKeys: String, CodingKey {
        case platform, hardware, architecture
        case operatingSystem = "operating_system"
        case processID = "process_id"
        case debuggerAttached = "debugger_attached"
        case graphLinked = "graph_linked"
        case onnxRuntimeLinked = "onnx_runtime_linked"
        case retrievalKitRevision = "retrievalkit_revision"
        case modelPath = "model_path"
        case indexPath = "index_path"
        case runtimeName = "runtime_name"
        case requestedCompute = "requested_compute"
        case selectedSIMDBackend = "selected_simd_backend"
        case aarch64Dotprod = "aarch64_dotprod"
        case compiledModelTreeSHA256 = "compiled_model_tree_sha256"
        case indexTreeSHA256 = "index_tree_sha256"
        case indexTreeBytes = "index_tree_bytes"
        case querySuiteSHA256 = "query_suite_sha256"
    }
}

public struct BenchmarkReport: Codable, Sendable {
    public let schemaVersion: Int
    public let contractVersion: String
    public let workloadID: String
    public let workloadClassification: String
    public let marketingEligible: Bool
    public let supportedV1CapacityChanged: Bool
    public let profileID: String
    public let profileClassification: String
    public let sessionID: String
    public let searchMode: SearchMode
    public let topK: Int
    public let warmupCount: Int
    public let samples: [RawSample]
    public let summaries: [String: StageSummary]
    public let environment: BenchmarkEnvironment
    public let iphoneValidity: IPhoneValidity?

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case contractVersion = "contract_version"
        case workloadID = "workload_id"
        case workloadClassification = "workload_classification"
        case marketingEligible = "marketing_eligible"
        case supportedV1CapacityChanged = "supported_v1_capacity_changed"
        case profileID = "profile_id"
        case profileClassification = "profile_classification"
        case sessionID = "session_id"
        case searchMode = "search_mode"
        case topK = "top_k"
        case warmupCount = "warmup_count"
        case samples, summaries, environment
        case iphoneValidity = "iphone_validity"
    }
}

public struct IPhoneValidity: Codable, Sendable {
    public let physicalDevice: Bool
    public let foregroundStart: Bool
    public let foregroundEnd: Bool
    public let networkDisabled: Bool
    public let lowPowerMode: Bool
    public let batteryPercent: Int
    public let batteryState: String
    public let charging: Bool
    public let thermalStart: String
    public let thermalEnd: String
    public let memoryWarning: Bool

    public init(
        physicalDevice: Bool,
        foregroundStart: Bool,
        foregroundEnd: Bool,
        networkDisabled: Bool,
        lowPowerMode: Bool,
        batteryPercent: Int,
        batteryState: String,
        charging: Bool,
        thermalStart: String,
        thermalEnd: String,
        memoryWarning: Bool
    ) {
        self.physicalDevice = physicalDevice
        self.foregroundStart = foregroundStart
        self.foregroundEnd = foregroundEnd
        self.networkDisabled = networkDisabled
        self.lowPowerMode = lowPowerMode
        self.batteryPercent = batteryPercent
        self.batteryState = batteryState
        self.charging = charging
        self.thermalStart = thermalStart
        self.thermalEnd = thermalEnd
        self.memoryWarning = memoryWarning
    }

    enum CodingKeys: String, CodingKey {
        case physicalDevice = "physical_device"
        case foregroundStart = "foreground_start"
        case foregroundEnd = "foreground_end"
        case networkDisabled = "network_disabled"
        case lowPowerMode = "low_power_mode"
        case batteryPercent = "battery_percent"
        case batteryState = "battery_state"
        case charging
        case thermalStart = "thermal_start"
        case thermalEnd = "thermal_end"
        case memoryWarning = "memory_warning"
    }
}

public struct RunConfiguration: Sendable {
    public let contractVersion: String
    public let queriesURL: URL
    public let indexURL: URL
    public let modelURL: URL
    public let tokenizerURL: URL
    public let outputURL: URL
    public let workloadID: String
    public let workloadClassification: String
    public let marketingEligible: Bool
    public let profileID: String
    public let profileClassification: String
    public let sessionID: String
    public let mode: SearchMode
    public let retrievalKitRevision: String
    public let iphoneValidityProvider: (@Sendable () -> IPhoneValidity?)?
    public let abortCheck: (@Sendable () -> String?)?

    public init(
        contractVersion: String = "apple-end-to-end-v1",
        queriesURL: URL,
        indexURL: URL,
        modelURL: URL,
        tokenizerURL: URL,
        outputURL: URL,
        workloadID: String,
        workloadClassification: String,
        marketingEligible: Bool,
        profileID: String,
        profileClassification: String,
        sessionID: String,
        mode: SearchMode,
        retrievalKitRevision: String,
        iphoneValidityProvider: (@Sendable () -> IPhoneValidity?)? = nil,
        abortCheck: (@Sendable () -> String?)? = nil
    ) {
        self.contractVersion = contractVersion
        self.queriesURL = queriesURL
        self.indexURL = indexURL
        self.modelURL = modelURL
        self.tokenizerURL = tokenizerURL
        self.outputURL = outputURL
        self.workloadID = workloadID
        self.workloadClassification = workloadClassification
        self.marketingEligible = marketingEligible
        self.profileID = profileID
        self.profileClassification = profileClassification
        self.sessionID = sessionID
        self.mode = mode
        self.retrievalKitRevision = retrievalKitRevision
        self.iphoneValidityProvider = iphoneValidityProvider
        self.abortCheck = abortCheck
    }
}

public struct QualityReport: Codable, Sendable {
    public let schemaVersion: Int
    public let queryCount: Int
    public let medianCosine: Double
    public let meanTop10Overlap: Double
    public let minimumTop10Overlap: Double
    public let exactTop10Rate: Double
    public let passed: Bool
    public let samples: [QualitySample]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case queryCount = "query_count"
        case medianCosine = "median_cosine"
        case meanTop10Overlap = "mean_top10_overlap"
        case minimumTop10Overlap = "minimum_top10_overlap"
        case exactTop10Rate = "exact_top10_rate"
        case passed, samples
    }
}

public struct QualitySample: Codable, Sendable {
    public let queryID: String
    public let cosine: Double
    public let top10Overlap: Double
    public let exactTop10: Bool

    enum CodingKeys: String, CodingKey {
        case queryID = "query_id"
        case cosine
        case top10Overlap = "top10_overlap"
        case exactTop10 = "exact_top10"
    }
}

public struct ProviderQualitySuite: Codable, Sendable {
    public let schemaVersion: Int
    public let queries: [ProviderQualityQuery]
    public let diagnostics: [String]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case queries, diagnostics
    }
}

public struct ProviderQualityQuery: Codable, Sendable {
    public let id: String
    public let text: String
}
