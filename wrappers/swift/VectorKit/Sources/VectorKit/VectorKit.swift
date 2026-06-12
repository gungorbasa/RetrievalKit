import CVectorKitFFI
import Foundation

public enum VectorMetric: Sendable {
    case cosine
    case dotProduct

    fileprivate var ffiValue: UInt32 {
        switch self {
        case .cosine: 0
        case .dotProduct: 1
        }
    }
}

public enum VectorEncoding: Sendable {
    case f32
    case f16
    case bf16
    case i8ScalarQuantized

    fileprivate var ffiValue: UInt32 {
        switch self {
        case .f32: 0
        case .f16: 1
        case .bf16: 2
        case .i8ScalarQuantized: 3
        }
    }
}

public enum MetadataValue: Equatable, Sendable {
    case string(String)
    case integer(Int64)
    case float(Double)
    case boolean(Bool)
    case timestampMillis(Int64)

    fileprivate func ffiValue(arena: CStringArena) -> VkMetadataValue {
        switch self {
        case .string(let value):
            VkMetadataValue(
                value_type: 0,
                string_value: arena.copy(value),
                integer_value: 0,
                float_value: 0,
                bool_value: false
            )
        case .integer(let value):
            VkMetadataValue(
                value_type: 1,
                string_value: nil,
                integer_value: value,
                float_value: 0,
                bool_value: false
            )
        case .float(let value):
            VkMetadataValue(
                value_type: 2,
                string_value: nil,
                integer_value: 0,
                float_value: value,
                bool_value: false
            )
        case .boolean(let value):
            VkMetadataValue(
                value_type: 3,
                string_value: nil,
                integer_value: 0,
                float_value: 0,
                bool_value: value
            )
        case .timestampMillis(let value):
            VkMetadataValue(
                value_type: 4,
                string_value: nil,
                integer_value: value,
                float_value: 0,
                bool_value: false
            )
        }
    }
}

public struct Document: Equatable, Sendable {
    public var id: String
    public var text: String
    public var metadata: [String: MetadataValue]

    public init(id: String, text: String = "", metadata: [String: MetadataValue] = [:]) {
        self.id = id
        self.text = text
        self.metadata = metadata
    }
}

public struct ChunkInput: Equatable, Sendable {
    public var text: String
    public var embedding: [Float]
    public var metadata: [String: MetadataValue]

    public init(text: String, embedding: [Float], metadata: [String: MetadataValue] = [:]) {
        self.text = text
        self.embedding = embedding
        self.metadata = metadata
    }
}

public struct SearchResult: Equatable, Sendable {
    public var chunkID: UInt64
    public var documentID: String
    public var text: String
    public var score: Float
    public var trace: SearchTrace
}

public struct SearchTrace: Equatable, Sendable {
    public var vectorScore: Float
    public var filterMatched: Bool
}

public struct KeywordResult: Equatable, Sendable {
    public var chunkID: UInt64
    public var documentID: String
    public var text: String
    public var score: Float
    public var matchedTerms: [String]
}

public struct HybridResult: Equatable, Sendable {
    public var chunkID: UInt64
    public var documentID: String
    public var text: String
    public var score: Float
    public var vectorScore: Float?
    public var keywordScore: Float?
    public var trace: HybridTrace
}

public struct HybridTrace: Equatable, Sendable {
    public var vectorRank: Int?
    public var keywordRank: Int?
    public var normalizedVectorScore: Float?
    public var normalizedKeywordScore: Float?
    public var matchedTerms: [String]
    public var filterMatched: Bool
}

public struct HybridOptions: Equatable, Sendable {
    public enum Fusion: Equatable, Sendable {
        case weightedNormalizedScore(vectorWeight: Float, keywordWeight: Float)
        case reciprocalRank(rrfK: Float)
    }

    public var vectorTopK: Int
    public var keywordTopK: Int
    public var fusion: Fusion

    public static let `default` = HybridOptions()

    public init(
        vectorTopK: Int = 50,
        keywordTopK: Int = 50,
        fusion: Fusion = .weightedNormalizedScore(vectorWeight: 0.6, keywordWeight: 0.4)
    ) {
        self.vectorTopK = vectorTopK
        self.keywordTopK = keywordTopK
        self.fusion = fusion
    }

    fileprivate var ffiValue: VkHybridOptions {
        switch fusion {
        case .weightedNormalizedScore(let vectorWeight, let keywordWeight):
            VkHybridOptions(
                vector_top_k: vectorTopK,
                keyword_top_k: keywordTopK,
                fusion_type: 0,
                vector_weight: vectorWeight,
                keyword_weight: keywordWeight,
                rrf_k: 0
            )
        case .reciprocalRank(let rrfK):
            VkHybridOptions(
                vector_top_k: vectorTopK,
                keyword_top_k: keywordTopK,
                fusion_type: 1,
                vector_weight: 0,
                keyword_weight: 0,
                rrf_k: rrfK
            )
        }
    }
}

