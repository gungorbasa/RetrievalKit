import Foundation
import RetrievalKitFFI
import RetrievalKitShared

public typealias CorpusID = RetrievalKitShared.CorpusID
public typealias RecordID = RetrievalKitShared.RecordID
public typealias DocumentID = RetrievalKitShared.DocumentID
public typealias RecordType = RetrievalKitShared.RecordType
public typealias ChunkKey = RetrievalKitShared.ChunkKey
public typealias RecordValue = RetrievalKitShared.RecordValue
public typealias MetadataValue = RetrievalKitShared.MetadataValue
public typealias Record = RetrievalKitShared.Record
public typealias Document = RetrievalKitShared.Document
public typealias EmbeddedDocument = RetrievalKitShared.EmbeddedDocument
public typealias Chunk = RetrievalKitShared.Chunk
public typealias RecordInput = RetrievalKitShared.RecordInput

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

extension RetrievalKitShared.MetadataValue {
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
  /// Internal chunk identifier assigned by RetrievalKit.
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
  /// Internal chunk identifier assigned by RetrievalKit.
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
  /// Internal chunk identifier assigned by RetrievalKit.
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

/// Candidate options for hybrid search.
public struct HybridOptions: Equatable, Sendable {
  /// Number of vector candidates generated before fusion.
  public var vectorTopK: Int
  /// Number of keyword candidates generated before fusion.
  public var keywordTopK: Int
  /// Experiment-backed V1 defaults: 50 vector and 50 keyword candidates.
  public static let `default` = HybridOptions()

  /// Creates hybrid search options.
  public init(
    vectorTopK: Int = 50,
    keywordTopK: Int = 50
  ) {
    self.vectorTopK = vectorTopK
    self.keywordTopK = keywordTopK
  }

  fileprivate func ffiValue(alpha: Float) -> VkHybridQueryOptions {
    VkHybridQueryOptions(
      vector_top_k: vectorTopK,
      keyword_top_k: keywordTopK,
      alpha: alpha
    )
  }
}

/// Error surfaced by the Swift wrapper.
public enum RetrievalKitError: Error, Equatable, CustomStringConvertible, Sendable {
  /// Invalid input before or at the FFI boundary.
  case invalidArgument(String)
  /// Typed error returned by the Rust core.
  case core(String)
  /// Panic caught at the FFI boundary.
  case panic(String)
  /// A persisted index failed an integrity check.
  case corruptIndex(String)
  /// An embedding did not match the configured model dimension.
  case invalidDimension(String)
  /// An external record or chunk identity was invalid.
  case invalidIdentity(String)
  /// A retrieval-capable upsert omitted an embedding.
  case missingEmbedding(String)
  /// The requested query requires hybrid retrieval.
  case retrievalCapabilityUnavailable(String)
  /// Unknown FFI status code.
  case unknown(code: Int32, message: String)

  /// Human-readable error message.
  public var description: String {
    switch self {
    case .invalidArgument(let message), .core(let message), .panic(let message),
      .corruptIndex(let message), .invalidDimension(let message),
      .invalidIdentity(let message), .missingEmbedding(let message),
      .retrievalCapabilityUnavailable(let message):
      message
    case .unknown(let code, let message):
      "RetrievalKit error \(code): \(message)"
    }
  }

