import XCTest

@testable import RetrievalKitShared

final class RetrievalKitSharedTests: XCTestCase {
  func testIdentifiersSupportStringLiteralsWithoutLocalValidation() {
    let corpus: CorpusID = "knowledge"
    let record: RecordID = "rust"
    let chunk: ChunkKey = "summary"

    XCTAssertEqual(corpus.rawValue, "knowledge")
    XCTAssertEqual(record.rawValue, "rust")
    XCTAssertEqual(chunk.rawValue, "summary")
  }

  func testRecordInputRoundTripsThroughCanonicalJSONShape() throws {
    let input = RecordInput(
      record: Record(
        id: "rust",
        type: "Topic",
        fields: ["title": .string("Rust")],
        metadata: ["source": .string("docs")]
      ),
      chunks: [Chunk(key: "summary", text: "native retrieval")]
    )

    let encoded = try JSONEncoder().encode(input)
    XCTAssertEqual(try JSONDecoder().decode(RecordInput.self, from: encoded), input)

    let json = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    let record = try XCTUnwrap(json["record"] as? [String: Any])
    XCTAssertEqual(record["record_type"] as? String, "Topic")
  }
}
