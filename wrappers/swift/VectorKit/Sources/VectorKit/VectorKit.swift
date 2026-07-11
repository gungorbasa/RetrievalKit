import VectorKitFFI
import Foundation

/// Vector similarity metric used by a `VectorIndex`.
public enum VectorMetric: Sendable {
    /// L2-normalized cosine similarity.
    case cosine
    /// Raw dot product similarity.
    case dotProduct

    fileprivate var ffiValue: UInt32 {
        switch self {
        case .cosine: 0
        case .dotProduct: 1
        }
    }
}

/// Stored vector representation used by the Rust retrieval core.
public enum VectorEncoding: Sendable {
    /// Store vectors as 32-bit floats.
    case f32
    /// Store vectors as IEEE 16-bit floats.
    case f16
    /// Store vectors as bfloat16 values.
    case bf16
    /// Store vectors using per-vector scalar quantized 8-bit values.
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

/// Typed metadata value supported by VectorKit filters and indexing.
public enum MetadataValue: Equatable, Sendable {
    /// UTF-8 string metadata.
    case string(String)
    /// Signed integer metadata.
    case integer(Int64)
    /// Floating-point metadata.
    case float(Double)
    /// Boolean metadata.
    case boolean(Bool)
    /// Timestamp represented as milliseconds since the caller's chosen epoch.
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

/// Caller-owned document identity and metadata.
public struct Document: Equatable, Sendable {
    /// Stable caller-owned document identifier.
    public var id: String
    /// Optional document-level text retained for caller context.
    public var text: String
    /// Metadata merged into each indexed chunk unless overridden by chunk metadata.
    public var metadata: [String: MetadataValue]

    /// Creates a document descriptor for upsert operations.
    public init(id: String, text: String = "", metadata: [String: MetadataValue] = [:]) {
        self.id = id
        self.text = text
        self.metadata = metadata
    }
}

/// Caller-provided retrievable chunk data.
public struct ChunkInput: Equatable, Sendable {
    /// Text used for display and BM25 keyword indexing.
    public var text: String
    /// Embedding vector. Its length must match the index dimension.
    public var embedding: [Float]
    /// Chunk metadata. Values override document metadata with the same key.
    public var metadata: [String: MetadataValue]

    /// Creates a chunk input for indexing.
    public init(text: String, embedding: [Float], metadata: [String: MetadataValue] = [:]) {
        self.text = text
        self.embedding = embedding
        self.metadata = metadata
    }
}

/// Exact vector search result.
public struct SearchResult: Equatable, Sendable {
    /// Internal chunk identifier assigned by VectorKit.
    public var chunkID: UInt64
    /// Caller-owned document identifier.
    public var documentID: String
    /// Stored chunk text.
    public var text: String
    /// Ranked vector score.
    public var score: Float
    /// Debug data for the score and filter decision.
    public var trace: SearchTrace
}

/// Debug data for exact vector search.
public struct SearchTrace: Equatable, Sendable {
    /// Raw vector score used for ranking.
    public var vectorScore: Float
    /// Whether the metadata filter matched this result.
    public var filterMatched: Bool
}

/// BM25 keyword search result.
public struct KeywordResult: Equatable, Sendable {
    /// Internal chunk identifier assigned by VectorKit.
    public var chunkID: UInt64
    /// Caller-owned document identifier.
    public var documentID: String
    /// Stored chunk text.
    public var text: String
    /// BM25 score.
    public var score: Float
    /// Query terms matched by the keyword index.
    public var matchedTerms: [String]
}

/// Hybrid vector plus keyword search result.
public struct HybridResult: Equatable, Sendable {
    /// Internal chunk identifier assigned by VectorKit.
    public var chunkID: UInt64
    /// Caller-owned document identifier.
    public var documentID: String
    /// Stored chunk text.
    public var text: String
    /// Final fused hybrid score.
    public var score: Float
    /// Vector score when the chunk came from vector candidates.
    public var vectorScore: Float?
    /// Keyword score when the chunk came from keyword candidates.
    public var keywordScore: Float?
    /// Debug data for ranking and fusion.
    public var trace: HybridTrace
}

/// Debug data for hybrid search ranking.
public struct HybridTrace: Equatable, Sendable {
    /// Rank in vector candidates before fusion.
    public var vectorRank: Int?
    /// Rank in keyword candidates before fusion.
    public var keywordRank: Int?
    /// Normalized vector score used by weighted fusion, when available.
    public var normalizedVectorScore: Float?
    /// Normalized keyword score used by weighted fusion, when available.
    public var normalizedKeywordScore: Float?
    /// Query terms matched by the keyword side of hybrid search.
    public var matchedTerms: [String]
    /// Whether the metadata filter matched this result.
    public var filterMatched: Bool
}

/// Candidate and fusion options for hybrid search.
public struct HybridOptions: Equatable, Sendable {
    /// Hybrid rank fusion strategy.
    public enum Fusion: Equatable, Sendable {
        /// Fuse min-max normalized vector and keyword scores with explicit weights.
        case weightedNormalizedScore(vectorWeight: Float, keywordWeight: Float)
        /// Fuse candidates using reciprocal rank fusion.
        case reciprocalRank(rrfK: Float)
    }