  fileprivate static func from(status: VkStatus) -> RetrievalKitError {
    let message = status.message.map { String(cString: $0) } ?? "unknown RetrievalKit FFI error"
    switch status.code {
    case 1: return .invalidArgument(message)
    case 2: return .core(message)
    case 3: return .panic(message)
    case 4: return .corruptIndex(message)
    case 5: return .invalidDimension(message)
    case 6: return .retrievalCapabilityUnavailable(message)
    case 7: return .invalidIdentity(message)
    case 8: return .missingEmbedding(message)
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

/// Writer-preferring asynchronous gate used to protect one native index handle.
///
/// Read operations may overlap. A write waits for existing readers, prevents
/// new readers from entering, and runs alone.
actor AsyncReadWriteGate {
  private var activeReaders = 0
  private var writerActive = false
  private var waitingReaders: [CheckedContinuation<Void, Never>] = []
  private var waitingWriters: [CheckedContinuation<Void, Never>] = []

  func withRead<T: Sendable>(
    _ operation: @Sendable () async throws -> T
  ) async rethrows -> T {
    await beginRead()
    do {
      let result = try await operation()
      endRead()
      return result
    } catch {
      endRead()
      throw error
    }
  }

  func withWrite<T: Sendable>(
    _ operation: @Sendable () async throws -> T
  ) async rethrows -> T {
    await beginWrite()
    do {
      let result = try await operation()
      endWrite()
      return result
    } catch {
      endWrite()
      throw error
    }
  }

  private func beginRead() async {
    guard writerActive || !waitingWriters.isEmpty else {
      activeReaders += 1
      return
    }
    await withCheckedContinuation { continuation in
      waitingReaders.append(continuation)
    }
  }

  private func endRead() {
    activeReaders -= 1
    guard activeReaders == 0, !waitingWriters.isEmpty else { return }
    writerActive = true
    waitingWriters.removeFirst().resume()
  }

  private func beginWrite() async {
    guard writerActive || activeReaders > 0 else {
      writerActive = true
      return
    }
    await withCheckedContinuation { continuation in
      waitingWriters.append(continuation)
    }
  }

  private func endWrite() {
    if !waitingWriters.isEmpty {
      waitingWriters.removeFirst().resume()
      return
    }

    writerActive = false
    let readers = waitingReaders
    waitingReaders.removeAll(keepingCapacity: true)
    activeReaders += readers.count
    for reader in readers {
      reader.resume()
    }
  }
}

/// Concurrent local retrieval index backed by the Rust core.
///
/// `VectorIndex` owns one Rust index handle. Its actor protects lifecycle and
/// admission state; native work runs outside the actor under a shared-read or
/// exclusive-write lease. Callers use `await` and never manage locks directly.
public actor VectorIndex {
  private let handle: UInt
  private let access = AsyncReadWriteGate()

  /// Required embedding dimension for indexed chunks and queries.
  public var dimension: Int {
    get async {
      let handle = handle
      return await access.withRead {
        Int(retrievalkit_index_dimension(OpaquePointer(bitPattern: handle)))
      }
    }
  }

  /// Number of chunks currently eligible for search results.
  public var activeChunkCount: Int {
    get async {
      let handle = handle
      return await access.withRead {
        Int(retrievalkit_index_active_chunk_count(OpaquePointer(bitPattern: handle)))
      }
    }
  }

  /// Total number of stored chunks, including deleted and superseded chunks.
  public var totalChunkCount: Int {
    get async {
      let handle = handle
      return await access.withRead {
        Int(retrievalkit_index_total_chunk_count(OpaquePointer(bitPattern: handle)))
      }
    }
  }

  /// Number of deleted or superseded chunks that compaction can remove.
  public var tombstonedChunkCount: Int {
    get async {
      let handle = handle
      return await access.withRead {
        Int(retrievalkit_index_tombstoned_chunk_count(OpaquePointer(bitPattern: handle)))
      }
    }
  }

  /// Creates an empty local index.
  public init(
    dimension: Int,
    metric: VectorMetric = .cosine,
    encoding: VectorEncoding = .i8ScalarQuantized
  ) throws {
    let pointer = try FFI.withStatusPointer { status in
      retrievalkit_index_new(dimension, metric.ffiValue, encoding.ffiValue, status)
    }
    handle = UInt(bitPattern: pointer)
  }

  private init(pointer: OpaquePointer) {
    handle = UInt(bitPattern: pointer)
  }

  deinit {
    retrievalkit_index_free(OpaquePointer(bitPattern: handle))
  }

  /// Loads an index saved by `save(to:includeBM25:)`.
  public static func load(from directory: URL) throws -> VectorIndex {
    let pointer = try FFI.withStatusPointer { status in
      directory.path.withCString { path in
        retrievalkit_index_load(path, status)
      }
    }
    return VectorIndex(pointer: pointer)
  }

  /// Verifies a saved index without changing it or retaining it in memory.
  public static func validate(at directory: URL) throws {
    try FFI.withStatusBool { status in
      directory.path.withCString { path in
        retrievalkit_index_validate(path, status)
      }
    }
  }

  /// Saves the loaded index to a local directory.
  public func save(to directory: URL, includeBM25: Bool = true) async throws {
    let handle = handle
    let owner = self
    try await access.withWrite {
      try Task.checkCancellation()
      try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        let arena = CStringArena()
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = retrievalkit_index_save(
          OpaquePointer(bitPattern: handle),
          arena.copy(directory.path),
          includeBM25,
          &status
        )
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
      }.value
    }
  }

  /// Adds or replaces all chunks for a document and returns assigned chunk IDs.
  @discardableResult
  public func upsert(document: Document, chunks: [ChunkInput]) async throws -> [UInt64] {
    let handle = handle
    let owner = self
    return try await access.withWrite {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        let arena = CStringArena()
        let documentMetadata = MetadataBuffer(document.metadata, arena: arena)
        let chunkMetadata = chunks.map { MetadataBuffer($0.metadata, arena: arena) }
        let embeddingBuffers = chunks.map { EmbeddingBuffer($0.embedding) }
        let ffiChunks = ChunkInputBuffer(
          chunks.enumerated().map { index, chunk in
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
        defer { retrievalkit_status_clear(&status) }
        let succeeded = retrievalkit_index_upsert_document(
          OpaquePointer(bitPattern: handle),
          arena.copy(document.id.rawValue),
          arena.copy(document.text),
          documentMetadata.pointer,
          documentMetadata.count,
          ffiChunks.pointer,
          ffiChunks.count,
          &output,
          &status
        )
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }

        defer { retrievalkit_chunk_id_buffer_free(output) }
        guard let values = output.values else {
          return []
        }
        return Array(UnsafeBufferPointer(start: values, count: output.count))
      }.value
    }
  }

  /// Tombstones all active chunks for a document ID.
  @discardableResult
  public func deleteDocument(id: String) async throws -> Int {
    let handle = handle
    let owner = self
    return try await access.withWrite {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        let arena = CStringArena()
        var deletedCount = 0
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = retrievalkit_index_delete_document(
          OpaquePointer(bitPattern: handle),
          arena.copy(id),
          &deletedCount,
          &status
        )
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
        return deletedCount
      }.value
    }
  }

  /// Rebuilds storage without deleted or superseded chunks.
  ///
  /// Surviving chunk IDs remain stable. Removed IDs are never reused.
  public func compact() async throws -> CompactionReport {
    let handle = handle
    let owner = self
    return try await access.withWrite {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        var output = VkCompactionReport(
          chunks_before: 0,
          chunks_after: 0,
          chunks_removed: 0,
          estimated_bytes_before: 0,
          estimated_bytes_after: 0,
          estimated_bytes_reclaimed: 0
        )
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = retrievalkit_index_compact(
          OpaquePointer(bitPattern: handle),
          &output,
          &status
        )
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
        return CompactionReport(
          chunksBefore: output.chunks_before,
          chunksAfter: output.chunks_after,
          chunksRemoved: output.chunks_removed,
          estimatedBytesBefore: output.estimated_bytes_before,
          estimatedBytesAfter: output.estimated_bytes_after,
          estimatedBytesReclaimed: output.estimated_bytes_reclaimed
        )
      }.value
    }
  }

