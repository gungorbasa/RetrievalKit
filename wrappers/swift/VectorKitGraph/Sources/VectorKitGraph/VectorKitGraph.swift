import Foundation
import VectorKitGraphFFI

public enum GraphMetric: Sendable { case cosine, dotProduct
    var ffi: UInt32 { self == .cosine ? 0 : 1 }
}

public enum GraphVectorEncoding: Sendable { case f32, f16, i8ScalarQuantized
    var ffi: UInt32 { switch self { case .f32: 0; case .f16: 1; case .i8ScalarQuantized: 3 } }
}

public enum GraphValue: Equatable, Sendable, Encodable {
    case null, bool(Bool), int(Int64), double(Double), string(String)
    case list([GraphValue]), map([String: GraphValue])

    public func encode(to encoder: Encoder) throws {
        switch self {
        case .null: var c = encoder.singleValueContainer(); try c.encode("Null")
        case .bool(let v): try tagged("Bool", v, encoder)
        case .int(let v): try tagged("I64", v, encoder)
        case .double(let v): try tagged("F64", v, encoder)
        case .string(let v): try tagged("String", v, encoder)
        case .list(let v): try tagged("List", v, encoder)
        case .map(let v): try tagged("Map", v, encoder)
        }
    }
}

public enum GraphMetadataValue: Equatable, Sendable, Encodable {
    case string(String), integer(Int64), double(Double), boolean(Bool), timestampMillis(Int64)
    public func encode(to encoder: Encoder) throws {
        switch self {
        case .string(let v): try tagged("String", v, encoder)
        case .integer(let v): try tagged("Integer", v, encoder)
        case .double(let v): try tagged("Float", v, encoder)
        case .boolean(let v): try tagged("Boolean", v, encoder)
        case .timestampMillis(let v): try tagged("TimestampMillis", v, encoder)
        }
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

public struct GraphRecord: Equatable, Sendable, Encodable {
    public var id: String; public var recordType: String
    public var fields: [String: GraphValue]; public var content: String?
    public init(id: String, recordType: String, fields: [String: GraphValue] = [:], content: String? = nil) {
        self.id = id; self.recordType = recordType; self.fields = fields; self.content = content
    }
    enum CodingKeys: String, CodingKey { case id, recordType = "record_type", fields, content }
}

public struct GraphChunk: Equatable, Sendable, Encodable {
    public var key: String; public var text: String; public var embedding: [Float]
    public var metadata: [String: GraphMetadataValue]
    public init(key: String, text: String, embedding: [Float], metadata: [String: GraphMetadataValue] = [:]) {
        self.key = key; self.text = text; self.embedding = embedding; self.metadata = metadata
    }
}

public struct GraphRecordBatch: Equatable, Sendable, Encodable {
    public var record: GraphRecord; public var projectedMetadata: [String: GraphMetadataValue]; public var chunks: [GraphChunk]
    public init(record: GraphRecord, projectedMetadata: [String: GraphMetadataValue] = [:], chunks: [GraphChunk]) {
        self.record = record; self.projectedMetadata = projectedMetadata; self.chunks = chunks
    }
    enum CodingKeys: String, CodingKey { case record, projectedMetadata = "projected_metadata", chunks }
}

public struct GraphFieldPath: Equatable, Hashable, Sendable, Encodable {
    public var segments: [String]
    public init(_ segments: [String]) { self.segments = segments }
    public init(_ field: String) { segments = [field] }
    public func encode(to encoder: Encoder) throws { var c = encoder.singleValueContainer(); try c.encode(segments) }
}

public enum GraphCardinality: String, Sendable, Encodable { case one = "One", optionalOne = "OptionalOne", many = "Many" }
public enum GraphMissingTargetPolicy: String, Sendable, Encodable { case error = "Error", omitEdge = "OmitEdge" }
public enum GraphDuplicatePolicy: String, Sendable, Encodable { case error = "Error", deduplicate = "Deduplicate" }

public struct GraphRecordNodeSchema: Equatable, Sendable, Encodable {
    public var recordType: String; public var nodeType: String; public var queryableFields: [GraphFieldPath]
    public init(recordType: String, nodeType: String, queryableFields: [GraphFieldPath] = []) {
        self.recordType = recordType; self.nodeType = nodeType; self.queryableFields = queryableFields
    }
    enum CodingKeys: String, CodingKey { case recordType = "record_type", nodeType = "node_type", queryableFields = "queryable_fields" }
}

public struct GraphRelationshipSchema: Equatable, Sendable, Encodable {
    public var relationshipType, sourceNodeType, targetNodeType: String
    public var sourceField: GraphFieldPath; public var cardinality: GraphCardinality
    public var missingTarget: GraphMissingTargetPolicy; public var duplicateReferences: GraphDuplicatePolicy
    public var allowSelfEdge: Bool; public var inverseRelationship: String?
    public init(relationshipType: String, sourceNodeType: String, targetNodeType: String, sourceField: GraphFieldPath, cardinality: GraphCardinality, missingTarget: GraphMissingTargetPolicy = .error, duplicateReferences: GraphDuplicatePolicy = .error, allowSelfEdge: Bool = false, inverseRelationship: String? = nil) {
        self.relationshipType = relationshipType; self.sourceNodeType = sourceNodeType; self.targetNodeType = targetNodeType; self.sourceField = sourceField; self.cardinality = cardinality; self.missingTarget = missingTarget; self.duplicateReferences = duplicateReferences; self.allowSelfEdge = allowSelfEdge; self.inverseRelationship = inverseRelationship
    }
    enum CodingKeys: String, CodingKey { case relationshipType = "relationship_type", sourceNodeType = "source_node_type", targetNodeType = "target_node_type", sourceField = "source_field", cardinality, missingTarget = "missing_target", duplicateReferences = "duplicate_references", allowSelfEdge = "allow_self_edge", inverseRelationship = "inverse_relationship" }
}

public struct GraphChunkNodeSchema: Equatable, Sendable, Encodable {
    public var nodeType, ownsRelationship: String; public var inverseRelationship: String?
    public init(nodeType: String, ownsRelationship: String, inverseRelationship: String? = nil) { self.nodeType = nodeType; self.ownsRelationship = ownsRelationship; self.inverseRelationship = inverseRelationship }
    enum CodingKeys: String, CodingKey { case nodeType = "node_type", ownsRelationship = "owns_relationship", inverseRelationship = "inverse_relationship" }
}

public struct GraphSchema: Equatable, Sendable, Encodable {
    public var version: UInt32 = 1; public var recordNodes: [GraphRecordNodeSchema]
    public var relationships: [GraphRelationshipSchema]; public var chunkNodes: GraphChunkNodeSchema?
    public init(recordNodes: [GraphRecordNodeSchema], relationships: [GraphRelationshipSchema] = [], chunkNodes: GraphChunkNodeSchema? = nil) { self.recordNodes = recordNodes; self.relationships = relationships; self.chunkNodes = chunkNodes }
    enum CodingKeys: String, CodingKey { case version, recordNodes = "record_nodes", relationships, chunkNodes = "chunk_nodes" }
}

public enum VectorKitGraphError: Error, Equatable {
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
    case internalError(String)
    case consumedBuilder
}

extension VectorKitGraphError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidSchema(let message), .invalidIdentity(let message), .staleGeneration(let message), .incompatibleVersion(let message), .graphUnavailable(let message), .corruptSnapshot(let message), .queryLimitExceeded(let message), .cancelled(let message), .timedOut(let message), .lockUnavailable(let message), .internalError(let message): message
        case .consumedBuilder: "graph builder has already been consumed by build(schema:)"
        }
    }
}

