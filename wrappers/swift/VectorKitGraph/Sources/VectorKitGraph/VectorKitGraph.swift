import Foundation
import VectorKitGraphFFI
import VectorKitShared

public typealias CorpusID = VectorKitShared.CorpusID
public typealias RecordID = VectorKitShared.RecordID
public typealias RecordType = VectorKitShared.RecordType
public typealias ChunkKey = VectorKitShared.ChunkKey
public typealias RecordValue = VectorKitShared.RecordValue
public typealias MetadataValue = VectorKitShared.MetadataValue
public typealias Record = VectorKitShared.Record
public typealias Chunk = VectorKitShared.Chunk
public typealias RecordInput = VectorKitShared.RecordInput

public enum GraphMetric: Sendable {
  case cosine, dotProduct
  var ffi: UInt32 { self == .cosine ? 0 : 1 }
}

public enum GraphVectorEncoding: Sendable {
  case f32, f16, i8ScalarQuantized
  var ffi: UInt32 {
    switch self {
    case .f32: 0
    case .f16: 1
    case .i8ScalarQuantized: 3
    }
  }
}

public enum GraphValue: Equatable, Sendable, Codable {
  case null
  case bool(Bool)
  case int(Int64)
  case double(Double)
  case string(String)
  case list([GraphValue])
  case map([String: GraphValue])