  /// Performs exact vector search over active chunks.
  public func search(
    embedding: [Float],
    topK: Int = 10,
    filter: Filter? = nil
  ) async throws -> [SearchResult] {
    let handle = handle
    let owner = self
    return try await access.withRead {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        var output = emptySearchResultBuffer()
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = embedding.withUnsafeBufferPointer { vector in
          retrievalkit_index_search(
            OpaquePointer(bitPattern: handle),
            vector.baseAddress,
            vector.count,
            topK,
            ffiFilter?.pointer,
            &output,
            &status
          )
        }
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
        defer { retrievalkit_search_results_free(output) }
        guard let hits = output.hits else {
          return []
        }
        let decoder = PackedResultDecoder(output)
        return try UnsafeBufferPointer(start: hits, count: output.count).map {
          try SearchResult($0, decoder: decoder)
        }
      }.value
    }
  }

  /// Performs BM25 keyword search over active chunks.
  public func keywordSearch(
    text: String,
    topK: Int = 10,
    filter: Filter? = nil
  ) async throws -> [KeywordResult] {
    let handle = handle
    let owner = self
    return try await access.withRead {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        var output = emptyKeywordResultBuffer()
        let arena = CStringArena()
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = retrievalkit_index_keyword_search(
          OpaquePointer(bitPattern: handle),
          arena.copy(text),
          topK,
          ffiFilter?.pointer,
          &output,
          &status
        )
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
        defer { retrievalkit_keyword_results_free(output) }
        guard let hits = output.hits else {
          return []
        }
        let decoder = PackedResultDecoder(output)
        return try UnsafeBufferPointer(start: hits, count: output.count).map {
          try KeywordResult($0, decoder: decoder)
        }
      }.value
    }
  }

  /// Performs hybrid vector plus keyword search over active chunks.
  public func hybridSearch(
    text: String,
    embedding: [Float],
    topK: Int = 10,
    filter: Filter? = nil,
    alpha: Float = 0.6,
    options: HybridOptions = .default
  ) async throws -> [HybridResult] {
    let handle = handle
    let owner = self
    return try await access.withRead {
      try Task.checkCancellation()
      return try await Task.detached(priority: Task.currentPriority) {
        defer { withExtendedLifetime(owner) {} }
        var output = emptyHybridResultBuffer()
        let arena = CStringArena()
        let ffiOptions = options.ffiValue(alpha: alpha)
        let ffiFilter = try filter?.makeFFI()
        var status = VkStatus(code: 0, message: nil)
        defer { retrievalkit_status_clear(&status) }
        let succeeded = embedding.withUnsafeBufferPointer { vector in
          retrievalkit_index_hybrid_search_alpha(
            OpaquePointer(bitPattern: handle),
            arena.copy(text),
            vector.baseAddress,
            vector.count,
            topK,
            ffiFilter?.pointer,
            ffiOptions,
            &output,
            &status
          )
        }
        guard succeeded else {
          throw RetrievalKitError.from(status: status)
        }
        defer { retrievalkit_hybrid_results_free(output) }
        guard let hits = output.hits else {
          return []
        }
        let decoder = PackedResultDecoder(output)
        return try UnsafeBufferPointer(start: hits, count: output.count).map {
          try HybridResult($0, decoder: decoder)
        }
      }.value
    }
  }
}