public struct GraphNodeID: Equatable, Hashable, Sendable {
    public var nodeType: String; public var recordID: String; public var chunkKey: String?
    public init(nodeType: String, recordID: String, chunkKey: String? = nil) { self.nodeType = nodeType; self.recordID = recordID; self.chunkKey = chunkKey }
}
public enum GraphDirection: Sendable { case outgoing, incoming }
public struct GraphTraversal: Equatable, Sendable {
    public var relationship: String; public var direction: GraphDirection; public var minHops, maxHops: Int
    public init(relationship: String, direction: GraphDirection = .outgoing, minHops: Int = 1, maxHops: Int = 1) { self.relationship = relationship; self.direction = direction; self.minHops = minHops; self.maxHops = maxHops }
}
public struct GraphQueryLimits: Equatable, Sendable {
    public var maxHops = 8, maxVisited = 100_000, maxResults = 10_000, maxWorkingBytes = 64 * 1024 * 1024
    public init(maxHops: Int = 8, maxVisited: Int = 100_000, maxResults: Int = 10_000, maxWorkingBytes: Int = 64 * 1024 * 1024) { self.maxHops = maxHops; self.maxVisited = maxVisited; self.maxResults = maxResults; self.maxWorkingBytes = maxWorkingBytes }
}
public enum GraphScalar: Equatable, Sendable {
    case string(String), integer(Int64), boolean(Bool)
}
public struct GraphEdgeProvenance: Equatable, Sendable {
    public let schemaRuleIndex: UInt32; public let sourceRecordID: String; public let sourceField: GraphFieldPath?
    public let derivedInverse, builtIn: Bool
    public init(schemaRuleIndex: UInt32, sourceRecordID: String, sourceField: GraphFieldPath?, derivedInverse: Bool, builtIn: Bool) {
        self.schemaRuleIndex = schemaRuleIndex; self.sourceRecordID = sourceRecordID; self.sourceField = sourceField; self.derivedInverse = derivedInverse; self.builtIn = builtIn
    }
}
public struct GraphPathEdge: Equatable, Sendable {
    public let relationship: String; public let source, target: GraphNodeID; public let occurrenceOrdinal: UInt32
    public let provenance: GraphEdgeProvenance
    public init(relationship: String, source: GraphNodeID, target: GraphNodeID, occurrenceOrdinal: UInt32, provenance: GraphEdgeProvenance) {
        self.relationship = relationship; self.source = source; self.target = target; self.occurrenceOrdinal = occurrenceOrdinal; self.provenance = provenance
    }
}
public struct GraphMatch: Equatable, Sendable {
    public let nodeID: GraphNodeID; public let depth: Int; public let path: [GraphPathEdge]
    public var pathLength: Int { path.count }
    public init(nodeID: GraphNodeID, depth: Int, path: [GraphPathEdge] = []) { self.nodeID = nodeID; self.depth = depth; self.path = path }
}
public struct GraphQueryTrace: Equatable, Sendable { public let seedCount, visitedStates, traversedEdges, resultCount, diagnostics: Int; public let truncationReason: UInt32 }
private final class NativeGraphExecution: @unchecked Sendable {
    let result, scope: UInt
    init(result: OpaquePointer, scope: OpaquePointer) { self.result = UInt(bitPattern: result); self.scope = UInt(bitPattern: scope) }
    deinit { vectorkit_graph_scope_free(OpaquePointer(bitPattern: scope)); vectorkit_graph_result_free(OpaquePointer(bitPattern: result)) }
}
public struct GraphQueryResult: Sendable {
    public let matches: [GraphMatch]; public let trace: GraphQueryTrace
    fileprivate let native: NativeGraphExecution
}
extension GraphQueryResult: Equatable { public static func == (left: Self, right: Self) -> Bool { left.matches == right.matches && left.trace == right.trace } }
public struct GraphSearchHit: Equatable, Sendable { public let chunkID: UInt64; public let recordID, text: String; public let score: Float }
public struct GraphKeywordHit: Equatable, Sendable { public let chunkID: UInt64; public let recordID, text: String; public let score: Float; public let matchedTerms: [String] }
public struct GraphHybridHit: Equatable, Sendable { public let chunkID: UInt64; public let recordID, text: String; public let score: Float; public let vectorScore, keywordScore: Float?; public let matchedTerms: [String] }