    /// Number of vector candidates generated before fusion.
    public var vectorTopK: Int
    /// Number of keyword candidates generated before fusion.
    public var keywordTopK: Int
    /// Fusion strategy used to rank the final results.
    public var fusion: Fusion

    /// Stable V1 defaults: 50 vector candidates, 50 keyword candidates, 0.6/0.4 weighted fusion.
    public static let `default` = HybridOptions()

    /// Creates hybrid search options.
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

/// Error surfaced by the Swift wrapper.
public enum VectorKitError: Error, Equatable, CustomStringConvertible, Sendable {
    /// Invalid input before or at the FFI boundary.
    case invalidArgument(String)
    /// Typed error returned by the Rust core.
    case core(String)
    /// Panic caught at the FFI boundary.
    case panic(String)
    /// A persisted index failed an integrity check.
    case corruptIndex(String)
    /// Unknown FFI status code.
    case unknown(code: Int32, message: String)

    /// Human-readable error message.
    public var description: String {
        switch self {
        case .invalidArgument(let message), .core(let message), .panic(let message),
             .corruptIndex(let message):
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
        case 4: return .corruptIndex(message)
        default: return .unknown(code: status.code, message: message)
        }
    }
}

/// Summary of memory reclaimed by removing tombstoned chunks.
public struct CompactionReport: Equatable, Sendable {
    public let chunksBefore: Int
    public let chunksAfter: Int
    public let chunksRemoved: Int
    public let estimatedBytesBefore: Int
    public let estimatedBytesAfter: Int
    public let estimatedBytesReclaimed: Int