public struct VectorIndexConfiguration: Equatable, Sendable {
  public var dimension: Int
  public var metric: VectorMetric
  public var encoding: VectorEncoding
  public init(
    dimension: Int, metric: VectorMetric = .cosine, encoding: VectorEncoding = .i8ScalarQuantized
  ) {
    self.dimension = dimension
    self.metric = metric
    self.encoding = encoding
  }
}

public struct RetrievalConfiguration: Equatable, Sendable {
  public var semantic: VectorIndexConfiguration

  public init(semantic: VectorIndexConfiguration) {
    self.semantic = semantic
  }
}

private final class NativeRetrievalOwner: @unchecked Sendable {
  private let lock = NSLock()
  private var handle: UInt?
  init(_ handle: UInt) { self.handle = handle }
  deinit { close() }
  func requireHandle() throws -> UInt {
    lock.lock()
    defer { lock.unlock() }
    guard let handle else { throw RetrievalKitError.core("retrieval database is closed") }
    return handle
  }
  func close() {
    lock.lock()
    let owned = handle
    handle = nil
    lock.unlock()
    if let owned { retrievalkit_retrieval_database_free(OpaquePointer(bitPattern: owned)) }
  }
}

public actor RetrievalQueries {
  private let owner: NativeRetrievalOwner
  fileprivate init(owner: NativeRetrievalOwner) { self.owner = owner }

  public func semanticSearch(embedding: [Float], topK: Int = 10, filter: Filter? = nil) async throws
    -> [SearchResult]
  {
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      var output = emptySearchResultBuffer()
      let ffiFilter = try filter?.makeFFI()
      var status = VkStatus(code: 0, message: nil)
      defer { retrievalkit_status_clear(&status) }
      let handle = try owner.requireHandle()
      guard embedding.withUnsafeBufferPointer({ vector in
        retrievalkit_retrieval_semantic_search(
          OpaquePointer(bitPattern: handle), vector.baseAddress, vector.count, topK,
          ffiFilter?.pointer, &output, &status)
      })
      else { throw RetrievalKitError.from(status: status) }
      defer { retrievalkit_search_results_free(output) }
      guard let hits = output.hits else { return [] }
      let decoder = PackedResultDecoder(output)
      return try UnsafeBufferPointer(start: hits, count: output.count).map {
        try SearchResult($0, decoder: decoder)
      }
    }.value
  }

  public func keywordSearch(
    text: String,
    topK: Int = 10,
    filter: Filter? = nil
  ) async throws -> [KeywordResult] {
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      var output = emptyKeywordResultBuffer()
      let arena = CStringArena()
      let ffiFilter = try filter?.makeFFI()
      var status = VkStatus(code: 0, message: nil)
      defer { retrievalkit_status_clear(&status) }
      guard
        retrievalkit_retrieval_keyword_search(
          OpaquePointer(bitPattern: try owner.requireHandle()),
          arena.copy(text),
          topK,
          ffiFilter?.pointer,
          &output,
          &status
        )
      else { throw RetrievalKitError.from(status: status) }
      defer { retrievalkit_keyword_results_free(output) }
      guard let hits = output.hits else { return [] }
      let decoder = PackedResultDecoder(output)
      return try UnsafeBufferPointer(start: hits, count: output.count).map {
        try KeywordResult($0, decoder: decoder)
      }
    }.value
  }

  public func hybridSearch(
    text: String, embedding: [Float], topK: Int = 10, filter: Filter? = nil,
    alpha: Float = 0.6,
    options: HybridOptions = .default
  ) async throws -> [HybridResult] {
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      var output = emptyHybridResultBuffer()
      let arena = CStringArena()
      let ffiFilter = try filter?.makeFFI()
      var status = VkStatus(code: 0, message: nil)
      defer { retrievalkit_status_clear(&status) }
      let handle = try owner.requireHandle()
      guard embedding.withUnsafeBufferPointer({ vector in
        retrievalkit_retrieval_hybrid_search_alpha(
          OpaquePointer(bitPattern: handle), arena.copy(text), vector.baseAddress,
          vector.count, topK, ffiFilter?.pointer, options.ffiValue(alpha: alpha), &output, &status)
      })
      else { throw RetrievalKitError.from(status: status) }
      defer { retrievalkit_hybrid_results_free(output) }
      guard let hits = output.hits else { return [] }
      let decoder = PackedResultDecoder(output)
      return try UnsafeBufferPointer(start: hits, count: output.count).map {
        try HybridResult($0, decoder: decoder)
      }
    }.value
  }
}