public final class GraphCancellationToken: @unchecked Sendable {
    private let handle: UInt = UInt(bitPattern: vectorkit_graph_cancellation_new())
    public init() {}
    deinit { vectorkit_graph_cancellation_free(OpaquePointer(bitPattern: handle)) }
    public func cancel() { vectorkit_graph_cancellation_cancel(OpaquePointer(bitPattern: handle)) }
    fileprivate var pointer: OpaquePointer? { OpaquePointer(bitPattern: handle) }
}

private final class GraphCStringArena {
    private var values: [UnsafeMutablePointer<CChar>] = []
    deinit { values.forEach { $0.deallocate() } }
    func copy(_ value: String) -> UnsafePointer<CChar> {
        let bytes = Array(value.utf8CString); let pointer = UnsafeMutablePointer<CChar>.allocate(capacity: bytes.count)
        pointer.initialize(from: bytes, count: bytes.count); values.append(pointer); return UnsafePointer(pointer)
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
        case VK_GRAPH_STATUS_INTERNAL, VK_STATUS_CORE_ERROR: return .internalError(message)
        default: return .internalError(message)
        }
    }
    static func pointer(_ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?) throws -> OpaquePointer {
        var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard let result = body(&status) else { throw error(status, fallback: "unknown graph error") }
        return result
    }
    static func bool(_ body: (UnsafeMutablePointer<VkStatus>) -> Bool) throws {
        var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard body(&status) else { throw error(status, fallback: "unknown graph error") }
    }
}