public enum VectorKitError: Error, Equatable, CustomStringConvertible, Sendable {
    case invalidArgument(String)
    case core(String)
    case panic(String)
    case unknown(code: Int32, message: String)

    public var description: String {
        switch self {
        case .invalidArgument(let message), .core(let message), .panic(let message):
            message
        case .unknown(let code, let message):
            "VectorKit error \(code): \(message)"
        }
    }

    fileprivate static func from(status: VkStatus) -> VectorKitError {
        let message = status.message.map { String(cString: $0) } ?? "unknown VectorKit FFI error"
        switch status.code {
        case 1: return .invalidArgument(message)
        case 2: return .core(message)
        case 3: return .panic(message)
        default: return .unknown(code: status.code, message: message)
        }
    }
}

public final class Filter {
    fileprivate let pointer: OpaquePointer

    private init(pointer: OpaquePointer) {
        self.pointer = pointer
    }

    deinit {
        vectorkit_filter_free(pointer)
    }

    public static func equals(_ field: String, _ value: MetadataValue) throws -> Filter {
        try make { status, arena in
            vectorkit_filter_equals(arena.copy(field), value.ffiValue(arena: arena), status)
        }
    }

    public static func notEquals(_ field: String, _ value: MetadataValue) throws -> Filter {
        try make { status, arena in
            vectorkit_filter_not_equals(arena.copy(field), value.ffiValue(arena: arena), status)
        }
    }

    public static func exists(_ field: String) throws -> Filter {
        try make { status, arena in
            vectorkit_filter_exists(arena.copy(field), status)
        }
    }

    public static func range(
        _ field: String,
        lower: MetadataValue? = nil,
        upper: MetadataValue? = nil
    ) throws -> Filter {
        try make { status, arena in
            var lowerValue = lower?.ffiValue(arena: arena)
            var upperValue = upper?.ffiValue(arena: arena)
            return withOptionalPointer(to: &lowerValue) { lowerPointer in
                withOptionalPointer(to: &upperValue) { upperPointer in
                    vectorkit_filter_range(arena.copy(field), lowerPointer, upperPointer, status)
                }
            }
        }
    }

    public static func inValues(_ field: String, _ values: [MetadataValue]) throws -> Filter {
        try make { status, arena in
            let ffiValues = values.map { $0.ffiValue(arena: arena) }
            return ffiValues.withUnsafeBufferPointer { buffer in
                vectorkit_filter_in_values(
                    arena.copy(field),
                    buffer.baseAddress,
                    buffer.count,
                    status
                )
            }
        }
    }

    public static func all(_ filters: [Filter]) throws -> Filter {
        try composite(filters, builder: vectorkit_filter_all)
    }

    public static func any(_ filters: [Filter]) throws -> Filter {
        try composite(filters, builder: vectorkit_filter_any)
    }

    private static func make(
        _ body: (UnsafeMutablePointer<VkStatus>, CStringArena) -> OpaquePointer?
    ) throws -> Filter {
        let pointer = try FFI.withStatusPointer { status in
            let arena = CStringArena()
            return body(status, arena)
        }
        return Filter(pointer: pointer)
    }

    private static func composite(
        _ filters: [Filter],
        builder: (
            UnsafePointer<OpaquePointer?>?,
            Int,
            UnsafeMutablePointer<VkStatus>?
        ) -> OpaquePointer?
    ) throws -> Filter {
        let pointer = try FFI.withStatusPointer { status in
            let pointers = filters.map { Optional($0.pointer) }
            return pointers.withUnsafeBufferPointer { buffer in
                builder(buffer.baseAddress, buffer.count, status)
            }
        }
        return Filter(pointer: pointer)
    }
}

public final class VectorIndex {
    private let pointer: OpaquePointer

    public var dimension: Int {
        Int(vectorkit_index_dimension(pointer))
    }

    public var activeChunkCount: Int {
        Int(vectorkit_index_active_chunk_count(pointer))
    }

    public init(
        dimension: Int,
        metric: VectorMetric = .cosine,
        encoding: VectorEncoding = .f32
    ) throws {
        pointer = try FFI.withStatusPointer { status in
            vectorkit_index_new(dimension, metric.ffiValue, encoding.ffiValue, status)
        }
    }

    private init(pointer: OpaquePointer) {
        self.pointer = pointer
    }

