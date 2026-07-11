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
}