public actor RetrievalDatabase {
  public actor Builder {
    private var handle: UInt?

    /// Creates a retrieval builder whose embedding dimension is inferred from
    /// the first document upsert.
    public init(
      corpusID: CorpusID = "default",
      metric: VectorMetric = .cosine,
      encoding: VectorEncoding = .i8ScalarQuantized
    ) throws {
      self.handle = UInt(
        bitPattern: try FFI.withStatusPointer { status in
          corpusID.rawValue.withCString { corpus in
            retrievalkit_retrieval_builder_new(
              metric.ffiValue, encoding.ffiValue, corpus, status)
          }
        })
    }
    deinit { if let handle { retrievalkit_retrieval_builder_free(OpaquePointer(bitPattern: handle)) } }

    /// Adds or replaces one searchable document using a caller-produced
    /// embedding. The first embedding fixes the database dimension.
    public func upsert(_ document: Document, embedding: [Float]) throws {
      guard let handle else {
        throw RetrievalKitError.core("retrieval builder was already consumed")
      }
      let arena = CStringArena()
      let metadata = MetadataBuffer(document.metadata, arena: arena)
      try FFI.withStatusBool { status in
        embedding.withUnsafeBufferPointer { vector in
          retrievalkit_retrieval_builder_upsert_document(
            OpaquePointer(bitPattern: handle),
            arena.copy(document.id.rawValue),
            arena.copy(document.text),
            metadata.pointer,
            metadata.count,
            vector.baseAddress,
            vector.count,
            status
          )
        }
      }
    }

    public func build() throws -> RetrievalDatabase {
      guard let owned = handle else {
        throw RetrievalKitError.core("retrieval builder was already consumed")
      }
      handle = nil
      let pointer = try FFI.withStatusPointer {
        retrievalkit_retrieval_builder_build(OpaquePointer(bitPattern: owned), $0)
      }
      return RetrievalDatabase(pointer: pointer)
    }
  }

  private let owner: NativeRetrievalOwner
  public nonisolated let retrieval: RetrievalQueries
  private init(pointer: OpaquePointer) {
    let owner = NativeRetrievalOwner(UInt(bitPattern: pointer))
    self.owner = owner
    retrieval = RetrievalQueries(owner: owner)
  }
  public func close() { owner.close() }

  /// Performs exact vector search.
  public func search(
    embedding: [Float],
    limit: Int = 10,
    filter: Filter? = nil
  ) async throws -> [SearchResult] {
    try await retrieval.semanticSearch(embedding: embedding, topK: limit, filter: filter)
  }

  /// Performs BM25 text search without requiring a query embedding.
  public func search(
    text: String,
    limit: Int = 10,
    filter: Filter? = nil
  ) async throws -> [KeywordResult] {
    try await retrieval.keywordSearch(text: text, topK: limit, filter: filter)
  }

  /// Performs weighted vector + BM25 search. `alpha` is the vector weight:
  /// `1` is vector-only, `0` is BM25-only, and intermediate values are hybrid.
  public func search(
    text: String,
    embedding: [Float],
    alpha: Float = 0.6,
    limit: Int = 10,
    filter: Filter? = nil
  ) async throws -> [HybridResult] {
    return try await retrieval.hybridSearch(
      text: text,
      embedding: embedding,
      topK: limit,
      filter: filter,
      alpha: alpha
    )
  }

  public func save(to directory: URL) async throws {
    let owner = owner
    try await Task.detached(priority: Task.currentPriority) {
      let handle = try owner.requireHandle()
      var status = VkStatus(code: 0, message: nil)
      defer { retrievalkit_status_clear(&status) }
      let succeeded = directory.path.withCString {
        retrievalkit_retrieval_database_save(OpaquePointer(bitPattern: handle), $0, &status)
      }
      guard succeeded else { throw RetrievalKitError.from(status: status) }
    }.value
  }
  public static func load(from directory: URL) throws -> RetrievalDatabase {
    RetrievalDatabase(
      pointer: try FFI.withStatusPointer { status in
        directory.path.withCString { retrievalkit_retrieval_database_load($0, status) }
      })
  }
  public static func validate(at directory: URL) throws {
    try FFI.withStatusBool { status in
      directory.path.withCString { retrievalkit_retrieval_database_validate($0, status) }
    }
  }
}