public actor GraphIndexBuilder {
    private var handle: UInt?
    public init(dimension: Int, corpusID: String, metric: GraphMetric = .cosine, encoding: GraphVectorEncoding = .f32) throws {
        handle = UInt(bitPattern: try Native.pointer { status in corpusID.withCString { vectorkit_graph_builder_new(dimension, metric.ffi, encoding.ffi, $0, status) } })
    }
    deinit { if let handle { vectorkit_graph_builder_free(OpaquePointer(bitPattern: handle)) } }
    public func upsert(_ batch: GraphRecordBatch) throws {
        guard let handle else { throw VectorKitGraphError.consumedBuilder }
        let data = try JSONEncoder().encode(batch); let json = String(decoding: data, as: UTF8.self)
        try Native.bool { status in json.withCString { vectorkit_graph_builder_upsert_record_json(OpaquePointer(bitPattern: handle), $0, status) } }
    }
    public func build(schema: GraphSchema) throws -> GraphIndex {
        guard let owned = handle else { throw VectorKitGraphError.consumedBuilder }; handle = nil
        let data = try JSONEncoder().encode(schema); let json = String(decoding: data, as: UTF8.self)
        let graph = try Native.pointer { status in json.withCString { vectorkit_graph_builder_build_json(OpaquePointer(bitPattern: owned), $0, status) } }
        return GraphIndex(handle: UInt(bitPattern: graph))
    }
}

