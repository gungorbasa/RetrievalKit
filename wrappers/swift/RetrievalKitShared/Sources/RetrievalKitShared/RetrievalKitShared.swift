import Foundation

public protocol RetrievalKitStringIdentifier: RawRepresentable, Hashable, Codable,
  Sendable, ExpressibleByStringLiteral
where RawValue == String {}

extension RetrievalKitStringIdentifier {
  public init(stringLiteral value: String) { self.init(rawValue: value)! }
  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    self.init(rawValue: try container.decode(String.self))!
  }
  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(rawValue)
  }
}

public struct CorpusID: RetrievalKitStringIdentifier {
  public let rawValue: String
  public init?(rawValue: String) { self.rawValue = rawValue }
  public init(_ rawValue: String) { self.rawValue = rawValue }
}
public struct RecordID: RetrievalKitStringIdentifier {
  public let rawValue: String
  public init?(rawValue: String) { self.rawValue = rawValue }
  public init(_ rawValue: String) { self.rawValue = rawValue }
}
public struct DocumentID: RetrievalKitStringIdentifier {
  public let rawValue: String
  public init?(rawValue: String) { self.rawValue = rawValue }
  public init(_ rawValue: String) { self.rawValue = rawValue }
}
public struct RecordType: RetrievalKitStringIdentifier {
  public let rawValue: String
  public init?(rawValue: String) { self.rawValue = rawValue }
  public init(_ rawValue: String) { self.rawValue = rawValue }
}
public struct ChunkKey: RetrievalKitStringIdentifier {
  public let rawValue: String
  public init?(rawValue: String) { self.rawValue = rawValue }
  public init(_ rawValue: String) { self.rawValue = rawValue }
}

private struct DynamicKey: CodingKey {
  let stringValue: String
  let intValue: Int? = nil
  init?(stringValue: String) { self.stringValue = stringValue }
  init?(intValue: Int) { return nil }
}
private func tagged<T: Encodable>(_ key: String, _ value: T, _ encoder: Encoder) throws {
  var c = encoder.container(keyedBy: DynamicKey.self)
  try c.encode(value, forKey: DynamicKey(stringValue: key)!)
}

public enum RecordValue: Equatable, Sendable, Codable {
  case null
  case bool(Bool)
  case int(Int64)
  case double(Double)
  case string(String)
  case list([RecordValue])
  case map([String: RecordValue])
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
    if let s = try? decoder.singleValueContainer(), let tag = try? s.decode(String.self),
      tag == "Null"
    {
      self = .null
      return
    }
    let c = try decoder.container(keyedBy: DynamicKey.self)
    if let k = DynamicKey(stringValue: "Bool"), c.contains(k) {
      self = .bool(try c.decode(Bool.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "I64"), c.contains(k) {
      self = .int(try c.decode(Int64.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "F64"), c.contains(k) {
      self = .double(try c.decode(Double.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "String"), c.contains(k) {
      self = .string(try c.decode(String.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "List"), c.contains(k) {
      self = .list(try c.decode([RecordValue].self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "Map"), c.contains(k) {
      self = .map(try c.decode([String: RecordValue].self, forKey: k))
      return
    }
    throw DecodingError.dataCorrupted(
      .init(codingPath: decoder.codingPath, debugDescription: "unsupported record value tag"))
  }
}

public enum MetadataValue: Equatable, Sendable, Codable {
  case string(String)
  case integer(Int64)
  case float(Double)
  case boolean(Bool)
  case timestampMillis(Int64)
  public func encode(to encoder: Encoder) throws {
    switch self {
    case .string(let v): try tagged("String", v, encoder)
    case .integer(let v): try tagged("Integer", v, encoder)
    case .float(let v): try tagged("Float", v, encoder)
    case .boolean(let v): try tagged("Boolean", v, encoder)
    case .timestampMillis(let v): try tagged("TimestampMillis", v, encoder)
    }
  }
  public init(from decoder: Decoder) throws {
    let c = try decoder.container(keyedBy: DynamicKey.self)
    if let k = DynamicKey(stringValue: "String"), c.contains(k) {
      self = .string(try c.decode(String.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "Integer"), c.contains(k) {
      self = .integer(try c.decode(Int64.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "Float"), c.contains(k) {
      self = .float(try c.decode(Double.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "Boolean"), c.contains(k) {
      self = .boolean(try c.decode(Bool.self, forKey: k))
      return
    }
    if let k = DynamicKey(stringValue: "TimestampMillis"), c.contains(k) {
      self = .timestampMillis(try c.decode(Int64.self, forKey: k))
      return
    }
    throw DecodingError.dataCorrupted(
      .init(codingPath: decoder.codingPath, debugDescription: "unsupported metadata value tag"))
  }
}

public struct Record: Equatable, Sendable, Codable {
  public var id: RecordID
  public var type: RecordType
  public var fields: [String: RecordValue]
  public var metadata: [String: MetadataValue]
  public var content: String?
  public init(
    id: RecordID, type: RecordType, fields: [String: RecordValue] = [:],
    metadata: [String: MetadataValue] = [:], content: String? = nil
  ) {
    self.id = id
    self.type = type
    self.fields = fields
    self.metadata = metadata
    self.content = content
  }
  enum CodingKeys: String, CodingKey {
    case id
    case type = "record_type"
    case fields, metadata, content
  }
}

/// A caller-owned unit of searchable text.
///
/// RetrievalKit stores the caller-produced embedding separately so applications
/// can bring any embedding model, including EmbeddingKit.
public struct Document: Equatable, Sendable, Codable {
  public var id: DocumentID
  public var text: String
  public var metadata: [String: MetadataValue]

  public init(
    id: DocumentID,
    text: String = "",
    metadata: [String: MetadataValue] = [:]
  ) {
    self.id = id
    self.text = text
    self.metadata = metadata
  }
}

/// A searchable document paired with its caller-produced embedding.
///
/// This is the advanced combined-ingestion value for records that own more
/// than one independently identifiable searchable document.
public struct EmbeddedDocument: Equatable, Sendable {
  public var document: Document
  public var embedding: [Float]

  public init(
    id: DocumentID,
    text: String,
    embedding: [Float],
    metadata: [String: MetadataValue] = [:]
  ) {
    self.document = Document(id: id, text: text, metadata: metadata)
    self.embedding = embedding
  }

  public init(document: Document, embedding: [Float]) {
    self.document = document
    self.embedding = embedding
  }

  public var id: DocumentID { document.id }
  public var text: String { document.text }
  public var metadata: [String: MetadataValue] { document.metadata }
}

public struct Chunk: Equatable, Sendable, Codable {
  public var key: ChunkKey
  public var text: String
  public var metadata: [String: MetadataValue]
  public init(key: ChunkKey, text: String, metadata: [String: MetadataValue] = [:]) {
    self.key = key
    self.text = text
    self.metadata = metadata
  }
}
public struct RecordInput: Equatable, Sendable, Codable {
  public var record: Record
  public var chunks: [Chunk]
  public init(record: Record, chunks: [Chunk] = []) {
    self.record = record
    self.chunks = chunks
  }
}