extension SearchResult {
  fileprivate init(_ hit: VkSearchHit, decoder: PackedResultDecoder) throws {
    self.init(
      chunkID: hit.chunk_id,
      documentID: try decoder.string(hit.document_id),
      text: try decoder.string(hit.text),
      score: hit.score,
      trace: SearchTrace(
        vectorScore: hit.vector_score,
        filterMatched: hit.filter_matched
      )
    )
  }
}

extension KeywordResult {
  fileprivate init(_ hit: VkKeywordHit, decoder: PackedResultDecoder) throws {
    self.init(
      chunkID: hit.chunk_id,
      documentID: try decoder.string(hit.document_id),
      text: try decoder.string(hit.text),
      score: hit.score,
      matchedTerms: try decoder.strings(
        start: hit.matched_terms_start,
        count: hit.matched_terms_count
      )
    )
  }
}

extension HybridResult {
  fileprivate init(_ hit: VkHybridHit, decoder: PackedResultDecoder) throws {
    self.init(
      chunkID: hit.chunk_id,
      documentID: try decoder.string(hit.document_id),
      text: try decoder.string(hit.text),
      score: hit.score,
      vectorScore: hit.has_vector_score ? hit.vector_score : nil,
      keywordScore: hit.has_keyword_score ? hit.keyword_score : nil,
      trace: HybridTrace(
        vectorRank: hit.has_vector_rank ? Int(hit.vector_rank) : nil,
        keywordRank: hit.has_keyword_rank ? Int(hit.keyword_rank) : nil,
        normalizedVectorScore: hit.has_normalized_vector_score ? hit.normalized_vector_score : nil,
        normalizedKeywordScore: hit.has_normalized_keyword_score
          ? hit.normalized_keyword_score : nil,
        matchedTerms: try decoder.strings(
          start: hit.matched_terms_start,
          count: hit.matched_terms_count
        ),
        filterMatched: hit.filter_matched
      )
    )
  }
}