public actor GraphIndex {
    private let handle: UInt
    fileprivate init(handle: UInt) { self.handle = handle }
    deinit { vectorkit_graph_index_free(OpaquePointer(bitPattern: handle)) }
    public func save(to directory: URL) throws { try Native.bool { status in directory.path.withCString { vectorkit_graph_index_save(OpaquePointer(bitPattern: handle), $0, status) } } }
    public static func load(from directory: URL) throws -> GraphIndex { GraphIndex(handle: UInt(bitPattern: try Native.pointer { status in directory.path.withCString { vectorkit_graph_index_load($0, status) } })) }
    public static func validate(at directory: URL) throws { try Native.bool { status in directory.path.withCString { vectorkit_graph_index_validate($0, status) } } }

    public func query(from seeds: [GraphNodeID], traversing steps: [GraphTraversal] = [], limits: GraphQueryLimits = .init(), cancellation: GraphCancellationToken? = nil) throws -> GraphQueryResult {
        let arena = GraphCStringArena()
        let nativeSeeds = seeds.map { seed in VkGraphNodeRef(node_type: arena.copy(seed.nodeType), source_type: seed.chunkKey == nil ? 0 : 1, record_id: arena.copy(seed.recordID), chunk_key: seed.chunkKey.map(arena.copy)) }
        let nativeSteps = steps.map { step in VkGraphStep(relationship: arena.copy(step.relationship), direction: step.direction == .outgoing ? 0 : 1, min_hops: step.minHops, max_hops: step.maxHops) }
        let seedPointer = UnsafeMutablePointer<VkGraphNodeRef>.allocate(capacity: max(1, nativeSeeds.count))
        let stepPointer = UnsafeMutablePointer<VkGraphStep>.allocate(capacity: max(1, nativeSteps.count))
        for (index, value) in nativeSeeds.enumerated() { seedPointer.advanced(by: index).initialize(to: value) }
        for (index, value) in nativeSteps.enumerated() { stepPointer.advanced(by: index).initialize(to: value) }
        defer { seedPointer.deinitialize(count: nativeSeeds.count); seedPointer.deallocate(); stepPointer.deinitialize(count: nativeSteps.count); stepPointer.deallocate() }
        let query = VkGraphQuery(seed_type: 0, node_ids: nativeSeeds.isEmpty ? nil : seedPointer, node_id_count: nativeSeeds.count, seed_node_type: nil, field_segments: nil, field_segment_count: 0, values: nil, value_count: 0, steps: nativeSteps.isEmpty ? nil : stepPointer, step_count: nativeSteps.count, limits: nativeLimits(limits))
        return try execute(query, cancellation: cancellation)
    }

    public func query(nodeType: String, field: GraphFieldPath, equals values: [GraphScalar], traversing steps: [GraphTraversal] = [], limits: GraphQueryLimits = .init(), cancellation: GraphCancellationToken? = nil) throws -> GraphQueryResult {
        let arena = GraphCStringArena()
        let nativeFields = field.segments.map(arena.copy)
        let nativeValues = values.map { value in
            switch value {
            case .string(let string): VkGraphScalar(value_type: 0, string_value: arena.copy(string), integer_value: 0, bool_value: false)
            case .integer(let integer): VkGraphScalar(value_type: 1, string_value: nil, integer_value: integer, bool_value: false)
            case .boolean(let boolean): VkGraphScalar(value_type: 2, string_value: nil, integer_value: 0, bool_value: boolean)
            }
        }
        let nativeSteps = steps.map { step in VkGraphStep(relationship: arena.copy(step.relationship), direction: step.direction == .outgoing ? 0 : 1, min_hops: step.minHops, max_hops: step.maxHops) }
        let fieldPointer = UnsafeMutablePointer<UnsafePointer<CChar>?>.allocate(capacity: max(1, nativeFields.count))
        let valuePointer = UnsafeMutablePointer<VkGraphScalar>.allocate(capacity: max(1, nativeValues.count))
        let stepPointer = UnsafeMutablePointer<VkGraphStep>.allocate(capacity: max(1, nativeSteps.count))
        for (index, value) in nativeFields.enumerated() { fieldPointer.advanced(by: index).initialize(to: value) }
        for (index, value) in nativeValues.enumerated() { valuePointer.advanced(by: index).initialize(to: value) }
        for (index, value) in nativeSteps.enumerated() { stepPointer.advanced(by: index).initialize(to: value) }
        defer {
            fieldPointer.deinitialize(count: nativeFields.count); fieldPointer.deallocate()
            valuePointer.deinitialize(count: nativeValues.count); valuePointer.deallocate()
            stepPointer.deinitialize(count: nativeSteps.count); stepPointer.deallocate()
        }
        let query = VkGraphQuery(seed_type: 1, node_ids: nil, node_id_count: 0, seed_node_type: arena.copy(nodeType), field_segments: nativeFields.isEmpty ? nil : fieldPointer, field_segment_count: nativeFields.count, values: nativeValues.isEmpty ? nil : valuePointer, value_count: nativeValues.count, steps: nativeSteps.isEmpty ? nil : stepPointer, step_count: nativeSteps.count, limits: nativeLimits(limits))
        return try execute(query, cancellation: cancellation)
    }

    private func execute(_ query: VkGraphQuery, cancellation: GraphCancellationToken?) throws -> GraphQueryResult {
        var status = VkStatus(code: 0, message: nil)
        guard let result = vectorkit_graph_query(OpaquePointer(bitPattern: handle), query, cancellation?.pointer, &status) else {
            defer { vectorkit_status_clear(&status) }
            throw Native.error(status, fallback: "unknown graph error")
        }
        vectorkit_status_clear(&status)
        guard let scope = vectorkit_graph_result_project(OpaquePointer(bitPattern: handle), result, &status) else {
            defer { vectorkit_status_clear(&status); vectorkit_graph_result_free(result) }
            throw Native.error(status, fallback: "graph projection failed")
        }
        let execution = NativeGraphExecution(result: result, scope: scope)
        var matches: [GraphMatch] = []
        for index in 0..<vectorkit_graph_result_count(result) {
            var value = VkGraphMatch(node_type: nil, source_type: 0, record_id: nil, chunk_key: nil, depth: 0, path_length: 0)
            try Native.bool { status in vectorkit_graph_result_match(result, index, &value, status) }
            let nodeID = graphNodeID(nodeType: value.node_type, recordID: value.record_id, chunkKey: value.chunk_key)
            let depth = value.depth; let pathLength = value.path_length
            vectorkit_graph_match_clear(&value)
            var path: [GraphPathEdge] = []
            for edgeIndex in 0..<pathLength {
                var edge = emptyPathEdge()
                try Native.bool { status in vectorkit_graph_result_path_edge(result, index, edgeIndex, &edge, status) }
                let sourceField = edge.source_field_segments.count == 0 ? nil : GraphFieldPath((0..<edge.source_field_segments.count).map { String(cString: edge.source_field_segments.values[$0]!) })
                path.append(GraphPathEdge(relationship: String(cString: edge.relationship_type), source: graphNodeID(edge.source), target: graphNodeID(edge.target), occurrenceOrdinal: edge.occurrence_ordinal, provenance: GraphEdgeProvenance(schemaRuleIndex: edge.schema_rule_index, sourceRecordID: String(cString: edge.source_record_id), sourceField: sourceField, derivedInverse: edge.derived_inverse, builtIn: edge.built_in)))
                vectorkit_graph_path_edge_clear(&edge)
            }
            matches.append(GraphMatch(nodeID: nodeID, depth: depth, path: path))
        }
        let trace = vectorkit_graph_result_trace(result)
        return GraphQueryResult(matches: matches, trace: GraphQueryTrace(seedCount: trace.seed_count, visitedStates: trace.visited_states, traversedEdges: trace.traversed_edges, resultCount: trace.result_count, diagnostics: trace.diagnostics, truncationReason: trace.truncation_reason), native: execution)
    }

    public func search(_ embedding: [Float], topK: Int, in result: GraphQueryResult) throws -> [GraphSearchHit] {
        let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count)); for (index, value) in embedding.enumerated() { pointer.advanced(by: index).initialize(to: value) }; defer { pointer.deinitialize(count: embedding.count); pointer.deallocate() }
        var output = VkSearchResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard vectorkit_graph_scope_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), embedding.isEmpty ? nil : pointer, embedding.count, topK, nil, &output, &status) else { throw Native.error(status, fallback: "scoped search failed") }
        defer { vectorkit_search_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; return GraphSearchHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score) }
    }

    public func keywordSearch(_ text: String, topK: Int, in result: GraphQueryResult) throws -> [GraphKeywordHit] {
        var output = VkKeywordResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        let arena = GraphCStringArena()
        let ok = vectorkit_graph_scope_keyword_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), arena.copy(text), topK, nil, &output, &status)
        guard ok else { throw Native.error(status, fallback: "scoped keyword search failed") }
        defer { vectorkit_keyword_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; let terms = (0..<hit.matched_terms.count).map { String(cString: hit.matched_terms.values[$0]!) }; return GraphKeywordHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score, matchedTerms: terms) }
    }

    public func hybridSearch(text: String, embedding: [Float], topK: Int, in result: GraphQueryResult) throws -> [GraphHybridHit] {
        let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count)); for (index, value) in embedding.enumerated() { pointer.advanced(by: index).initialize(to: value) }; defer { pointer.deinitialize(count: embedding.count); pointer.deallocate() }
        let arena = GraphCStringArena(); var output = VkHybridResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        let options = VkHybridOptions(vector_top_k: 50, keyword_top_k: 50, fusion_type: 1, vector_weight: 0.5, keyword_weight: 0.5, rrf_k: 60)
        guard vectorkit_graph_scope_hybrid_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), arena.copy(text), embedding.isEmpty ? nil : pointer, embedding.count, topK, nil, options, &output, &status) else { throw Native.error(status, fallback: "scoped hybrid search failed") }
        defer { vectorkit_hybrid_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; let terms = (0..<hit.matched_terms.count).map { String(cString: hit.matched_terms.values[$0]!) }; return GraphHybridHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score, vectorScore: hit.has_vector_score ? hit.vector_score : nil, keywordScore: hit.has_keyword_score ? hit.keyword_score : nil, matchedTerms: terms) }
    }
}