    deinit {
        vectorkit_index_free(pointer)
    }

    public static func load(from directory: URL) throws -> VectorIndex {
        let pointer = try FFI.withStatusPointer { status in
            directory.path.withCString { path in
                vectorkit_index_load(path, status)
            }
        }
        return VectorIndex(pointer: pointer)
    }

    public func save(to directory: URL, includeBM25: Bool = true) throws {
        try FFI.withStatusBool { status in
            directory.path.withCString { path in
                vectorkit_index_save(pointer, path, includeBM25, status)
            }
        }
    }

    @discardableResult
    public func upsert(document: Document, chunks: [ChunkInput]) throws -> [UInt64] {
        let arena = CStringArena()
        let documentMetadata = MetadataBuffer(document.metadata, arena: arena)
        let chunkMetadata = chunks.map { MetadataBuffer($0.metadata, arena: arena) }
        let embeddingBuffers = chunks.map { EmbeddingBuffer($0.embedding) }
        let ffiChunks = chunks.enumerated().map { index, chunk in
            VkChunkInput(
                text: arena.copy(chunk.text),
                embedding: embeddingBuffers[index].pointer,
                embedding_len: embeddingBuffers[index].count,
                metadata: chunkMetadata[index].pointer,
                metadata_len: chunkMetadata[index].count
            )
        }
        var output = VkChunkIdBuffer(values: nil, count: 0)

        try FFI.withStatusBool { status in
            ffiChunks.withUnsafeBufferPointer { chunkBuffer in
                vectorkit_index_upsert_document(
                    pointer,
                    arena.copy(document.id),
                    arena.copy(document.text),
                    documentMetadata.pointer,
                    documentMetadata.count,
                    chunkBuffer.baseAddress,
                    chunkBuffer.count,
                    &output,
                    status
                )
            }
        }

        defer { vectorkit_chunk_id_buffer_free(output) }
        guard let values = output.values else {
            return []
        }
        return Array(UnsafeBufferPointer(start: values, count: output.count))
    }

    @discardableResult
    public func deleteDocument(id: String) throws -> Int {
        let arena = CStringArena()
        var deletedCount = 0
        try FFI.withStatusBool { status in
            vectorkit_index_delete_document(pointer, arena.copy(id), &deletedCount, status)
        }
        return deletedCount
    }

    public func search(
        embedding: [Float],
        topK: Int = 10,
        filter: Filter? = nil
    ) throws -> [SearchResult] {
        var output = VkSearchResultBuffer(hits: nil, count: 0)
        try FFI.withStatusBool { status in
            embedding.withUnsafeBufferPointer { buffer in
                vectorkit_index_search(
                    pointer,
                    buffer.baseAddress,
                    buffer.count,
                    topK,
                    filter?.pointer,
                    &output,
                    status
                )
            }
        }
        defer { vectorkit_search_results_free(output) }
        guard let hits = output.hits else {
            return []
        }
        return UnsafeBufferPointer(start: hits, count: output.count).map(SearchResult.init)
    }

    public func keywordSearch(
        text: String,
        topK: Int = 10,
        filter: Filter? = nil
    ) throws -> [KeywordResult] {
        var output = VkKeywordResultBuffer(hits: nil, count: 0)
        try FFI.withStatusBool { status in
            text.withCString { textPointer in
                vectorkit_index_keyword_search(
                    pointer,
                    textPointer,
                    topK,
                    filter?.pointer,
                    &output,
                    status
                )
            }
        }
        defer { vectorkit_keyword_results_free(output) }
        guard let hits = output.hits else {
            return []
        }
        return UnsafeBufferPointer(start: hits, count: output.count).map(KeywordResult.init)
    }

    public func hybridSearch(
        text: String,
        embedding: [Float],
        topK: Int = 10,
        filter: Filter? = nil,
        options: HybridOptions = .default
    ) throws -> [HybridResult] {
        var output = VkHybridResultBuffer(hits: nil, count: 0)
        let ffiOptions = options.ffiValue
        try FFI.withStatusBool { status in
            text.withCString { textPointer in
                embedding.withUnsafeBufferPointer { buffer in
                    vectorkit_index_hybrid_search(
                        pointer,
                        textPointer,
                        buffer.baseAddress,
                        buffer.count,
                        topK,
                        filter?.pointer,
                        ffiOptions,
                        &output,
                        status
                    )
                }
            }
        }
        defer { vectorkit_hybrid_results_free(output) }
        guard let hits = output.hits else {
            return []
        }
        return UnsafeBufferPointer(start: hits, count: output.count).map(HybridResult.init)
    }
}