private struct PackedResultDecoder {
  private let utf8: UnsafePointer<UInt8>?
  private let utf8Count: Int
  private let matchedTerms: UnsafePointer<VkUtf8Range>?
  private let matchedTermsCount: Int

  init(_ output: VkSearchResultBuffer) {
    utf8 = output.utf8
    utf8Count = output.utf8_len
    matchedTerms = nil
    matchedTermsCount = 0
  }

  init(_ output: VkKeywordResultBuffer) {
    utf8 = output.utf8
    utf8Count = output.utf8_len
    matchedTerms = output.matched_terms
    matchedTermsCount = output.matched_terms_count
  }

  init(_ output: VkHybridResultBuffer) {
    utf8 = output.utf8
    utf8Count = output.utf8_len
    matchedTerms = output.matched_terms
    matchedTermsCount = output.matched_terms_count
  }

  func string(_ range: VkUtf8Range) throws -> String {
    guard
      utf8Count >= 0,
      range.offset >= 0,
      range.length >= 0,
      range.offset <= utf8Count,
      range.length <= utf8Count - range.offset
    else {
      throw RetrievalKitError.core("native result contains an invalid UTF-8 range")
    }
    guard range.length > 0 else { return "" }
    guard let utf8 else {
      throw RetrievalKitError.core("native result UTF-8 arena is missing")
    }
    let bytes = UnsafeBufferPointer(start: utf8.advanced(by: range.offset), count: range.length)
    guard let value = String(bytes: bytes, encoding: .utf8) else {
      throw RetrievalKitError.core("native result contains invalid UTF-8")
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
      throw RetrievalKitError.core("native result contains an invalid matched-term range")
    }
    guard count > 0 else { return [] }
    guard let matchedTerms else {
      throw RetrievalKitError.core("native result matched-term ranges are missing")
    }
    return try UnsafeBufferPointer(start: matchedTerms.advanced(by: start), count: count).map {
      try string($0)
    }
  }
}