private func nativeLimits(_ limits: GraphQueryLimits) -> VkGraphLimits {
    VkGraphLimits(max_hops: limits.maxHops, max_visited: limits.maxVisited, max_results: limits.maxResults, max_working_bytes: limits.maxWorkingBytes)
}

private func graphNodeID(nodeType: UnsafeMutablePointer<CChar>?, recordID: UnsafeMutablePointer<CChar>?, chunkKey: UnsafeMutablePointer<CChar>?) -> GraphNodeID {
    GraphNodeID(nodeType: String(cString: nodeType!), recordID: String(cString: recordID!), chunkKey: chunkKey.map { String(cString: $0) })
}

private func graphNodeID(_ value: VkGraphOwnedNode) -> GraphNodeID {
    graphNodeID(nodeType: value.node_type, recordID: value.record_id, chunkKey: value.chunk_key)
}

private func emptyOwnedNode() -> VkGraphOwnedNode {
    VkGraphOwnedNode(node_type: nil, source_type: 0, record_id: nil, chunk_key: nil)
}

private func emptyPathEdge() -> VkGraphPathEdge {
    VkGraphPathEdge(relationship_type: nil, source: emptyOwnedNode(), target: emptyOwnedNode(), occurrence_ordinal: 0, schema_rule_index: 0, source_record_id: nil, source_field_segments: VkStringArray(values: nil, count: 0), derived_inverse: false, built_in: false)
}
