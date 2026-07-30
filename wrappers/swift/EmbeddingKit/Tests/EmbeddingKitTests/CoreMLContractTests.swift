#if canImport(CoreML)
import Foundation
import XCTest
@testable import EmbeddingKit

final class CoreMLContractTests: XCTestCase {
    func testProductionShapeTokenizerUsesExactly256SlotsAndTruncates() throws {
        let tokenizer = try BertWordPieceTokenizer(
            tokenizerJSON: makeTokenizerJSON(),
            sequenceLength: 256
        )
        let tokenized = try tokenizer.tokenize(
            Array(repeating: "hello", count: 300).joined(separator: " ")
        )

        XCTAssertEqual(tokenized.inputIDs.count, 256)
        XCTAssertEqual(tokenized.attentionMask.count, 256)
        XCTAssertEqual(tokenized.inputIDs.first, 101)
        XCTAssertEqual(tokenized.inputIDs.last, 102)
        XCTAssertEqual(tokenized.attentionMask, Array(repeating: 1, count: 256))
    }

    func testProductionShapeTokenizerHandlesUnicodeAndRejectsEmptyInput() throws {
        let tokenizer = try BertWordPieceTokenizer(
            tokenizerJSON: makeTokenizerJSON(),
            sequenceLength: 256
        )
        let tokenized = try tokenizer.tokenize("Héllo 中文 👩🏽‍💻")

        XCTAssertEqual(tokenized.inputIDs.count, 256)
        XCTAssertEqual(tokenized.inputIDs.prefix(4), [101, 7592, 1746, 1861])
        XCTAssertThrowsError(try tokenizer.tokenize("")) { error in
            XCTAssertEqual(error as? EmbeddingKitError, .emptyInput)
        }
    }

    func testProductionContractReturns384FiniteUnitNormalizedF32Values() async throws {
        let embedder = CoreMLEmbedder(
            modelInfo: productionModelInfo(),
            tokenizer: ContractTokenizer(),
            backend: ContractBackend(output: Array(repeating: 2, count: 384))
        )
        let embedding = try await embedder.embed("hello")
        let norm = sqrt(embedding.reduce(Float(0)) { $0 + $1 * $1 })

        XCTAssertEqual(embedding.count, 384)
        XCTAssertTrue(embedding.allSatisfy(\.isFinite))
        XCTAssertEqual(norm, 1, accuracy: 1e-5)
    }

    func testProductionContractRejectsNonFiniteAndZeroNormOutput() async throws {
        var nonFinite = Array(repeating: Float(1), count: 384)
        nonFinite[17] = .nan
        for output in [nonFinite, Array(repeating: Float(0), count: 384)] {
            let embedder = CoreMLEmbedder(
                modelInfo: productionModelInfo(),
                tokenizer: ContractTokenizer(),
                backend: ContractBackend(output: output)
            )
            do {
                _ = try await embedder.embed("hello")
                XCTFail("expected output validation failure")
            } catch {
                guard case EmbeddingKitError.unsupportedModelInterface = error else {
                    return XCTFail("unexpected error \(error)")
                }
            }
        }
    }

    func testCoreMLRuntimeErrorsAreSurfaced() async throws {
        let embedder = CoreMLEmbedder(
            modelInfo: productionModelInfo(),
            tokenizer: ContractTokenizer(),
            backend: FailingContractBackend()
        )
        do {
            _ = try await embedder.embed("hello")
            XCTFail("expected runtime failure")
        } catch {
            XCTAssertEqual(error as? EmbeddingKitError, .backend("simulated Core ML failure"))
        }
    }
}

private struct ContractTokenizer: TextTokenizer {
    let identifier = "contract"

    func tokenize(_ text: String) throws -> TokenizedText {
        guard !text.isEmpty else { throw EmbeddingKitError.emptyInput }
        return try TokenizedText(inputIDs: [1], attentionMask: [1], tokenTypeIDs: [0])
    }
}

private actor ContractBackend: CoreMLEmbeddingBackend {
    nonisolated let runtimeInfo = EmbeddingRuntimeInfo(
        name: "Core ML contract fixture",
        requestedCompute: .all,
        actualCompute: .all
    )
    let output: [Float]

    init(output: [Float]) {
        self.output = output
    }

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        output
    }
}

private actor FailingContractBackend: CoreMLEmbeddingBackend {
    nonisolated let runtimeInfo = EmbeddingRuntimeInfo(
        name: "Core ML failure fixture",
        requestedCompute: .all,
        actualCompute: .all
    )

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        throw EmbeddingKitError.backend("simulated Core ML failure")
    }
}

private func productionModelInfo() -> EmbeddingModelInfo {
    try! EmbeddingModelInfo(
        identifier: "sentence-transformers/all-MiniLM-L6-v2",
        revision: "fixture",
        dimension: 384,
        maxInputTokens: 256,
        producesNormalizedEmbeddings: true
    )
}

private func makeTokenizerJSON() throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("tokenizer.json")
    let json = """
    {
      "version":"1.0",
      "normalizer":{"type":"BertNormalizer","clean_text":true,
        "handle_chinese_chars":true,"strip_accents":null,"lowercase":true},
      "pre_tokenizer":{"type":"BertPreTokenizer"},
      "post_processor":{"type":"TemplateProcessing",
        "special_tokens":{"[CLS]":{"ids":[101]},"[SEP]":{"ids":[102]}}},
      "model":{"type":"WordPiece","unk_token":"[UNK]",
        "continuing_subword_prefix":"##","max_input_chars_per_word":100,
        "vocab":{"[PAD]":0,"[UNK]":100,"[CLS]":101,"[SEP]":102,
          "中":1746,"文":1861,"hello":7592}}
    }
    """
    try Data(json.utf8).write(to: url)
    return url
}
#endif