private extension SearchResult {
    init(_ hit: VkSearchHit) {
        self.init(
            chunkID: hit.chunk_id,
            documentID: string(from: hit.document_id),
            text: string(from: hit.text),
            score: hit.score,
            trace: SearchTrace(
                vectorScore: hit.vector_score,
                filterMatched: hit.filter_matched
            )
        )
    }
}

private extension KeywordResult {
    init(_ hit: VkKeywordHit) {
        self.init(
            chunkID: hit.chunk_id,
            documentID: string(from: hit.document_id),
            text: string(from: hit.text),
            score: hit.score,
            matchedTerms: strings(from: hit.matched_terms)
        )
    }
}

private extension HybridResult {
    init(_ hit: VkHybridHit) {
        self.init(
            chunkID: hit.chunk_id,
            documentID: string(from: hit.document_id),
            text: string(from: hit.text),
            score: hit.score,
            vectorScore: hit.has_vector_score ? hit.vector_score : nil,
            keywordScore: hit.has_keyword_score ? hit.keyword_score : nil,
            trace: HybridTrace(
                vectorRank: hit.has_vector_rank ? Int(hit.vector_rank) : nil,
                keywordRank: hit.has_keyword_rank ? Int(hit.keyword_rank) : nil,
                normalizedVectorScore: hit.has_normalized_vector_score ? hit.normalized_vector_score : nil,
                normalizedKeywordScore: hit.has_normalized_keyword_score ? hit.normalized_keyword_score : nil,
                matchedTerms: strings(from: hit.matched_terms),
                filterMatched: hit.filter_matched
            )
        )
    }
}

private enum FFI {
    static func withStatusPointer(
        _ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?
    ) throws -> OpaquePointer {
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        guard let pointer = body(&status) else {
            throw VectorKitError.from(status: status)
        }
        return pointer
    }

    static func withStatusBool(
        _ body: (UnsafeMutablePointer<VkStatus>) -> Bool
    ) throws {
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        guard body(&status) else {
            throw VectorKitError.from(status: status)
        }
    }
}

private final class CStringArena {
    private var pointers: [UnsafeMutablePointer<CChar>] = []

    deinit {
        for pointer in pointers {
            free(pointer)
        }
    }

    func copy(_ value: String) -> UnsafePointer<CChar>? {
        guard let pointer = strdup(value) else {
            return nil
        }
        pointers.append(pointer)
        return UnsafePointer(pointer)
    }
}

private final class EmbeddingBuffer {
    let pointer: UnsafePointer<Float>?
    let count: Int
    private let mutablePointer: UnsafeMutablePointer<Float>?

    init(_ values: [Float]) {
        count = values.count
        if values.isEmpty {
            mutablePointer = nil
            pointer = nil
        } else {
            let allocated = UnsafeMutablePointer<Float>.allocate(capacity: values.count)
            allocated.initialize(from: values, count: values.count)
            mutablePointer = allocated
            pointer = UnsafePointer(allocated)
        }
    }

    deinit {
        mutablePointer?.deinitialize(count: count)
        mutablePointer?.deallocate()
    }
}

private final class MetadataBuffer {
    let pointer: UnsafePointer<VkMetadataEntry>?
    let count: Int
    private let mutablePointer: UnsafeMutablePointer<VkMetadataEntry>?

    init(_ metadata: [String: MetadataValue], arena: CStringArena) {
        count = metadata.count
        if metadata.isEmpty {
            mutablePointer = nil
            pointer = nil
        } else {
            let entries = metadata
                .sorted { $0.key < $1.key }
                .map { field, value in
                    VkMetadataEntry(
                        field: arena.copy(field),
                        value: value.ffiValue(arena: arena)
                    )
                }
            let allocated = UnsafeMutablePointer<VkMetadataEntry>.allocate(capacity: entries.count)
            allocated.initialize(from: entries, count: entries.count)
            mutablePointer = allocated
            pointer = UnsafePointer(allocated)
        }
    }

    deinit {
        mutablePointer?.deinitialize(count: count)
        mutablePointer?.deallocate()
    }
}

private func withOptionalPointer<T, R>(
    to value: inout T?,
    _ body: (UnsafePointer<T>?) -> R
) -> R {
    guard var unwrapped = value else {
        return body(nil)
    }
    return withUnsafePointer(to: &unwrapped, body)
}

private func string(from pointer: UnsafeMutablePointer<CChar>?) -> String {
    guard let pointer else {
        return ""
    }
    return String(cString: pointer)
}

private func strings(from array: VkStringArray) -> [String] {
    guard let values = array.values else {
        return []
    }
    return UnsafeBufferPointer(start: values, count: array.count).map { pointer in
        pointer.map { String(cString: $0) } ?? ""
    }
}