private func emptySearchResultBuffer() -> VkSearchResultBuffer {
  VkSearchResultBuffer(hits: nil, count: 0, utf8: nil, utf8_len: 0)
}

private func emptyKeywordResultBuffer() -> VkKeywordResultBuffer {
  VkKeywordResultBuffer(
    hits: nil,
    count: 0,
    utf8: nil,
    utf8_len: 0,
    matched_terms: nil,
    matched_terms_count: 0
  )
}

private func emptyHybridResultBuffer() -> VkHybridResultBuffer {
  VkHybridResultBuffer(
    hits: nil,
    count: 0,
    utf8: nil,
    utf8_len: 0,
    matched_terms: nil,
    matched_terms_count: 0
  )
}

private enum FFI {
  static func withStatusPointer(
    _ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?
  ) throws -> OpaquePointer {
    var status = VkStatus(code: 0, message: nil)
    defer { retrievalkit_status_clear(&status) }
    guard let pointer = body(&status) else {
      throw RetrievalKitError.from(status: status)
    }
    return pointer
  }

  static func withStatusBool(
    _ body: (UnsafeMutablePointer<VkStatus>) -> Bool
  ) throws {
    var status = VkStatus(code: 0, message: nil)
    defer { retrievalkit_status_clear(&status) }
    guard body(&status) else {
      throw RetrievalKitError.from(status: status)
    }
  }
}

private final class FFIFilter {
  let pointer: OpaquePointer

  init(pointer: OpaquePointer) {
    self.pointer = pointer
  }

  deinit {
    retrievalkit_filter_free(pointer)
  }
}

extension Filter {
  fileprivate func makeFFI() throws -> FFIFilter {
    let pointer = try makeFFIPointer()
    return FFIFilter(pointer: pointer)
  }

  fileprivate func makeFFIPointer() throws -> OpaquePointer {
    switch self {
    case .equals(let field, let value):
      try makeFFILeaf { status, arena in
        retrievalkit_filter_equals(arena.copy(field), value.ffiValue(arena: arena), status)
      }
    case .notEquals(let field, let value):
      try makeFFILeaf { status, arena in
        retrievalkit_filter_not_equals(arena.copy(field), value.ffiValue(arena: arena), status)
      }
    case .exists(let field):
      try makeFFILeaf { status, arena in
        retrievalkit_filter_exists(arena.copy(field), status)
      }
    case .range(let field, let lower, let upper):
      try makeFFILeaf { status, arena in
        var lowerValue = lower?.ffiValue(arena: arena)
        var upperValue = upper?.ffiValue(arena: arena)
        return withOptionalPointer(to: &lowerValue) { lowerPointer in
          withOptionalPointer(to: &upperValue) { upperPointer in
            retrievalkit_filter_range(
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
          retrievalkit_filter_in_values(
            arena.copy(field),
            buffer.baseAddress,
            buffer.count,
            status
          )
        }
      }
    case .all(let filters):
      try makeFFIComposite(filters, builder: retrievalkit_filter_all)
    case .any(let filters):
      try makeFFIComposite(filters, builder: retrievalkit_filter_any)
    }
  }

  fileprivate func makeFFILeaf(
    _ body: (UnsafeMutablePointer<VkStatus>, CStringArena) -> OpaquePointer?
  ) throws -> OpaquePointer {
    try FFI.withStatusPointer { status in
      let arena = CStringArena()
      return body(status, arena)
    }
  }

  fileprivate func makeFFIComposite(
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
      let entries =
        metadata
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