    public init(
        chunksBefore: Int,
        chunksAfter: Int,
        chunksRemoved: Int,
        estimatedBytesBefore: Int,
        estimatedBytesAfter: Int,
        estimatedBytesReclaimed: Int
    ) {
        self.chunksBefore = chunksBefore
        self.chunksAfter = chunksAfter
        self.chunksRemoved = chunksRemoved
        self.estimatedBytesBefore = estimatedBytesBefore
        self.estimatedBytesAfter = estimatedBytesAfter
        self.estimatedBytesReclaimed = estimatedBytesReclaimed
    }
}

/// Immutable metadata filter used by search APIs.
public indirect enum Filter: Equatable, Sendable {
    /// Matches chunks where a metadata field equals a value.
    case equals(field: String, value: MetadataValue)
    /// Matches chunks where a metadata field does not equal a value.
    case notEquals(field: String, value: MetadataValue)
    /// Matches chunks containing a metadata field.
    case exists(field: String)
    /// Matches chunks where a numeric or timestamp field is within an inclusive range.
    case range(field: String, lower: MetadataValue?, upper: MetadataValue?)
    /// Matches chunks where a metadata field equals any provided value.
    case inValues(field: String, values: [MetadataValue])
    /// Matches chunks where all child filters match.
    case all([Filter])
    /// Matches chunks where any child filter matches.
    case any([Filter])

    /// Creates an equality filter.
    public static func equals(_ field: String, _ value: MetadataValue) -> Filter {
        .equals(field: field, value: value)
    }

    /// Creates a not-equals filter.
    public static func notEquals(_ field: String, _ value: MetadataValue) -> Filter {
        .notEquals(field: field, value: value)
    }

    /// Creates an exists filter.
    public static func exists(_ field: String) -> Filter {
        .exists(field: field)
    }

    /// Creates an inclusive range filter. Pass `nil` for one-sided ranges.
    public static func range(
        _ field: String,
        lower: MetadataValue? = nil,
        upper: MetadataValue? = nil
    ) -> Filter {
        .range(field: field, lower: lower, upper: upper)
    }

    /// Creates an in-values filter.
    public static func inValues(_ field: String, _ values: [MetadataValue]) -> Filter {
        .inValues(field: field, values: values)
    }
}

/// Actor-isolated local retrieval index backed by the Rust core.
///
/// `VectorIndex` owns one Rust index handle. All mutation, search, and
/// persistence calls are actor-isolated, so callers use `await` outside the
/// actor and no manual locking is exposed in Swift.
public actor VectorIndex {
    private let handle: UInt

    private var pointer: OpaquePointer {
        OpaquePointer(bitPattern: handle)!
    }

    /// Required embedding dimension for indexed chunks and queries.
    public var dimension: Int {
        Int(vectorkit_index_dimension(pointer))
    }

    /// Number of chunks currently eligible for search results.
    public var activeChunkCount: Int {
        Int(vectorkit_index_active_chunk_count(pointer))
    }

    /// Total number of stored chunks, including deleted and superseded chunks.
    public var totalChunkCount: Int {
        Int(vectorkit_index_total_chunk_count(pointer))
    }

    /// Number of deleted or superseded chunks that compaction can remove.
    public var tombstonedChunkCount: Int {
        Int(vectorkit_index_tombstoned_chunk_count(pointer))
    }

    /// Creates an empty local index.
    public init(
        dimension: Int,
        metric: VectorMetric = .cosine,
        encoding: VectorEncoding = .f32
    ) throws {
        let pointer = try FFI.withStatusPointer { status in
            vectorkit_index_new(dimension, metric.ffiValue, encoding.ffiValue, status)
        }
        handle = UInt(bitPattern: pointer)
    }

    private init(pointer: OpaquePointer) {
        handle = UInt(bitPattern: pointer)
    }

    deinit {
        vectorkit_index_free(OpaquePointer(bitPattern: handle))
    }

    /// Loads an index saved by `save(to:includeBM25:)`.
    public static func load(from directory: URL) throws -> VectorIndex {
        let pointer = try FFI.withStatusPointer { status in
            directory.path.withCString { path in
                vectorkit_index_load(path, status)
            }
        }
        return VectorIndex(pointer: pointer)
    }

    /// Verifies a saved index without changing it or retaining it in memory.
    public static func validate(at directory: URL) throws {
        try FFI.withStatusBool { status in
            directory.path.withCString { path in
                vectorkit_index_validate(path, status)
            }
        }
    }

    /// Saves the loaded index to a local directory.
    public func save(to directory: URL, includeBM25: Bool = true) throws {
        let arena = CStringArena()
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_save(
            pointer,
            arena.copy(directory.path),
            includeBM25,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }
    }

    /// Adds or replaces all chunks for a document and returns assigned chunk IDs.
    @discardableResult
    public func upsert(document: Document, chunks: [ChunkInput]) throws -> [UInt64] {
        let arena = CStringArena()
        let documentMetadata = MetadataBuffer(document.metadata, arena: arena)
        let chunkMetadata = chunks.map { MetadataBuffer($0.metadata, arena: arena) }
        let embeddingBuffers = chunks.map { EmbeddingBuffer($0.embedding) }
        let ffiChunks = ChunkInputBuffer(chunks.enumerated().map { index, chunk in
            VkChunkInput(
                text: arena.copy(chunk.text),
                embedding: embeddingBuffers[index].pointer,
                embedding_len: embeddingBuffers[index].count,
                metadata: chunkMetadata[index].pointer,
                metadata_len: chunkMetadata[index].count
            )
        })
        var output = VkChunkIdBuffer(values: nil, count: 0)

        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_upsert_document(
            pointer,
            arena.copy(document.id),
            arena.copy(document.text),
            documentMetadata.pointer,
            documentMetadata.count,
            ffiChunks.pointer,
            ffiChunks.count,
            &output,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }

        defer { vectorkit_chunk_id_buffer_free(output) }
        guard let values = output.values else {
            return []
        }
        return Array(UnsafeBufferPointer(start: values, count: output.count))
    }

    /// Tombstones all active chunks for a document ID.
    @discardableResult
    public func deleteDocument(id: String) throws -> Int {
        let arena = CStringArena()
        var deletedCount = 0
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_delete_document(
            pointer,
            arena.copy(id),
            &deletedCount,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }
        return deletedCount
    }

    /// Rebuilds storage without deleted or superseded chunks.
    ///
    /// Surviving chunk IDs remain stable. Removed IDs are never reused.
    public func compact() throws -> CompactionReport {
        var output = VkCompactionReport(
            chunks_before: 0,
            chunks_after: 0,
            chunks_removed: 0,
            estimated_bytes_before: 0,
            estimated_bytes_after: 0,
            estimated_bytes_reclaimed: 0
        )
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_compact(pointer, &output, &status)
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }
        return CompactionReport(
            chunksBefore: output.chunks_before,
            chunksAfter: output.chunks_after,
            chunksRemoved: output.chunks_removed,
            estimatedBytesBefore: output.estimated_bytes_before,
            estimatedBytesAfter: output.estimated_bytes_after,
            estimatedBytesReclaimed: output.estimated_bytes_reclaimed
        )
    }

    /// Performs exact vector search over active chunks.
    public func search(
        embedding: [Float],
        topK: Int = 10,
        filter: Filter? = nil
    ) throws -> [SearchResult] {
        var output = VkSearchResultBuffer(hits: nil, count: 0)
        let embeddingBuffer = EmbeddingBuffer(embedding)
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_search(
            pointer,
            embeddingBuffer.pointer,
            embeddingBuffer.count,
            topK,
            ffiFilter?.pointer,
            &output,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }
        defer { vectorkit_search_results_free(output) }
        guard let hits = output.hits else {
            return []
        }
        return UnsafeBufferPointer(start: hits, count: output.count).map(SearchResult.init)
    }

