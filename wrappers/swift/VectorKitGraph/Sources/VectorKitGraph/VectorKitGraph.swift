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

public enum VectorKitGraphError: Error, Equatable { case native(code: Int32, message: String), consumedBuilder }

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
public struct GraphMatch: Equatable, Sendable { public let nodeID: GraphNodeID; public let depth, pathLength: Int }
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
    static func pointer(_ body: (UnsafeMutablePointer<VkStatus>) -> OpaquePointer?) throws -> OpaquePointer {
        var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard let result = body(&status) else { throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "unknown graph error") }
        return result
    }
    static func bool(_ body: (UnsafeMutablePointer<VkStatus>) -> Bool) throws {
        var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard body(&status) else { throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "unknown graph error") }
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
                let query = VkGraphQuery(seed_type: 0, node_ids: nativeSeeds.isEmpty ? nil : seedPointer, node_id_count: nativeSeeds.count, seed_node_type: nil, field_segments: nil, field_segment_count: 0, values: nil, value_count: 0, steps: nativeSteps.isEmpty ? nil : stepPointer, step_count: nativeSteps.count, limits: VkGraphLimits(max_hops: limits.maxHops, max_visited: limits.maxVisited, max_results: limits.maxResults, max_working_bytes: limits.maxWorkingBytes))
                var status = VkStatus(code: 0, message: nil)
                guard let result = vectorkit_graph_query(OpaquePointer(bitPattern: handle), query, cancellation?.pointer, &status) else {
                    defer { vectorkit_status_clear(&status) }
                    throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "unknown graph error")
                }
                vectorkit_status_clear(&status)
                guard let scope = vectorkit_graph_result_project(OpaquePointer(bitPattern: handle), result, &status) else {
                    defer { vectorkit_status_clear(&status); vectorkit_graph_result_free(result) }
                    throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "graph projection failed")
                }
                let execution = NativeGraphExecution(result: result, scope: scope)
                var matches: [GraphMatch] = []
                for index in 0..<vectorkit_graph_result_count(result) {
                    var value = VkGraphMatch(node_type: nil, source_type: 0, record_id: nil, chunk_key: nil, depth: 0, path_length: 0)
                    try Native.bool { status in vectorkit_graph_result_match(result, index, &value, status) }
                    defer { vectorkit_graph_match_clear(&value) }
                    matches.append(GraphMatch(nodeID: GraphNodeID(nodeType: String(cString: value.node_type), recordID: String(cString: value.record_id), chunkKey: value.chunk_key.map { String(cString: $0) }), depth: value.depth, pathLength: value.path_length))
                }
                let trace = vectorkit_graph_result_trace(result)
                return GraphQueryResult(matches: matches, trace: GraphQueryTrace(seedCount: trace.seed_count, visitedStates: trace.visited_states, traversedEdges: trace.traversed_edges, resultCount: trace.result_count, diagnostics: trace.diagnostics, truncationReason: trace.truncation_reason), native: execution)
    }

    public func search(_ embedding: [Float], topK: Int, in result: GraphQueryResult) throws -> [GraphSearchHit] {
        let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count)); for (index, value) in embedding.enumerated() { pointer.advanced(by: index).initialize(to: value) }; defer { pointer.deinitialize(count: embedding.count); pointer.deallocate() }
        var output = VkSearchResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        guard vectorkit_graph_scope_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), embedding.isEmpty ? nil : pointer, embedding.count, topK, nil, &output, &status) else { throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "scoped search failed") }
        defer { vectorkit_search_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; return GraphSearchHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score) }
    }

    public func keywordSearch(_ text: String, topK: Int, in result: GraphQueryResult) throws -> [GraphKeywordHit] {
        var output = VkKeywordResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        let arena = GraphCStringArena()
        let ok = vectorkit_graph_scope_keyword_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), arena.copy(text), topK, nil, &output, &status)
        guard ok else { throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "scoped keyword search failed") }
        defer { vectorkit_keyword_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; let terms = (0..<hit.matched_terms.count).map { String(cString: hit.matched_terms.values[$0]!) }; return GraphKeywordHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score, matchedTerms: terms) }
    }

    public func hybridSearch(text: String, embedding: [Float], topK: Int, in result: GraphQueryResult) throws -> [GraphHybridHit] {
        let pointer = UnsafeMutablePointer<Float>.allocate(capacity: max(1, embedding.count)); for (index, value) in embedding.enumerated() { pointer.advanced(by: index).initialize(to: value) }; defer { pointer.deinitialize(count: embedding.count); pointer.deallocate() }
        let arena = GraphCStringArena(); var output = VkHybridResultBuffer(hits: nil, count: 0); var status = VkStatus(code: 0, message: nil); defer { vectorkit_status_clear(&status) }
        let options = VkHybridOptions(vector_top_k: 50, keyword_top_k: 50, fusion_type: 1, vector_weight: 0.5, keyword_weight: 0.5, rrf_k: 60)
        guard vectorkit_graph_scope_hybrid_search(OpaquePointer(bitPattern: handle), OpaquePointer(bitPattern: result.native.scope), arena.copy(text), embedding.isEmpty ? nil : pointer, embedding.count, topK, nil, options, &output, &status) else { throw VectorKitGraphError.native(code: status.code, message: status.message.map { String(cString: $0) } ?? "scoped hybrid search failed") }
        defer { vectorkit_hybrid_results_free(output) }
        return (0..<output.count).map { index in let hit = output.hits[index]; let terms = (0..<hit.matched_terms.count).map { String(cString: hit.matched_terms.values[$0]!) }; return GraphHybridHit(chunkID: hit.chunk_id, recordID: String(cString: hit.document_id), text: String(cString: hit.text), score: hit.score, vectorScore: hit.has_vector_score ? hit.vector_score : nil, keywordScore: hit.has_keyword_score ? hit.keyword_score : nil, matchedTerms: terms) }
    }
}