  public func encode(to encoder: Encoder) throws {
    switch self {
    case .null:
      var c = encoder.singleValueContainer()
      try c.encode("Null")
    case .bool(let v): try tagged("Bool", v, encoder)
    case .int(let v): try tagged("I64", v, encoder)
    case .double(let v): try tagged("F64", v, encoder)
    case .string(let v): try tagged("String", v, encoder)
    case .list(let v): try tagged("List", v, encoder)
    case .map(let v): try tagged("Map", v, encoder)
    }
  }
  public init(from decoder: Decoder) throws {
    if let single = try? decoder.singleValueContainer(), let tag = try? single.decode(String.self),
      tag == "Null"
    {
      self = .null
      return
    }
    let container = try decoder.container(keyedBy: DynamicKey.self)
    if let key = DynamicKey(stringValue: "Bool"), container.contains(key) {
      self = .bool(try container.decode(Bool.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "I64"), container.contains(key) {
      self = .int(try container.decode(Int64.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "F64"), container.contains(key) {
      self = .double(try container.decode(Double.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "String"), container.contains(key) {
      self = .string(try container.decode(String.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "List"), container.contains(key) {
      self = .list(try container.decode([GraphValue].self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "Map"), container.contains(key) {
      self = .map(try container.decode([String: GraphValue].self, forKey: key))
      return
    }
    throw DecodingError.dataCorrupted(
      .init(codingPath: decoder.codingPath, debugDescription: "unsupported graph value tag"))
  }
}

public enum GraphMetadataValue: Equatable, Sendable, Codable {
  case string(String)
  case integer(Int64)
  case double(Double)
  case boolean(Bool)
  case timestampMillis(Int64)
  public func encode(to encoder: Encoder) throws {
    switch self {
    case .string(let v): try tagged("String", v, encoder)
    case .integer(let v): try tagged("Integer", v, encoder)
    case .double(let v): try tagged("Float", v, encoder)
    case .boolean(let v): try tagged("Boolean", v, encoder)
    case .timestampMillis(let v): try tagged("TimestampMillis", v, encoder)
    }
  }
  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: DynamicKey.self)
    if let key = DynamicKey(stringValue: "String"), container.contains(key) {
      self = .string(try container.decode(String.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "Integer"), container.contains(key) {
      self = .integer(try container.decode(Int64.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "Float"), container.contains(key) {
      self = .double(try container.decode(Double.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "Boolean"), container.contains(key) {
      self = .boolean(try container.decode(Bool.self, forKey: key))
      return
    }
    if let key = DynamicKey(stringValue: "TimestampMillis"), container.contains(key) {
      self = .timestampMillis(try container.decode(Int64.self, forKey: key))
      return
    }
    throw DecodingError.dataCorrupted(
      .init(codingPath: decoder.codingPath, debugDescription: "unsupported graph metadata tag"))
  }
  fileprivate func ffiValue(arena: GraphCStringArena) -> VkMetadataValue {
    switch self {
    case .string(let value):
      VkMetadataValue(
        value_type: 0, string_value: arena.copy(value), integer_value: 0, float_value: 0,
        bool_value: false)
    case .integer(let value):
      VkMetadataValue(
        value_type: 1, string_value: nil, integer_value: value, float_value: 0, bool_value: false)
    case .double(let value):
      VkMetadataValue(
        value_type: 2, string_value: nil, integer_value: 0, float_value: value, bool_value: false)
    case .boolean(let value):
      VkMetadataValue(
        value_type: 3, string_value: nil, integer_value: 0, float_value: 0, bool_value: value)
    case .timestampMillis(let value):
      VkMetadataValue(
        value_type: 4, string_value: nil, integer_value: value, float_value: 0, bool_value: false)
    }
  }
}

public indirect enum GraphFilter: Equatable, Sendable {
  case equals(field: String, value: GraphMetadataValue)
  case notEquals(field: String, value: GraphMetadataValue)
  case exists(field: String)
  case range(field: String, lower: GraphMetadataValue?, upper: GraphMetadataValue?)
  case inValues(field: String, values: [GraphMetadataValue])
  case all([GraphFilter])
  case any([GraphFilter])

  public static func equals(_ field: String, _ value: GraphMetadataValue) -> Self {
    .equals(field: field, value: value)
  }
  public static func notEquals(_ field: String, _ value: GraphMetadataValue) -> Self {
    .notEquals(field: field, value: value)
  }
  public static func exists(_ field: String) -> Self { .exists(field: field) }
  public static func range(
    _ field: String, lower: GraphMetadataValue? = nil, upper: GraphMetadataValue? = nil
  ) -> Self { .range(field: field, lower: lower, upper: upper) }
  public static func inValues(_ field: String, _ values: [GraphMetadataValue]) -> Self {
    .inValues(field: field, values: values)
  }
}

public struct GraphHybridOptions: Equatable, Sendable {
  public var vectorTopK, keywordTopK: Int
  public static let `default` = GraphHybridOptions()
  public init(vectorTopK: Int = 50, keywordTopK: Int = 50) {
    self.vectorTopK = vectorTopK
    self.keywordTopK = keywordTopK
  }
  fileprivate func ffiValue(alpha: Float) -> VkHybridOptions {
    VkHybridOptions(
      vector_top_k: vectorTopK, keyword_top_k: keywordTopK, fusion_type: 0,
      vector_weight: alpha, keyword_weight: 1 - alpha, rrf_k: 0)
  }
}

private struct DynamicKey: CodingKey {
  let stringValue: String
  let intValue: Int? = nil
  init?(stringValue: String) { self.stringValue = stringValue }
  init?(intValue: Int) { return nil }
}

private func tagged<T: Encodable>(_ key: String, _ value: T, _ encoder: Encoder) throws {
  var container = encoder.container(keyedBy: DynamicKey.self)
  try container.encode(value, forKey: DynamicKey(stringValue: key)!)
}

public struct GraphRecord: Equatable, Sendable, Codable {
  public var id: String
  public var recordType: String
  public var fields: [String: GraphValue]
  public var content: String?
  public init(
    id: String, recordType: String, fields: [String: GraphValue] = [:], content: String? = nil
  ) {
    self.id = id
    self.recordType = recordType
    self.fields = fields
    self.content = content
  }
  enum CodingKeys: String, CodingKey {
    case id
    case recordType = "record_type"
    case fields, content
  }
}

public struct GraphChunk: Equatable, Sendable, Codable {
  public var key: String
  public var text: String
  public var embedding: [Float]
  public var metadata: [String: GraphMetadataValue]
  public init(
    key: String, text: String, embedding: [Float], metadata: [String: GraphMetadataValue] = [:]
  ) {
    self.key = key
    self.text = text
    self.embedding = embedding
    self.metadata = metadata
  }
}

public struct GraphRecordBatch: Equatable, Sendable, Codable {
  public var record: GraphRecord
  public var projectedMetadata: [String: GraphMetadataValue]
  public var chunks: [GraphChunk]
  public init(
    record: GraphRecord, projectedMetadata: [String: GraphMetadataValue] = [:], chunks: [GraphChunk]
  ) {
    self.record = record
    self.projectedMetadata = projectedMetadata
    self.chunks = chunks
  }
  enum CodingKeys: String, CodingKey {
    case record
    case projectedMetadata = "projected_metadata"
    case chunks
  }
}

public struct GraphFieldPath: Equatable, Hashable, Sendable, Codable, ExpressibleByStringLiteral {
  public var segments: [String]
  public init(_ segments: [String]) { self.segments = segments }
  public init(_ field: String) { segments = [field] }
  public init(stringLiteral value: String) { segments = [value] }
  public func encode(to encoder: Encoder) throws {
    var c = encoder.singleValueContainer()
    try c.encode(segments)
  }
  public init(from decoder: Decoder) throws {
    let c = try decoder.singleValueContainer()
    segments = try c.decode([String].self)
  }
}

public enum GraphCardinality: String, Sendable, Codable {
  case one = "One"
  case optionalOne = "OptionalOne"
  case many = "Many"
}
public enum GraphMissingTargetPolicy: String, Sendable, Codable {
  case error = "Error"
  case omitEdge = "OmitEdge"
}
public enum GraphDuplicatePolicy: String, Sendable, Codable {
  case error = "Error"
  case deduplicate = "Deduplicate"
}

public struct GraphRecordNodeSchema: Equatable, Sendable, Codable {
  public var recordType: String
  public var nodeType: String
  public var queryableFields: [GraphFieldPath]
  public init(recordType: String, nodeType: String, queryableFields: [GraphFieldPath] = []) {
    self.recordType = recordType
    self.nodeType = nodeType
    self.queryableFields = queryableFields
  }
  enum CodingKeys: String, CodingKey {
    case recordType = "record_type"
    case nodeType = "node_type"
    case queryableFields = "queryable_fields"
  }
}

public struct GraphRelationshipSchema: Equatable, Sendable, Codable {
  public var relationshipType, sourceNodeType, targetNodeType: String
  public var sourceField: GraphFieldPath
  public var cardinality: GraphCardinality
  public var missingTarget: GraphMissingTargetPolicy
  public var duplicateReferences: GraphDuplicatePolicy
  public var allowSelfEdge: Bool
  public var inverseRelationship: String?
  public init(
    relationshipType: String, sourceNodeType: String, targetNodeType: String,
    sourceField: GraphFieldPath, cardinality: GraphCardinality,
    missingTarget: GraphMissingTargetPolicy = .error,
    duplicateReferences: GraphDuplicatePolicy = .error, allowSelfEdge: Bool = false,
    inverseRelationship: String? = nil
  ) {
    self.relationshipType = relationshipType
    self.sourceNodeType = sourceNodeType
    self.targetNodeType = targetNodeType
    self.sourceField = sourceField
    self.cardinality = cardinality
    self.missingTarget = missingTarget
    self.duplicateReferences = duplicateReferences
    self.allowSelfEdge = allowSelfEdge
    self.inverseRelationship = inverseRelationship
  }
  enum CodingKeys: String, CodingKey {
    case relationshipType = "relationship_type"
    case sourceNodeType = "source_node_type"
    case targetNodeType = "target_node_type"
    case sourceField = "source_field"
    case cardinality
    case missingTarget = "missing_target"
    case duplicateReferences = "duplicate_references"
    case allowSelfEdge = "allow_self_edge"
    case inverseRelationship = "inverse_relationship"
  }
}

public struct GraphChunkNodeSchema: Equatable, Sendable, Codable {
  public var nodeType, ownsRelationship: String
  public var inverseRelationship: String?
  public init(nodeType: String, ownsRelationship: String, inverseRelationship: String? = nil) {
    self.nodeType = nodeType
    self.ownsRelationship = ownsRelationship
    self.inverseRelationship = inverseRelationship
  }
  enum CodingKeys: String, CodingKey {
    case nodeType = "node_type"
    case ownsRelationship = "owns_relationship"
    case inverseRelationship = "inverse_relationship"
  }
}

public struct GraphSchema: Equatable, Sendable, Codable {
  public var version: UInt32 = 1
  public var recordNodes: [GraphRecordNodeSchema]
  public var relationships: [GraphRelationshipSchema]
  public var chunkNodes: GraphChunkNodeSchema?
  public init(
    recordNodes: [GraphRecordNodeSchema], relationships: [GraphRelationshipSchema] = [],
    chunkNodes: GraphChunkNodeSchema? = nil
  ) {
    self.recordNodes = recordNodes
    self.relationships = relationships
    self.chunkNodes = chunkNodes
  }
  enum CodingKeys: String, CodingKey {
    case version
    case recordNodes = "record_nodes"
    case relationships
    case chunkNodes = "chunk_nodes"
  }
}

public enum VectorKitGraphError: Error, Equatable, Sendable {
  case invalidSchema(String)
  case invalidIdentity(String)
  case staleGeneration(String)
  case incompatibleVersion(String)
  case graphUnavailable(String)
  case corruptSnapshot(String)
  case queryLimitExceeded(String)
  case cancelled(String)
  case timedOut(String)
  case lockUnavailable(String)
  case invalidDimension(String)
  case missingEmbedding(String)
  case retrievalCapabilityUnavailable(String)
  case internalError(String)
  case consumedBuilder
}

extension VectorKitGraphError: LocalizedError {
  public var errorDescription: String? {
    switch self {
    case .invalidSchema(let message), .invalidIdentity(let message), .staleGeneration(let message),
      .incompatibleVersion(let message), .graphUnavailable(let message),
      .corruptSnapshot(let message), .queryLimitExceeded(let message), .cancelled(let message),
      .timedOut(let message), .lockUnavailable(let message), .invalidDimension(let message),
      .missingEmbedding(let message), .retrievalCapabilityUnavailable(let message),
      .internalError(let message):
      message
    case .consumedBuilder: "graph builder has already been consumed by build(schema:)"
    }
  }
}

public struct GraphNodeID: Equatable, Hashable, Sendable {
  public var nodeType: String
  public var recordID: String
  public var chunkKey: String?
  public init(nodeType: String, recordID: String, chunkKey: String? = nil) {
    self.nodeType = nodeType
    self.recordID = recordID
    self.chunkKey = chunkKey
  }
}
public enum GraphDirection: Sendable { case outgoing, incoming }
public struct GraphTraversal: Equatable, Sendable {
  public var relationship: String
  public var direction: GraphDirection
  public var minHops, maxHops: Int
  public init(
    relationship: String, direction: GraphDirection = .outgoing, minHops: Int = 1, maxHops: Int = 1
  ) {
    self.relationship = relationship
    self.direction = direction
    self.minHops = minHops
    self.maxHops = maxHops
  }
}
public struct GraphQueryLimits: Equatable, Sendable {
  public var maxHops = 8, maxVisited = 100_000, maxResults = 10_000,
    maxWorkingBytes = 64 * 1024 * 1024
  public init(
    maxHops: Int = 8, maxVisited: Int = 100_000, maxResults: Int = 10_000,
    maxWorkingBytes: Int = 64 * 1024 * 1024
  ) {
    self.maxHops = maxHops
    self.maxVisited = maxVisited
    self.maxResults = maxResults
    self.maxWorkingBytes = maxWorkingBytes
  }
}
public enum GraphScalar: Equatable, Sendable {
  case string(String)
  case integer(Int64)
  case boolean(Bool)
}
public struct GraphEdgeProvenance: Equatable, Sendable {
  public let schemaRuleIndex: UInt32
  public let sourceRecordID: String
  public let sourceField: GraphFieldPath?
  public let derivedInverse, builtIn: Bool
  public init(
    schemaRuleIndex: UInt32, sourceRecordID: String, sourceField: GraphFieldPath?,
    derivedInverse: Bool, builtIn: Bool
  ) {
    self.schemaRuleIndex = schemaRuleIndex
    self.sourceRecordID = sourceRecordID
    self.sourceField = sourceField
    self.derivedInverse = derivedInverse
    self.builtIn = builtIn
  }
}
public struct GraphPathEdge: Equatable, Sendable {
  public let relationship: String
  public let source, target: GraphNodeID
  public let occurrenceOrdinal: UInt32
  public let provenance: GraphEdgeProvenance
  public init(
    relationship: String, source: GraphNodeID, target: GraphNodeID, occurrenceOrdinal: UInt32,
    provenance: GraphEdgeProvenance
  ) {
    self.relationship = relationship
    self.source = source
    self.target = target
    self.occurrenceOrdinal = occurrenceOrdinal
    self.provenance = provenance
  }
}
public struct GraphMatch: Equatable, Sendable {
  public let nodeID: GraphNodeID
  public let depth: Int
  public let path: [GraphPathEdge]
  public var pathLength: Int { path.count }
  public init(nodeID: GraphNodeID, depth: Int, path: [GraphPathEdge] = []) {
    self.nodeID = nodeID
    self.depth = depth
    self.path = path
  }
}
public enum GraphTruncationReason: UInt32, Equatable, Sendable {
  case maxHops = 1
  case maxVisited = 2
  case maxResults = 3
  case maxWorkingBytes = 4
}
public struct GraphQueryTrace: Equatable, Sendable {
  public let seedCount, visitedStates, traversedEdges, resultCount, diagnostics: Int
  public let truncationReason: GraphTruncationReason?
}
public struct GraphProjectionTrace: Equatable, Sendable {
  public let sourceNodes, resolvedChunks: Int
}
private final class NativeGraphExecution: @unchecked Sendable {
  private let condition = NSCondition()
  private var result, scope: UInt?
  private var activeUses = 0
  private var closed = false
  init(result: OpaquePointer, scope: OpaquePointer) {
    self.result = UInt(bitPattern: result)
    self.scope = UInt(bitPattern: scope)
  }
  deinit { close() }
  func close() {
    condition.lock()
    closed = true
    while activeUses > 0 { condition.wait() }
    let ownedScope = scope
    let ownedResult = result
    scope = nil
    result = nil
    condition.unlock()
    if let ownedScope { vectorkit_graph_scope_free(OpaquePointer(bitPattern: ownedScope)) }
    if let ownedResult { vectorkit_graph_result_free(OpaquePointer(bitPattern: ownedResult)) }
  }
  func withScope<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
    condition.lock()
    guard !closed, let scope, let pointer = OpaquePointer(bitPattern: scope) else {
      condition.unlock()
      throw VectorKitGraphError.graphUnavailable("graph query result is closed")
    }
    activeUses += 1
    condition.unlock()
    defer {
      condition.lock()
      activeUses -= 1
      condition.broadcast()
      condition.unlock()
    }
    return try body(pointer)
  }
}
public final class GraphQueryResult: @unchecked Sendable {
  public let matches: [GraphMatch]
  public let trace: GraphQueryTrace
  public let projection: GraphProjectionTrace
  fileprivate let native: NativeGraphExecution
  fileprivate init(
    matches: [GraphMatch], trace: GraphQueryTrace, projection: GraphProjectionTrace,
    native: NativeGraphExecution
  ) {
    self.matches = matches
    self.trace = trace
    self.projection = projection
    self.native = native
  }
  public func close() { native.close() }
}
extension GraphQueryResult: Equatable {
  public static func == (left: GraphQueryResult, right: GraphQueryResult) -> Bool {
    left.matches == right.matches && left.trace == right.trace
      && left.projection == right.projection
  }
}
public struct GraphSearchHit: Equatable, Sendable {
  public let chunkID: UInt64
  public let recordID, text: String
  public let score, vectorScore: Float
  public let filterMatched: Bool
}
public struct GraphKeywordHit: Equatable, Sendable {
  public let chunkID: UInt64
  public let recordID, text: String
  public let score: Float
  public let matchedTerms: [String]
}
public struct GraphHybridTrace: Equatable, Sendable {
  public let vectorRank, keywordRank: Int?
  public let normalizedVectorScore, normalizedKeywordScore: Float?
  public let matchedTerms: [String]
  public let filterMatched: Bool
}
public struct GraphHybridHit: Equatable, Sendable {
  public let chunkID: UInt64
  public let recordID, text: String
  public let score: Float
  public let vectorScore, keywordScore: Float?
  public let trace: GraphHybridTrace
  public var matchedTerms: [String] { trace.matchedTerms }
}

public final class GraphCancellationToken: @unchecked Sendable {
  private let condition = NSCondition()
  private var handle: UInt? = UInt(bitPattern: vectorkit_graph_cancellation_new())
  private var activeUses = 0
  private var closed = false
  public init() {}
  deinit { close() }
  public func cancel() {
    condition.lock()
    defer { condition.unlock() }
    if let handle { vectorkit_graph_cancellation_cancel(OpaquePointer(bitPattern: handle)) }
  }
  public func close() {
    condition.lock()
    closed = true
    while activeUses > 0 { condition.wait() }
    let owned = handle
    handle = nil
    condition.unlock()
    if let owned { vectorkit_graph_cancellation_free(OpaquePointer(bitPattern: owned)) }
  }
  fileprivate func withPointer<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
    condition.lock()
    guard !closed, let handle, let pointer = OpaquePointer(bitPattern: handle) else {
      condition.unlock()
      throw VectorKitGraphError.graphUnavailable("graph cancellation token is closed")
    }
    activeUses += 1
    condition.unlock()
    defer {
      condition.lock()
      activeUses -= 1
      condition.broadcast()
      condition.unlock()
    }
    return try body(pointer)
  }
}

private final class GraphCStringArena {
  private var values: [UnsafeMutablePointer<CChar>] = []
  deinit { values.forEach { $0.deallocate() } }
  func copy(_ value: String) -> UnsafePointer<CChar> {
    let bytes = Array(value.utf8CString)
    let pointer = UnsafeMutablePointer<CChar>.allocate(capacity: bytes.count)
    pointer.initialize(from: bytes, count: bytes.count)
    values.append(pointer)
    return UnsafePointer(pointer)
  }
}

private enum Native {
  static func error(_ status: VkStatus, fallback: String) -> VectorKitGraphError {
    let message = status.message.map { String(cString: $0) } ?? fallback
    switch status.code {
    case VK_STATUS_INVALID_ARGUMENT: return .internalError(message)
    case VK_GRAPH_STATUS_INVALID_SCHEMA: return .invalidSchema(message)
    case VK_GRAPH_STATUS_INVALID_IDENTITY: return .invalidIdentity(message)
    case VK_GRAPH_STATUS_STALE_GENERATION: return .staleGeneration(message)
    case VK_GRAPH_STATUS_INCOMPATIBLE_VERSION: return .incompatibleVersion(message)
    case VK_GRAPH_STATUS_GRAPH_UNAVAILABLE: return .graphUnavailable(message)
    case VK_GRAPH_STATUS_CORRUPT_SNAPSHOT, VK_STATUS_CORRUPT_INDEX: return .corruptSnapshot(message)
    case VK_GRAPH_STATUS_QUERY_LIMIT_EXCEEDED: return .queryLimitExceeded(message)
    case VK_GRAPH_STATUS_CANCELLED: return .cancelled(message)
    case VK_GRAPH_STATUS_TIMED_OUT: return .timedOut(message)
    case VK_GRAPH_STATUS_LOCK_UNAVAILABLE: return .lockUnavailable(message)
    case VK_GRAPH_STATUS_INVALID_DIMENSION: return .invalidDimension(message)
    case VK_GRAPH_STATUS_MISSING_EMBEDDING: return .missingEmbedding(message)
    case VK_GRAPH_STATUS_RETRIEVAL_CAPABILITY_UNAVAILABLE:
      return .retrievalCapabilityUnavailable(message)
    case VK_GRAPH_STATUS_INTERNAL, VK_STATUS_CORE_ERROR: return .internalError(message)
    default: return .internalError(message)
    }
  }
  static func pointer(_ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?) throws
    -> OpaquePointer
  {
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    guard let result = body(&status) else { throw error(status, fallback: "unknown graph error") }
    return result
  }
  static func bool(_ body: (UnsafeMutablePointer<VkStatus>) -> Bool) throws {
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    guard body(&status) else { throw error(status, fallback: "unknown graph error") }
  }
  static func filterPointer(_ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?) throws
    -> OpaquePointer
  {
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    guard let pointer = body(&status) else {
      if status.code == VK_STATUS_INVALID_ARGUMENT {
        throw VectorKitGraphError.invalidIdentity(
          status.message.map { String(cString: $0) } ?? "invalid metadata filter")
      }
      throw error(status, fallback: "could not create metadata filter")
    }
    return pointer
  }
}

private final class NativeGraphFilter {
  let pointer: OpaquePointer
  init(_ pointer: OpaquePointer) { self.pointer = pointer }
  deinit { vectorkit_filter_free(pointer) }
}

extension GraphFilter {
  fileprivate func makeFFI() throws -> NativeGraphFilter { NativeGraphFilter(try makeFFIPointer()) }

  fileprivate func makeFFIPointer() throws -> OpaquePointer {
    switch self {
    case .equals(let field, let value):
      try makeLeaf { status, arena in
        vectorkit_filter_equals(arena.copy(field), value.ffiValue(arena: arena), status)
      }
    case .notEquals(let field, let value):
      try makeLeaf { status, arena in
        vectorkit_filter_not_equals(arena.copy(field), value.ffiValue(arena: arena), status)
      }
    case .exists(let field):
      try makeLeaf { status, arena in vectorkit_filter_exists(arena.copy(field), status) }
    case .range(let field, let lower, let upper):
      try makeLeaf { status, arena in
        var lowerValue = lower?.ffiValue(arena: arena)
        var upperValue = upper?.ffiValue(arena: arena)
        return withOptionalGraphPointer(to: &lowerValue) { lowerPointer in
          withOptionalGraphPointer(to: &upperValue) { upperPointer in
            vectorkit_filter_range(arena.copy(field), lowerPointer, upperPointer, status)
          }
        }
      }
    case .inValues(let field, let values):
      try makeLeaf { status, arena in
        let nativeValues = values.map { $0.ffiValue(arena: arena) }
        return nativeValues.withUnsafeBufferPointer { buffer in
          vectorkit_filter_in_values(arena.copy(field), buffer.baseAddress, buffer.count, status)
        }
      }
    case .all(let filters): try makeComposite(filters, builder: vectorkit_filter_all)
    case .any(let filters): try makeComposite(filters, builder: vectorkit_filter_any)
    }
  }

  fileprivate func makeLeaf(
    _ body: (UnsafeMutablePointer<VkStatus>, GraphCStringArena) -> OpaquePointer?
  ) throws -> OpaquePointer {
    try Native.filterPointer { status in body(status, GraphCStringArena()) }
  }

  fileprivate func makeComposite(
    _ filters: [GraphFilter],
    builder: (UnsafePointer<OpaquePointer?>?, Int, UnsafeMutablePointer<VkStatus>?) ->
      OpaquePointer?
  ) throws -> OpaquePointer {
    let children = try filters.map { try $0.makeFFI() }
    return try Native.filterPointer { status in
      let pointers = children.map { Optional($0.pointer) }
      return pointers.withUnsafeBufferPointer { builder($0.baseAddress, $0.count, status) }
    }
  }
}

actor GraphReadWriteGate {
  private var activeReaders = 0
  private var writerActive = false
  private var waitingReaders: [CheckedContinuation<Void, Never>] = []
  private var waitingWriters: [CheckedContinuation<Void, Never>] = []
  func withRead<T: Sendable>(_ operation: @Sendable () async throws -> T) async rethrows -> T {
    await beginRead()
    do {
      let value = try await operation()
      endRead()
      return value
    } catch {
      endRead()
      throw error
    }
  }
  func withWrite<T: Sendable>(_ operation: @Sendable () async throws -> T) async rethrows -> T {
    await beginWrite()
    do {
      let value = try await operation()
      endWrite()
      return value
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
    await withCheckedContinuation { waitingReaders.append($0) }
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
    await withCheckedContinuation { waitingWriters.append($0) }
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
    readers.forEach { $0.resume() }
  }
}

private final class NativeGraphIndexOwner: @unchecked Sendable {
  private let lock = NSLock()
  private var handle: UInt?
  init(_ handle: UInt) { self.handle = handle }
  deinit { close() }
  func requireHandle() throws -> UInt {
    lock.lock()
    defer { lock.unlock() }
    guard let handle else { throw VectorKitGraphError.graphUnavailable("graph index is closed") }
    return handle
  }
  func close() {
    lock.lock()
    let owned = handle
    handle = nil
    lock.unlock()
    if let owned { vectorkit_graph_index_free(OpaquePointer(bitPattern: owned)) }
  }
}

public actor GraphIndexBuilder {
  private var handle: UInt?
  private var closed = false
  public init(
    dimension: Int, corpusID: String, metric: GraphMetric = .cosine,
    encoding: GraphVectorEncoding = .f32
  ) throws {
    try requireNonnegative(dimension, name: "embedding dimension")
    handle = UInt(
      bitPattern: try Native.pointer { status in
        corpusID.withCString {
          vectorkit_graph_builder_new(dimension, metric.ffi, encoding.ffi, $0, status)
        }
      })
  }
  deinit { if let handle { vectorkit_graph_builder_free(OpaquePointer(bitPattern: handle)) } }
  public func close() {
    if let handle {
      vectorkit_graph_builder_free(OpaquePointer(bitPattern: handle))
      self.handle = nil
    }
    closed = true
  }
  public func upsert(_ batch: GraphRecordBatch) throws {
    if closed { throw VectorKitGraphError.graphUnavailable("graph builder is closed") }
    guard let handle else { throw VectorKitGraphError.consumedBuilder }
    let data = try JSONEncoder().encode(batch)
    let json = String(decoding: data, as: UTF8.self)
    try Native.bool { status in
      json.withCString {
        vectorkit_graph_builder_upsert_record_json(OpaquePointer(bitPattern: handle), $0, status)
      }
    }
  }
  public func build(schema: GraphSchema) throws -> GraphIndex {
    if closed { throw VectorKitGraphError.graphUnavailable("graph builder is closed") }
    guard let owned = handle else { throw VectorKitGraphError.consumedBuilder }
    handle = nil
    let data = try JSONEncoder().encode(schema)
    let json = String(decoding: data, as: UTF8.self)
    let graph = try Native.pointer { status in
      json.withCString {
        vectorkit_graph_builder_build_json(OpaquePointer(bitPattern: owned), $0, status)
      }
    }
    return GraphIndex(handle: UInt(bitPattern: graph))
  }
}

public actor GraphIndex {
  private let owner: NativeGraphIndexOwner
  private let access = GraphReadWriteGate()
  fileprivate init(handle: UInt) { owner = NativeGraphIndexOwner(handle) }
  public func close() async {
    let owner = owner
    await access.withWrite { owner.close() }
  }
  public func save(to directory: URL) async throws {
    let owner = owner
    try await access.withWrite {
      try await Task.detached(priority: Task.currentPriority) {
        let handle = try owner.requireHandle()
        try Native.bool { status in
          directory.path.withCString {
            vectorkit_graph_index_save(OpaquePointer(bitPattern: handle), $0, status)
          }
        }
      }.value
    }
  }
  public static func load(from directory: URL) throws -> GraphIndex {
    GraphIndex(
      handle: UInt(
        bitPattern: try Native.pointer { status in
          directory.path.withCString { vectorkit_graph_index_load($0, status) }
        }))
  }
  public static func validate(at directory: URL) throws {
    try Native.bool { status in
      directory.path.withCString { vectorkit_graph_index_validate($0, status) }
    }
  }

  public func query(
    from seeds: [GraphNodeID], traversing steps: [GraphTraversal] = [],
    limits: GraphQueryLimits = .init(), cancellation: GraphCancellationToken? = nil
  ) async throws -> GraphQueryResult {
    let owner = owner
    return try await access.withRead {
      try await Task.detached(priority: Task.currentPriority) {
        try Self.performNodeQuery(
          handle: owner.requireHandle(), seeds: seeds, steps: steps, limits: limits,
          cancellation: cancellation)
      }.value
    }
  }

  public func query(
    nodeType: String, field: GraphFieldPath, equals values: [GraphScalar],
    traversing steps: [GraphTraversal] = [], limits: GraphQueryLimits = .init(),
    cancellation: GraphCancellationToken? = nil
  ) async throws -> GraphQueryResult {
    let owner = owner
    return try await access.withRead {
      try await Task.detached(priority: Task.currentPriority) {
        try Self.performEqualityQuery(
          handle: owner.requireHandle(), nodeType: nodeType, field: field, values: values,
          steps: steps, limits: limits, cancellation: cancellation)
      }.value
    }
  }

  public func search(
    _ embedding: [Float], topK: Int, in result: GraphQueryResult, filter: GraphFilter? = nil
  ) async throws -> [GraphSearchHit] {
    let owner = owner
    return try await access.withRead {
      try await Task.detached(priority: Task.currentPriority) {
        try Self.performSearch(
          handle: owner.requireHandle(), embedding: embedding, topK: topK, result: result,
          filter: filter)
      }.value
    }
  }

  public func keywordSearch(
    _ text: String, topK: Int, in result: GraphQueryResult, filter: GraphFilter? = nil
  ) async throws -> [GraphKeywordHit] {
    let owner = owner
    return try await access.withRead {
      try await Task.detached(priority: Task.currentPriority) {
        try Self.performKeywordSearch(
          handle: owner.requireHandle(), text: text, topK: topK, result: result, filter: filter)
      }.value
    }
  }

  public func hybridSearch(
    text: String, embedding: [Float], topK: Int, in result: GraphQueryResult,
    filter: GraphFilter? = nil, alpha: Float = 0.6, options: GraphHybridOptions = .default
  ) async throws -> [GraphHybridHit] {
    let owner = owner
    return try await access.withRead {
      try await Task.detached(priority: Task.currentPriority) {
        try Self.performHybridSearch(
          handle: owner.requireHandle(), text: text, embedding: embedding, topK: topK,
          result: result, filter: filter, alpha: alpha, options: options)
      }.value
    }
  }

  private nonisolated static func performNodeQuery(
    handle: UInt, seeds: [GraphNodeID], steps: [GraphTraversal], limits: GraphQueryLimits,
    cancellation: GraphCancellationToken?
  ) throws -> GraphQueryResult {
    try validateQuerySizes(steps: steps, limits: limits)
    let arena = GraphCStringArena()
    let nativeSeeds = seeds.map { seed in
      VkGraphNodeRef(
        node_type: arena.copy(seed.nodeType), source_type: seed.chunkKey == nil ? 0 : 1,
        record_id: arena.copy(seed.recordID), chunk_key: seed.chunkKey.map(arena.copy))
    }
    let nativeSteps = steps.map { step in
      VkGraphStep(
        relationship: arena.copy(step.relationship), direction: step.direction == .outgoing ? 0 : 1,
        min_hops: step.minHops, max_hops: step.maxHops)
    }
    let seedPointer = UnsafeMutablePointer<VkGraphNodeRef>.allocate(
      capacity: max(1, nativeSeeds.count))
    let stepPointer = UnsafeMutablePointer<VkGraphStep>.allocate(
      capacity: max(1, nativeSteps.count))
    for (index, value) in nativeSeeds.enumerated() {
      seedPointer.advanced(by: index).initialize(to: value)
    }
    for (index, value) in nativeSteps.enumerated() {
      stepPointer.advanced(by: index).initialize(to: value)
    }
    defer {
      seedPointer.deinitialize(count: nativeSeeds.count)
      seedPointer.deallocate()
      stepPointer.deinitialize(count: nativeSteps.count)
      stepPointer.deallocate()
    }
    let query = VkGraphQuery(
      seed_type: 0, node_ids: nativeSeeds.isEmpty ? nil : seedPointer,
      node_id_count: nativeSeeds.count, seed_node_type: nil, field_segments: nil,
      field_segment_count: 0, values: nil, value_count: 0,
      steps: nativeSteps.isEmpty ? nil : stepPointer, step_count: nativeSteps.count,
      limits: nativeLimits(limits))
    return try execute(handle: handle, query, cancellation: cancellation)
  }

  private nonisolated static func performEqualityQuery(
    handle: UInt, nodeType: String, field: GraphFieldPath, values: [GraphScalar],
    steps: [GraphTraversal], limits: GraphQueryLimits, cancellation: GraphCancellationToken?
  ) throws -> GraphQueryResult {
    try validateQuerySizes(steps: steps, limits: limits)
    let arena = GraphCStringArena()
    let nativeFields = field.segments.map(arena.copy)
    let nativeValues = values.map { value in
      switch value {
      case .string(let string):
        VkGraphScalar(
          value_type: 0, string_value: arena.copy(string), integer_value: 0, bool_value: false)
      case .integer(let integer):
        VkGraphScalar(value_type: 1, string_value: nil, integer_value: integer, bool_value: false)
      case .boolean(let boolean):
        VkGraphScalar(value_type: 2, string_value: nil, integer_value: 0, bool_value: boolean)
      }
    }
    let nativeSteps = steps.map { step in
      VkGraphStep(
        relationship: arena.copy(step.relationship), direction: step.direction == .outgoing ? 0 : 1,
        min_hops: step.minHops, max_hops: step.maxHops)
    }
    let fieldPointer = UnsafeMutablePointer<UnsafePointer<CChar>?>.allocate(
      capacity: max(1, nativeFields.count))
    let valuePointer = UnsafeMutablePointer<VkGraphScalar>.allocate(
      capacity: max(1, nativeValues.count))
    let stepPointer = UnsafeMutablePointer<VkGraphStep>.allocate(
      capacity: max(1, nativeSteps.count))
    for (index, value) in nativeFields.enumerated() {
      fieldPointer.advanced(by: index).initialize(to: value)
    }
    for (index, value) in nativeValues.enumerated() {
      valuePointer.advanced(by: index).initialize(to: value)
    }
    for (index, value) in nativeSteps.enumerated() {
      stepPointer.advanced(by: index).initialize(to: value)
    }
    defer {
      fieldPointer.deinitialize(count: nativeFields.count)
      fieldPointer.deallocate()
      valuePointer.deinitialize(count: nativeValues.count)
      valuePointer.deallocate()
      stepPointer.deinitialize(count: nativeSteps.count)
      stepPointer.deallocate()
    }
    let query = VkGraphQuery(
      seed_type: 1, node_ids: nil, node_id_count: 0, seed_node_type: arena.copy(nodeType),
      field_segments: nativeFields.isEmpty ? nil : fieldPointer,
      field_segment_count: nativeFields.count, values: nativeValues.isEmpty ? nil : valuePointer,
      value_count: nativeValues.count, steps: nativeSteps.isEmpty ? nil : stepPointer,
      step_count: nativeSteps.count, limits: nativeLimits(limits))
    return try execute(handle: handle, query, cancellation: cancellation)
  }

  private nonisolated static func execute(
    handle: UInt, _ query: VkGraphQuery, cancellation: GraphCancellationToken?
  ) throws -> GraphQueryResult {
    var status = VkStatus(code: 0, message: nil)
    let result: OpaquePointer? =
      if let cancellation {
        try cancellation.withPointer { pointer in
          vectorkit_graph_query(OpaquePointer(bitPattern: handle), query, pointer, &status)
        }
      } else {
        vectorkit_graph_query(OpaquePointer(bitPattern: handle), query, nil, &status)
      }
    guard let result else {
      defer { vectorkit_status_clear(&status) }
      throw Native.error(status, fallback: "unknown graph error")
    }
    vectorkit_status_clear(&status)
    guard
      let scope = vectorkit_graph_result_project(OpaquePointer(bitPattern: handle), result, &status)
    else {
      defer {
        vectorkit_status_clear(&status)
        vectorkit_graph_result_free(result)
      }
      throw Native.error(status, fallback: "graph projection failed")
    }
    let projection = GraphProjectionTrace(
      sourceNodes: vectorkit_graph_scope_source_nodes(scope),
      resolvedChunks: vectorkit_graph_scope_resolved_chunks(scope))
    let execution = NativeGraphExecution(result: result, scope: scope)
    var matches: [GraphMatch] = []
    for index in 0..<vectorkit_graph_result_count(result) {
      var value = VkGraphMatch(
        node_type: nil, source_type: 0, record_id: nil, chunk_key: nil, depth: 0, path_length: 0)
      try Native.bool { status in vectorkit_graph_result_match(result, index, &value, status) }
      let nodeID = graphNodeID(
        nodeType: value.node_type, recordID: value.record_id, chunkKey: value.chunk_key)
      let depth = value.depth
      let pathLength = value.path_length
      vectorkit_graph_match_clear(&value)
      var path: [GraphPathEdge] = []
      for edgeIndex in 0..<pathLength {
        var edge = emptyPathEdge()
        try Native.bool { status in
          vectorkit_graph_result_path_edge(result, index, edgeIndex, &edge, status)
        }
        let sourceField =
          edge.source_field_segments.count == 0
          ? nil
          : GraphFieldPath(
            (0..<edge.source_field_segments.count).map {
              String(cString: edge.source_field_segments.values[$0]!)
            })
        path.append(
          GraphPathEdge(
            relationship: String(cString: edge.relationship_type), source: graphNodeID(edge.source),
            target: graphNodeID(edge.target), occurrenceOrdinal: edge.occurrence_ordinal,
            provenance: GraphEdgeProvenance(
              schemaRuleIndex: edge.schema_rule_index,
              sourceRecordID: String(cString: edge.source_record_id), sourceField: sourceField,
              derivedInverse: edge.derived_inverse, builtIn: edge.built_in)))
        vectorkit_graph_path_edge_clear(&edge)
      }
      matches.append(GraphMatch(nodeID: nodeID, depth: depth, path: path))
    }
    let trace = vectorkit_graph_result_trace(result)
    return GraphQueryResult(
      matches: matches,
      trace: GraphQueryTrace(
        seedCount: trace.seed_count, visitedStates: trace.visited_states,
        traversedEdges: trace.traversed_edges, resultCount: trace.result_count,
        diagnostics: trace.diagnostics,
        truncationReason: GraphTruncationReason(rawValue: trace.truncation_reason)),
      projection: projection, native: execution)
  }

  private nonisolated static func performSearch(
    handle: UInt, embedding: [Float], topK: Int, result: GraphQueryResult, filter: GraphFilter?
  ) throws -> [GraphSearchHit] {
    try requireNonnegative(topK, name: "topK")
    let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count))
    for (index, value) in embedding.enumerated() {
      pointer.advanced(by: index).initialize(to: value)
    }
    defer {
      pointer.deinitialize(count: embedding.count)
      pointer.deallocate()
    }
    let nativeFilter = try filter?.makeFFI()
    var output = VkSearchResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    let succeeded = try result.native.withScope { scope in
      vectorkit_graph_scope_search(
        OpaquePointer(bitPattern: handle), scope, embedding.isEmpty ? nil : pointer,
        embedding.count, topK, nativeFilter?.pointer, &output, &status)
    }
    guard succeeded else { throw Native.error(status, fallback: "scoped search failed") }
    defer { vectorkit_search_results_free(output) }
    return (0..<output.count).map { index in
      let hit = output.hits[index]
      return GraphSearchHit(
        chunkID: hit.chunk_id, recordID: String(cString: hit.document_id),
        text: String(cString: hit.text), score: hit.score, vectorScore: hit.vector_score,
        filterMatched: hit.filter_matched)
    }
  }

  private nonisolated static func performKeywordSearch(
    handle: UInt, text: String, topK: Int, result: GraphQueryResult, filter: GraphFilter?
  ) throws -> [GraphKeywordHit] {
    try requireNonnegative(topK, name: "topK")
    let nativeFilter = try filter?.makeFFI()
    var output = VkKeywordResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    let arena = GraphCStringArena()
    let ok = try result.native.withScope { scope in
      vectorkit_graph_scope_keyword_search(
        OpaquePointer(bitPattern: handle), scope, arena.copy(text), topK, nativeFilter?.pointer,
        &output, &status)
    }
    guard ok else { throw Native.error(status, fallback: "scoped keyword search failed") }
    defer { vectorkit_keyword_results_free(output) }
    return (0..<output.count).map { index in
      let hit = output.hits[index]
      let terms = (0..<hit.matched_terms.count).map {
        String(cString: hit.matched_terms.values[$0]!)
      }
      return GraphKeywordHit(
        chunkID: hit.chunk_id, recordID: String(cString: hit.document_id),
        text: String(cString: hit.text), score: hit.score, matchedTerms: terms)
    }
  }

  private nonisolated static func performHybridSearch(
    handle: UInt, text: String, embedding: [Float], topK: Int, result: GraphQueryResult,
    filter: GraphFilter?, alpha: Float, options: GraphHybridOptions
  ) throws -> [GraphHybridHit] {
    try requireNonnegative(topK, name: "topK")
    try requireNonnegative(options.vectorTopK, name: "vectorTopK")
    try requireNonnegative(options.keywordTopK, name: "keywordTopK")
    let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count))
    for (index, value) in embedding.enumerated() {
      pointer.advanced(by: index).initialize(to: value)
    }
    defer {
      pointer.deinitialize(count: embedding.count)
      pointer.deallocate()
    }
    let nativeFilter = try filter?.makeFFI()
    let arena = GraphCStringArena()
    var output = VkHybridResultBuffer(hits: nil, count: 0)
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    let succeeded = try result.native.withScope { scope in
      vectorkit_graph_scope_hybrid_search(
        OpaquePointer(bitPattern: handle), scope, arena.copy(text),
        embedding.isEmpty ? nil : pointer, embedding.count, topK, nativeFilter?.pointer,
        options.ffiValue(alpha: alpha), &output, &status)
    }
    guard succeeded else { throw Native.error(status, fallback: "scoped hybrid search failed") }
    defer { vectorkit_hybrid_results_free(output) }
    return (0..<output.count).map { index in
      let hit = output.hits[index]
      let terms = (0..<hit.matched_terms.count).map {
        String(cString: hit.matched_terms.values[$0]!)
      }
      let trace = GraphHybridTrace(
        vectorRank: hit.has_vector_rank ? hit.vector_rank : nil,
        keywordRank: hit.has_keyword_rank ? hit.keyword_rank : nil,
        normalizedVectorScore: hit.has_normalized_vector_score ? hit.normalized_vector_score : nil,
        normalizedKeywordScore: hit.has_normalized_keyword_score
          ? hit.normalized_keyword_score : nil, matchedTerms: terms,
        filterMatched: hit.filter_matched)
      return GraphHybridHit(
        chunkID: hit.chunk_id, recordID: String(cString: hit.document_id),
        text: String(cString: hit.text), score: hit.score,
        vectorScore: hit.has_vector_score ? hit.vector_score : nil,
        keywordScore: hit.has_keyword_score ? hit.keyword_score : nil, trace: trace)
    }
  }
}

private func nativeLimits(_ limits: GraphQueryLimits) -> VkGraphLimits {
  VkGraphLimits(
    max_hops: limits.maxHops, max_visited: limits.maxVisited, max_results: limits.maxResults,
    max_working_bytes: limits.maxWorkingBytes)
}

private func requireNonnegative(_ value: Int, name: String) throws {
  guard value >= 0 else {
    throw VectorKitGraphError.invalidIdentity("\(name) must not be negative")
  }
}

private func validateQuerySizes(steps: [GraphTraversal], limits: GraphQueryLimits) throws {
  try requireNonnegative(limits.maxHops, name: "maxHops")
  try requireNonnegative(limits.maxVisited, name: "maxVisited")
  try requireNonnegative(limits.maxResults, name: "maxResults")
  try requireNonnegative(limits.maxWorkingBytes, name: "maxWorkingBytes")
  for step in steps {
    try requireNonnegative(step.minHops, name: "minHops")
    try requireNonnegative(step.maxHops, name: "maxHops")
  }
}

private func graphNodeID(
  nodeType: UnsafeMutablePointer<CChar>?, recordID: UnsafeMutablePointer<CChar>?,
  chunkKey: UnsafeMutablePointer<CChar>?
) -> GraphNodeID {
  GraphNodeID(
    nodeType: String(cString: nodeType!), recordID: String(cString: recordID!),
    chunkKey: chunkKey.map { String(cString: $0) })
}

private func graphNodeID(_ value: VkGraphOwnedNode) -> GraphNodeID {
  graphNodeID(nodeType: value.node_type, recordID: value.record_id, chunkKey: value.chunk_key)
}

private func emptyOwnedNode() -> VkGraphOwnedNode {
  VkGraphOwnedNode(node_type: nil, source_type: 0, record_id: nil, chunk_key: nil)
}

private func emptyPathEdge() -> VkGraphPathEdge {
  VkGraphPathEdge(
    relationship_type: nil, source: emptyOwnedNode(), target: emptyOwnedNode(),
    occurrence_ordinal: 0, schema_rule_index: 0, source_record_id: nil,
    source_field_segments: VkStringArray(values: nil, count: 0), derived_inverse: false,
    built_in: false)
}

private func withOptionalGraphPointer<T, R>(to value: inout T?, _ body: (UnsafePointer<T>?) -> R)
  -> R
{
  guard var unwrapped = value else { return body(nil) }
  return withUnsafePointer(to: &unwrapped, body)
}

public struct VectorIndexConfiguration: Equatable, Sendable {
  public var dimension: Int
  public var metric: GraphMetric
  public var encoding: GraphVectorEncoding
  public init(
    dimension: Int, metric: GraphMetric = .cosine,
    encoding: GraphVectorEncoding = .i8ScalarQuantized
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

private struct CapabilityGraphBatch: Encodable {
  let record: Record
  let projectedMetadata: [String: MetadataValue]
  let chunks: [Chunk]
  enum CodingKeys: String, CodingKey {
    case record
    case projectedMetadata = "projected_metadata"
    case chunks
  }
}
private struct CapabilityRetrievalChunk: Encodable {
  let key: ChunkKey
  let text: String
  let embedding: [Float]
  let metadata: [String: MetadataValue]
}
private struct CapabilityRetrievalBatch: Encodable {
  let record: Record
  let projectedMetadata: [String: MetadataValue]
  let chunks: [CapabilityRetrievalChunk]
  enum CodingKeys: String, CodingKey {
    case record
    case projectedMetadata = "projected_metadata"
    case chunks
  }
}

private enum CapabilityDatabaseKind: Sendable { case graph, graphRetrieval }
private final class NativeCapabilityDatabase: @unchecked Sendable {
  let kind: CapabilityDatabaseKind
  private let lock = NSLock()
  private var handle: UInt?
  init(kind: CapabilityDatabaseKind, handle: OpaquePointer) {
    self.kind = kind
    self.handle = UInt(bitPattern: handle)
  }
  deinit { close() }
  func requireHandle() throws -> UInt {
    lock.lock()
    defer { lock.unlock() }
    guard let handle else { throw VectorKitGraphError.graphUnavailable("database is closed") }
    return handle
  }
  func close() {
    lock.lock()
    let owned = handle
    handle = nil
    lock.unlock()
    guard let owned else { return }
    switch kind {
    case .graph: vectorkit_graph_database_free(OpaquePointer(bitPattern: owned))
    case .graphRetrieval: vectorkit_graph_retrieval_database_free(OpaquePointer(bitPattern: owned))
    }
  }
}
private final class NativeSelection: @unchecked Sendable {
  private let lock = NSLock()
  private var handle: UInt?
  init(_ handle: OpaquePointer) { self.handle = UInt(bitPattern: handle) }
  deinit {
    lock.lock()
    let owned = handle
    handle = nil
    lock.unlock()
    if let owned { vectorkit_graph_result_free(OpaquePointer(bitPattern: owned)) }
  }
  func requireHandle() throws -> OpaquePointer {
    lock.lock()
    defer { lock.unlock() }
    guard let handle, let pointer = OpaquePointer(bitPattern: handle) else {
      throw VectorKitGraphError.graphUnavailable("graph selection is closed")
    }
    return pointer
  }
}
public final class GraphSelection: @unchecked Sendable {
  public let matches: [GraphMatch]
  public let trace: GraphQueryTrace
  fileprivate let native: NativeSelection
  fileprivate init(matches: [GraphMatch], trace: GraphQueryTrace, native: NativeSelection) {
    self.matches = matches
    self.trace = trace
    self.native = native
  }
}

public struct GraphChunkIdentity: Equatable, Hashable, Sendable {
  public let recordID: RecordID
  public let chunkKey: ChunkKey
  public init(recordID: RecordID, chunkKey: ChunkKey) {
    self.recordID = recordID
    self.chunkKey = chunkKey
  }
}

public struct GraphCandidateProjection: Equatable, Sendable {
  public let candidates: [GraphChunkIdentity]
  public let sourceNodes: Int
  public let projectedChunksBeforeFilter: Int
  public let projectedChunksAfterFilter: Int
  public init(
    candidates: [GraphChunkIdentity], sourceNodes: Int, projectedChunksBeforeFilter: Int,
    projectedChunksAfterFilter: Int
  ) {
    self.candidates = candidates
    self.sourceNodes = sourceNodes
    self.projectedChunksBeforeFilter = projectedChunksBeforeFilter
    self.projectedChunksAfterFilter = projectedChunksAfterFilter
  }
}

public actor GraphQueries {
  private let owner: NativeCapabilityDatabase
  fileprivate init(owner: NativeCapabilityDatabase) { self.owner = owner }
  public func query(
    from seeds: [GraphNodeID], traversing steps: [GraphTraversal] = [],
    limits: GraphQueryLimits = .init(), cancellation: GraphCancellationToken? = nil
  ) async throws -> GraphSelection {
    try validateQuerySizes(steps: steps, limits: limits)
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      try Self.nodeQuery(
        owner: owner, seeds: seeds, steps: steps, limits: limits, cancellation: cancellation)
    }.value
  }
  public func query(
    nodeType: String, field: GraphFieldPath, equals values: [GraphScalar],
    traversing steps: [GraphTraversal] = [], limits: GraphQueryLimits = .init(),
    cancellation: GraphCancellationToken? = nil
  ) async throws -> GraphSelection {
    try validateQuerySizes(steps: steps, limits: limits)
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      try Self.equalityQuery(
        owner: owner, nodeType: nodeType, field: field, values: values, steps: steps,
        limits: limits, cancellation: cancellation)
    }.value
  }
  public func query(
    nodeType: String, field: GraphFieldPath, equals value: GraphScalar,
    traversing steps: [GraphTraversal] = [], limits: GraphQueryLimits = .init(),
    cancellation: GraphCancellationToken? = nil
  ) async throws -> GraphSelection {
    try await query(
      nodeType: nodeType,
      field: field,
      equals: [value],
      traversing: steps,
      limits: limits,
      cancellation: cancellation
    )
  }
  private nonisolated static func nodeQuery(
    owner: NativeCapabilityDatabase, seeds: [GraphNodeID], steps: [GraphTraversal],
    limits: GraphQueryLimits, cancellation: GraphCancellationToken?
  ) throws -> GraphSelection {
    let arena = GraphCStringArena()
    let nativeSeeds = seeds.map {
      VkGraphNodeRef(
        node_type: arena.copy($0.nodeType), source_type: $0.chunkKey == nil ? 0 : 1,
        record_id: arena.copy($0.recordID), chunk_key: $0.chunkKey.map(arena.copy))
    }
    let nativeSteps = steps.map {
      VkGraphStep(
        relationship: arena.copy($0.relationship), direction: $0.direction == .outgoing ? 0 : 1,
        min_hops: $0.minHops, max_hops: $0.maxHops)
    }
    return try nativeSeeds.withUnsafeBufferPointer { seedBuffer in
      try nativeSteps.withUnsafeBufferPointer { stepBuffer in
        try execute(
          owner: owner,
          query: VkGraphQuery(
            seed_type: 0, node_ids: seedBuffer.baseAddress, node_id_count: seedBuffer.count,
            seed_node_type: nil, field_segments: nil, field_segment_count: 0, values: nil,
            value_count: 0, steps: stepBuffer.baseAddress, step_count: stepBuffer.count,
            limits: nativeLimits(limits)), cancellation: cancellation)
      }
    }
  }
  private nonisolated static func equalityQuery(
    owner: NativeCapabilityDatabase, nodeType: String, field: GraphFieldPath, values: [GraphScalar],
    steps: [GraphTraversal], limits: GraphQueryLimits, cancellation: GraphCancellationToken?
  ) throws -> GraphSelection {
    let arena = GraphCStringArena()
    let fields: [UnsafePointer<CChar>?] = field.segments.map(arena.copy)
    let scalars = values.map { value in
      switch value {
      case .string(let v):
        VkGraphScalar(
          value_type: 0, string_value: arena.copy(v), integer_value: 0, bool_value: false)
      case .integer(let v):
        VkGraphScalar(value_type: 1, string_value: nil, integer_value: v, bool_value: false)
      case .boolean(let v):
        VkGraphScalar(value_type: 2, string_value: nil, integer_value: 0, bool_value: v)
      }
    }
    let nativeSteps = steps.map {
      VkGraphStep(
        relationship: arena.copy($0.relationship), direction: $0.direction == .outgoing ? 0 : 1,
        min_hops: $0.minHops, max_hops: $0.maxHops)
    }
    return try fields.withUnsafeBufferPointer { fieldBuffer in
      try scalars.withUnsafeBufferPointer { scalarBuffer in
        try nativeSteps.withUnsafeBufferPointer { stepBuffer in
          try execute(
            owner: owner,
            query: VkGraphQuery(
              seed_type: 1, node_ids: nil, node_id_count: 0, seed_node_type: arena.copy(nodeType),
              field_segments: fieldBuffer.baseAddress, field_segment_count: fieldBuffer.count,
              values: scalarBuffer.baseAddress, value_count: scalarBuffer.count,
              steps: stepBuffer.baseAddress, step_count: stepBuffer.count,
              limits: nativeLimits(limits)), cancellation: cancellation)
        }
      }
    }
  }
  private nonisolated static func execute(
    owner: NativeCapabilityDatabase, query: VkGraphQuery, cancellation: GraphCancellationToken?
  ) throws -> GraphSelection {
    let handle = try owner.requireHandle()
    var status = VkStatus(code: 0, message: nil)
    defer { vectorkit_status_clear(&status) }
    let call: (OpaquePointer?) throws -> OpaquePointer? = { cancellationPointer in
      switch owner.kind {
      case .graph:
        vectorkit_graph_database_query(
          OpaquePointer(bitPattern: handle), query, cancellationPointer, &status)
      case .graphRetrieval:
        vectorkit_graph_retrieval_database_query(
          OpaquePointer(bitPattern: handle), query, cancellationPointer, &status)
      }
    }
    let result =
      if let cancellation { try cancellation.withPointer { try call($0) } } else { try call(nil) }
    guard let result else { throw Native.error(status, fallback: "graph query failed") }
    do { return try materializeSelection(result) } catch {
      vectorkit_graph_result_free(result)
      throw error
    }
  }
}

private func materializeSelection(_ result: OpaquePointer) throws -> GraphSelection {
  var matches: [GraphMatch] = []
  for index in 0..<vectorkit_graph_result_count(result) {
    var value = VkGraphMatch(
      node_type: nil, source_type: 0, record_id: nil, chunk_key: nil, depth: 0, path_length: 0)
    try Native.bool { vectorkit_graph_result_match(result, index, &value, $0) }
    let node = graphNodeID(
      nodeType: value.node_type, recordID: value.record_id, chunkKey: value.chunk_key)
    let depth = value.depth
    let pathLength = value.path_length
    vectorkit_graph_match_clear(&value)
    var path: [GraphPathEdge] = []
    for edgeIndex in 0..<pathLength {
      var edge = emptyPathEdge()
      try Native.bool { vectorkit_graph_result_path_edge(result, index, edgeIndex, &edge, $0) }
      let sourceField =
        edge.source_field_segments.count == 0
        ? nil
        : GraphFieldPath(
          (0..<edge.source_field_segments.count).map {
            String(cString: edge.source_field_segments.values[$0]!)
          })
      path.append(
        GraphPathEdge(
          relationship: String(cString: edge.relationship_type), source: graphNodeID(edge.source),
          target: graphNodeID(edge.target), occurrenceOrdinal: edge.occurrence_ordinal,
          provenance: GraphEdgeProvenance(
            schemaRuleIndex: edge.schema_rule_index,
            sourceRecordID: String(cString: edge.source_record_id), sourceField: sourceField,
            derivedInverse: edge.derived_inverse, builtIn: edge.built_in)))
      vectorkit_graph_path_edge_clear(&edge)
    }
    matches.append(GraphMatch(nodeID: node, depth: depth, path: path))
  }
  let trace = vectorkit_graph_result_trace(result)
  return GraphSelection(
    matches: matches,
    trace: GraphQueryTrace(
      seedCount: trace.seed_count, visitedStates: trace.visited_states,
      traversedEdges: trace.traversed_edges, resultCount: trace.result_count,
      diagnostics: trace.diagnostics,
      truncationReason: GraphTruncationReason(rawValue: trace.truncation_reason)),
    native: NativeSelection(result))
}

private func materializeCandidateProjection(
  owner: NativeCapabilityDatabase, selection: GraphSelection, filter: GraphFilter?
) throws -> GraphCandidateProjection {
  let nativeFilter = try filter?.makeFFI()
  var output = VkGraphCandidateProjection(
    candidates: nil, count: 0, source_nodes: 0, projected_chunks_before_filter: 0,
    projected_chunks_after_filter: 0)
  var status = VkStatus(code: 0, message: nil)
  defer {
    vectorkit_graph_candidate_projection_clear(&output)
    vectorkit_status_clear(&status)
  }
  let database = OpaquePointer(bitPattern: try owner.requireHandle())
  let result = try selection.native.requireHandle()
  let succeeded = switch owner.kind {
  case .graph:
    vectorkit_graph_database_project_candidates(
      database, result, nativeFilter?.pointer, &output, &status)
  case .graphRetrieval:
    vectorkit_graph_retrieval_database_project_candidates(
      database, result, nativeFilter?.pointer, &output, &status)
  }
  guard succeeded else { throw Native.error(status, fallback: "candidate projection failed") }
  var candidates: [GraphChunkIdentity] = []
  candidates.reserveCapacity(output.count)
  for index in 0..<output.count {
    let value = output.candidates[index]
    guard
      let recordID = RecordID(rawValue: String(cString: value.record_id)),
      let chunkKey = ChunkKey(rawValue: String(cString: value.chunk_key))
    else {
      throw VectorKitGraphError.internalError("native projection returned an invalid identity")
    }
    candidates.append(GraphChunkIdentity(recordID: recordID, chunkKey: chunkKey))
  }
  return GraphCandidateProjection(
    candidates: candidates, sourceNodes: output.source_nodes,
    projectedChunksBeforeFilter: output.projected_chunks_before_filter,
    projectedChunksAfterFilter: output.projected_chunks_after_filter)
}

public actor GraphDatabase {
  public actor Builder {
    private var handle: UInt?
    public init(corpusID: CorpusID, schema: GraphSchema) throws {
      let json = String(decoding: try JSONEncoder().encode(schema), as: UTF8.self)
      handle = UInt(
        bitPattern: try Native.pointer { status in
          corpusID.rawValue.withCString { corpus in
            json.withCString { vectorkit_graph_database_builder_new(corpus, $0, status) }
          }
        })
    }
    deinit {
      if let handle { vectorkit_graph_database_builder_free(OpaquePointer(bitPattern: handle)) }
    }
    public func upsert(_ input: RecordInput) throws {
      guard let handle else { throw VectorKitGraphError.consumedBuilder }
      let batch = CapabilityGraphBatch(
        record: input.record, projectedMetadata: input.record.metadata, chunks: input.chunks)
      let json = String(decoding: try JSONEncoder().encode(batch), as: UTF8.self)
      try Native.bool { status in
        json.withCString {
          vectorkit_graph_database_builder_upsert_record_json(
            OpaquePointer(bitPattern: handle), $0, status)
        }
      }
    }
    public func build() throws -> GraphDatabase {
      guard let owned = handle else { throw VectorKitGraphError.consumedBuilder }
      handle = nil
      return GraphDatabase(
        pointer: try Native.pointer {
          vectorkit_graph_database_builder_build(OpaquePointer(bitPattern: owned), $0)
        })
    }
  }
  private let owner: NativeCapabilityDatabase
  public nonisolated let graph: GraphQueries
  private init(pointer: OpaquePointer) {
    let owner = NativeCapabilityDatabase(kind: .graph, handle: pointer)
    self.owner = owner
    graph = GraphQueries(owner: owner)
  }
  public func close() { owner.close() }
  public func projectCandidates(
    from selection: GraphSelection, filter: GraphFilter? = nil
  ) async throws -> GraphCandidateProjection {
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      try materializeCandidateProjection(owner: owner, selection: selection, filter: filter)
    }.value
  }
  public func save(to directory: URL) async throws {
    let handle = try owner.requireHandle()
    try await Task.detached {
      var status = VkStatus(code: 0, message: nil)
      defer { vectorkit_status_clear(&status) }
      guard
        directory.path.withCString({
          vectorkit_graph_database_save(OpaquePointer(bitPattern: handle), $0, &status)
        })
      else { throw Native.error(status, fallback: "graph save failed") }
    }.value
  }
  public static func load(from directory: URL) throws -> GraphDatabase {
    GraphDatabase(
      pointer: try Native.pointer { status in
        directory.path.withCString { vectorkit_graph_database_load($0, status) }
      })
  }
  public static func validate(at directory: URL) throws {
    try Native.bool { status in
      directory.path.withCString { vectorkit_graph_database_validate($0, status) }
    }
  }
}

public actor GraphRetrievalQueries {
  private let owner: NativeCapabilityDatabase
  fileprivate init(owner: NativeCapabilityDatabase) { self.owner = owner }
  public func semanticSearch(
    embedding: [Float], topK: Int = 10, within selection: GraphSelection? = nil,
    filter: GraphFilter? = nil
  ) async throws -> [GraphSearchHit] {
    let owner = owner
    return try await Task.detached {
      let values = embedding
      let nativeFilter = try filter?.makeFFI()
      var output = VkSearchResultBuffer(hits: nil, count: 0)
      var status = VkStatus(code: 0, message: nil)
      defer { vectorkit_status_clear(&status) }
      let succeeded = try values.withUnsafeBufferPointer { buffer in
        vectorkit_graph_retrieval_semantic_search(
          OpaquePointer(bitPattern: try owner.requireHandle()),
          try selection?.native.requireHandle(), buffer.baseAddress, buffer.count, topK,
          nativeFilter?.pointer, &output, &status)
      }
      guard succeeded else { throw Native.error(status, fallback: "semantic search failed") }
      defer { vectorkit_search_results_free(output) }
      return (0..<output.count).map {
        let hit = output.hits[$0]
        return GraphSearchHit(
          chunkID: hit.chunk_id, recordID: String(cString: hit.document_id),
          text: String(cString: hit.text), score: hit.score, vectorScore: hit.vector_score,
          filterMatched: hit.filter_matched)
      }
    }.value
  }
  public func hybridSearch(
    text: String, embedding: [Float], topK: Int = 10, within selection: GraphSelection? = nil,
    filter: GraphFilter? = nil, alpha: Float = 0.6, options: GraphHybridOptions = .default
  ) async throws -> [GraphHybridHit] {
    let owner = owner
    return try await Task.detached {
      let values = embedding
      let arena = GraphCStringArena()
      let nativeFilter = try filter?.makeFFI()
      var output = VkHybridResultBuffer(hits: nil, count: 0)
      var status = VkStatus(code: 0, message: nil)
      defer { vectorkit_status_clear(&status) }
      let succeeded = try values.withUnsafeBufferPointer { buffer in
        vectorkit_graph_retrieval_hybrid_search(
          OpaquePointer(bitPattern: try owner.requireHandle()),
          try selection?.native.requireHandle(), arena.copy(text), buffer.baseAddress, buffer.count,
          topK, nativeFilter?.pointer, options.ffiValue(alpha: alpha), &output, &status)
      }
      guard succeeded else { throw Native.error(status, fallback: "hybrid search failed") }
      defer { vectorkit_hybrid_results_free(output) }
      return (0..<output.count).map {
        let hit = output.hits[$0]
        let terms = (0..<hit.matched_terms.count).map {
          String(cString: hit.matched_terms.values[$0]!)
        }
        return GraphHybridHit(
          chunkID: hit.chunk_id, recordID: String(cString: hit.document_id),
          text: String(cString: hit.text), score: hit.score,
          vectorScore: hit.has_vector_score ? hit.vector_score : nil,
          keywordScore: hit.has_keyword_score ? hit.keyword_score : nil,
          trace: GraphHybridTrace(
            vectorRank: hit.has_vector_rank ? hit.vector_rank : nil,
            keywordRank: hit.has_keyword_rank ? hit.keyword_rank : nil,
            normalizedVectorScore: hit.has_normalized_vector_score
              ? hit.normalized_vector_score : nil,
            normalizedKeywordScore: hit.has_normalized_keyword_score
              ? hit.normalized_keyword_score : nil, matchedTerms: terms,
            filterMatched: hit.filter_matched))
      }
    }.value
  }
}

public actor GraphRetrievalDatabase {
  public actor Builder {
    private var handle: UInt?
    public init(corpusID: CorpusID, graph schema: GraphSchema, retrieval: RetrievalConfiguration)
      throws
    {
      let json = String(decoding: try JSONEncoder().encode(schema), as: UTF8.self)
      let vector = retrieval.semantic
      handle = UInt(
        bitPattern: try Native.pointer { status in
          corpusID.rawValue.withCString { corpus in
            json.withCString {
              vectorkit_graph_retrieval_builder_new(
                vector.dimension, vector.metric.ffi, vector.encoding.ffi, corpus,
                $0, status)
            }
          }
        })
    }
    deinit {
      if let handle { vectorkit_graph_retrieval_builder_free(OpaquePointer(bitPattern: handle)) }
    }
    public func upsert(_ input: RecordInput, embeddings: [ChunkKey: [Float]]) throws {
      guard let handle else { throw VectorKitGraphError.consumedBuilder }
      let expected = Set(input.chunks.map(\.key))
      let actual = Set(embeddings.keys)
      guard expected == actual else {
        let missing = expected.subtracting(actual).map(\.rawValue).sorted()
        if !missing.isEmpty {
          throw VectorKitGraphError.missingEmbedding(
            "missing embeddings for: \(missing.joined(separator: ", "))")
        }
        throw VectorKitGraphError.invalidIdentity("embeddings contain unknown chunk keys")
      }
      let batch = CapabilityRetrievalBatch(
        record: input.record, projectedMetadata: input.record.metadata,
        chunks: input.chunks.map {
          CapabilityRetrievalChunk(
            key: $0.key, text: $0.text, embedding: embeddings[$0.key]!, metadata: $0.metadata)
        })
      let json = String(decoding: try JSONEncoder().encode(batch), as: UTF8.self)
      try Native.bool { status in
        json.withCString {
          vectorkit_graph_retrieval_builder_upsert_record_json(
            OpaquePointer(bitPattern: handle), $0, status)
        }
      }
    }
    public func build() throws -> GraphRetrievalDatabase {
      guard let owned = handle else { throw VectorKitGraphError.consumedBuilder }
      handle = nil
      return GraphRetrievalDatabase(
        pointer: try Native.pointer {
          vectorkit_graph_retrieval_builder_build(OpaquePointer(bitPattern: owned), $0)
        })
    }
  }
  private let owner: NativeCapabilityDatabase
  public nonisolated let graph: GraphQueries
  public nonisolated let retrieval: GraphRetrievalQueries
  private init(pointer: OpaquePointer) {
    let owner = NativeCapabilityDatabase(kind: .graphRetrieval, handle: pointer)
    self.owner = owner
    graph = GraphQueries(owner: owner)
    retrieval = GraphRetrievalQueries(owner: owner)
  }
  public func close() { owner.close() }
  public func projectCandidates(
    from selection: GraphSelection, filter: GraphFilter? = nil
  ) async throws -> GraphCandidateProjection {
    let owner = owner
    return try await Task.detached(priority: Task.currentPriority) {
      try materializeCandidateProjection(owner: owner, selection: selection, filter: filter)
    }.value
  }
  public func save(to directory: URL) async throws {
    let handle = try owner.requireHandle()
    try await Task.detached {
      var status = VkStatus(code: 0, message: nil)
      defer { vectorkit_status_clear(&status) }
      guard
        directory.path.withCString({
          vectorkit_graph_retrieval_database_save(OpaquePointer(bitPattern: handle), $0, &status)
        })
      else { throw Native.error(status, fallback: "graph retrieval save failed") }
    }.value
  }
  public static func load(from directory: URL) throws -> GraphRetrievalDatabase {
    GraphRetrievalDatabase(
      pointer: try Native.pointer { status in
        directory.path.withCString { vectorkit_graph_retrieval_database_load($0, status) }
      })
  }
  public static func validate(at directory: URL) throws {
    try Native.bool { status in
      directory.path.withCString { vectorkit_graph_retrieval_database_validate($0, status) }
    }
  }
}