    /// Performs BM25 keyword search over active chunks.
    public func keywordSearch(
        text: String,
        topK: Int = 10,
        filter: Filter? = nil
    ) throws -> [KeywordResult] {
        var output = VkKeywordResultBuffer(hits: nil, count: 0)
        let arena = CStringArena()
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_keyword_search(
            pointer,
            arena.copy(text),
            topK,
            ffiFilter?.pointer,
            &output,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
        }
        defer { vectorkit_keyword_results_free(output) }
        guard let hits = output.hits else {
            return []
        }
        return UnsafeBufferPointer(start: hits, count: output.count).map(KeywordResult.init)
    }

    /// Performs hybrid vector plus keyword search over active chunks.
    public func hybridSearch(
        text: String,
        embedding: [Float],
        topK: Int = 10,
        filter: Filter? = nil,
        options: HybridOptions = .default
    ) throws -> [HybridResult] {
        var output = VkHybridResultBuffer(hits: nil, count: 0)
        let arena = CStringArena()
        let embeddingBuffer = EmbeddingBuffer(embedding)
        let ffiOptions = options.ffiValue
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { vectorkit_status_clear(&status) }
        let succeeded = vectorkit_index_hybrid_search(
            pointer,
            arena.copy(text),
            embeddingBuffer.pointer,
            embeddingBuffer.count,
            topK,
            ffiFilter?.pointer,
            ffiOptions,
            &output,
            &status
        )
        guard succeeded else {
            throw VectorKitError.from(status: status)
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

private final class FFIFilter {
    let pointer: OpaquePointer

    init(pointer: OpaquePointer) {
        self.pointer = pointer
    }

    deinit {
        vectorkit_filter_free(pointer)
    }
}

private extension Filter {
    func makeFFI() throws -> FFIFilter {
        let pointer = try makeFFIPointer()
        return FFIFilter(pointer: pointer)
    }

    func makeFFIPointer() throws -> OpaquePointer {
        switch self {
        case .equals(let field, let value):
            try makeFFILeaf { status, arena in
                vectorkit_filter_equals(arena.copy(field), value.ffiValue(arena: arena), status)
            }
        case .notEquals(let field, let value):
            try makeFFILeaf { status, arena in
                vectorkit_filter_not_equals(arena.copy(field), value.ffiValue(arena: arena), status)
            }
        case .exists(let field):
            try makeFFILeaf { status, arena in
                vectorkit_filter_exists(arena.copy(field), status)
            }
        case .range(let field, let lower, let upper):
            try makeFFILeaf { status, arena in
                var lowerValue = lower?.ffiValue(arena: arena)
                var upperValue = upper?.ffiValue(arena: arena)
                return withOptionalPointer(to: &lowerValue) { lowerPointer in
                    withOptionalPointer(to: &upperValue) { upperPointer in
                        vectorkit_filter_range(
                            arena.copy(field),
                            lowerPointer,
                            upperPointer,
                            status
                        )
                    }
                }
            }
        case .inValues(let field, let values):
            try makeFFILeaf { status, arena in
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
        case .all(let filters):
            try makeFFIComposite(filters, builder: vectorkit_filter_all)
        case .any(let filters):
            try makeFFIComposite(filters, builder: vectorkit_filter_any)
        }
    }

    func makeFFILeaf(
        _ body: (UnsafeMutablePointer<VkStatus>, CStringArena) -> OpaquePointer?
    ) throws -> OpaquePointer {
        try FFI.withStatusPointer { status in
            let arena = CStringArena()
            return body(status, arena)
        }
    }

    func makeFFIComposite(
        _ filters: [Filter],
        builder: (
            UnsafePointer<OpaquePointer?>?,
            Int,
            UnsafeMutablePointer<VkStatus>?
        ) -> OpaquePointer?
    ) throws -> OpaquePointer {
        let children = try filters.map { try $0.makeFFI() }
        return try FFI.withStatusPointer { status in
            let pointers = children.map { Optional($0.pointer) }
            return pointers.withUnsafeBufferPointer { buffer in
                builder(buffer.baseAddress, buffer.count, status)
            }
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

private final class ChunkInputBuffer {
    let pointer: UnsafePointer<VkChunkInput>?
    let count: Int
    private let mutablePointer: UnsafeMutablePointer<VkChunkInput>?

    init(_ values: [VkChunkInput]) {
        count = values.count
        if values.isEmpty {
            mutablePointer = nil
            pointer = nil
        } else {
            let allocated = UnsafeMutablePointer<VkChunkInput>.allocate(capacity: values.count)
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
