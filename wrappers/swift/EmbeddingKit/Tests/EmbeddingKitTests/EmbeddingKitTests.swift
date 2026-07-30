import XCTest
@testable import EmbeddingKit

final class EmbeddingKitTests: XCTestCase {
    func testPrecomputedEmbedderReturnsSingleAndBatchEmbeddings() async throws {
        let model = try EmbeddingModelInfo(identifier: "test-model", dimension: 2)
        let embedder = try PrecomputedEmbedder(
            modelInfo: model,
            embeddings: [
                "alpha": [1, 0],
                "beta": [0, 1],
            ]
        )

        let single = try await embedder.embed("alpha")
        let batch = try await embedder.embed(["alpha", "beta"])

        XCTAssertEqual(single, [1, 0])
        XCTAssertEqual(batch, [[1, 0], [0, 1]])
        XCTAssertEqual(embedder.runtimeInfo.actualCompute, .cpuOnly)
    }

    func testPrecomputedEmbedderRejectsDimensionMismatch() throws {
        let model = try EmbeddingModelInfo(identifier: "test-model", dimension: 2)

        XCTAssertThrowsError(
            try PrecomputedEmbedder(modelInfo: model, embeddings: ["bad": [1, 0, 0]])
        ) { error in
            XCTAssertEqual(error as? EmbeddingKitError, .invalidDimension(expected: 2, actual: 3))
        }
    }

    func testPrecomputedEmbedderReportsMissingText() async throws {
        let model = try EmbeddingModelInfo(identifier: "test-model", dimension: 2)
        let embedder = try PrecomputedEmbedder(modelInfo: model, embeddings: ["alpha": [1, 0]])

        do {
            _ = try await embedder.embed("missing")
            XCTFail("expected missing embedding")
        } catch {
            XCTAssertEqual(error as? EmbeddingKitError, .missingPrecomputedEmbedding("missing"))
        }
    }

    func testKnownModelsUseExpectedDimensions() {
        XCTAssertEqual(KnownEmbeddingModels.bgeSmallEnV15.dimension, 384)
        XCTAssertEqual(KnownEmbeddingModels.allMiniLML6V2.dimension, 384)
        XCTAssertEqual(KnownEmbeddingModels.e5SmallV2.dimension, 384)
        XCTAssertEqual(KnownEmbeddingModels.gteSmall.dimension, 384)
        XCTAssertEqual(KnownEmbeddingModels.bgeBaseEnV15.dimension, 768)
        XCTAssertEqual(KnownEmbeddingModels.snowflakeArcticEmbedM.dimension, 768)
    }

    func testBenchmarkRunsSingleQueryAndBatchMeasurements() async throws {
        let model = try EmbeddingModelInfo(identifier: "test-model", dimension: 2)
        let embedder = try PrecomputedEmbedder(
            modelInfo: model,
            embeddings: [
                "alpha": [1, 0],
                "beta": [0, 1],
                "gamma": [0.5, 0.5],
            ]
        )
        let config = try EmbeddingBenchmarkConfig(
            warmupIterations: 2,
            measuredIterations: 5,
            batchSizes: [1, 2]
        )

        let report = try await EmbeddingBenchmark.run(
            embedder: embedder,
            queries: ["alpha", "beta", "gamma"],
            config: config
        )

        XCTAssertEqual(report.modelInfo, model)
        XCTAssertEqual(report.queryCount, 3)
        XCTAssertEqual(report.singleQueryLatency.sampleCount, 5)
        XCTAssertEqual(report.batchResults.map(\.batchSize), [1, 2])
        XCTAssertTrue(report.batchResults.allSatisfy { $0.latency.sampleCount == 5 })
        XCTAssertTrue(report.batchResults.allSatisfy { $0.textsPerSecond >= 0 })
    }

    func testTokenizedTextValidatesAlignedInputs() throws {
        XCTAssertNoThrow(try TokenizedText(inputIDs: [1, 2], attentionMask: [1, 1]))
        XCTAssertThrowsError(try TokenizedText(inputIDs: [1, 2], attentionMask: [1])) { error in
            guard case EmbeddingKitError.unsupportedModelInterface(let message) = error else {
                return XCTFail("expected unsupported interface error, got \(error)")
            }
            XCTAssertTrue(message.contains("attention mask"))
        }
    }

    func testBertWordPieceTokenizerMatchesExpectedCoreMLInputs() throws {
        let tokenizerURL = try makeWordPieceTokenizerFixture()
        let tokenizer = try BertWordPieceTokenizer(tokenizerJSON: tokenizerURL, sequenceLength: 8)

        let tokenized = try tokenizer.tokenize("Hello, RetrievalKit!")

        XCTAssertEqual(tokenized.inputIDs, [101, 7592, 1010, 9207, 23615, 999, 102, 0])
        XCTAssertEqual(tokenized.attentionMask, [1, 1, 1, 1, 1, 1, 1, 0])
        XCTAssertEqual(tokenized.tokenTypeIDs, [0, 0, 0, 0, 0, 0, 0, 0])
    }

    func testBertWordPieceTokenizerNormalizesChineseAndAccentedText() throws {
        let tokenizerURL = try makeWordPieceTokenizerFixture()
        let tokenizer = try BertWordPieceTokenizer(tokenizerJSON: tokenizerURL, sequenceLength: 12)

        let tokenized = try tokenizer.tokenize("Héllo, 中文!")

        XCTAssertEqual(tokenized.inputIDs, [101, 7592, 1010, 1746, 1861, 999, 102, 0, 0, 0, 0, 0])
        XCTAssertEqual(tokenized.attentionMask, [1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0])
    }

    func testBertWordPieceTokenizerTruncatesToSequenceLength() throws {
        let tokenizerURL = try makeWordPieceTokenizerFixture()
        let tokenizer = try BertWordPieceTokenizer(tokenizerJSON: tokenizerURL, sequenceLength: 5)

        let tokenized = try tokenizer.tokenize("hello private notes search")

        XCTAssertEqual(tokenized.inputIDs, [101, 7592, 2797, 3964, 102])
        XCTAssertEqual(tokenized.attentionMask, [1, 1, 1, 1, 1])
    }

    func testBertWordPieceTokenizerUsesUnknownForMissingWordPiece() throws {
        let tokenizerURL = try makeWordPieceTokenizerFixture()
        let tokenizer = try BertWordPieceTokenizer(tokenizerJSON: tokenizerURL, sequenceLength: 5)

        let tokenized = try tokenizer.tokenize("missing")

        XCTAssertEqual(tokenized.inputIDs, [101, 100, 102, 0, 0])
        XCTAssertEqual(tokenized.attentionMask, [1, 1, 1, 0, 0])
    }

    func testBertWordPieceTokenizerMatchesGeneratedComparisonTokenizersWhenPresent() throws {
        let modelSlugs = [
            "bge-small-en-v1.5",
            "all-MiniLM-L6-v2",
            "e5-small-v2",
            "gte-small",
            "snowflake-arctic-embed-xs",
            "snowflake-arctic-embed-s",
        ]
        var comparedModels = 0

        for slug in modelSlugs {
            let tokenizerURL = repositoryRoot()
                .appendingPathComponent("target/embedding-models/\(slug)/tokenizer/tokenizer.json")
            guard FileManager.default.fileExists(atPath: tokenizerURL.path) else {
                continue
            }

            comparedModels += 1
            let tokenizer = try BertWordPieceTokenizer(tokenizerJSON: tokenizerURL, sequenceLength: 12)
            try assertTokenizer(
                tokenizer,
                text: "Hello, RetrievalKit!",
                inputIDs: [101, 7592, 1010, 9207, 23615, 999, 102, 0, 0, 0, 0, 0],
                attentionMask: [1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0],
                file: #filePath,
                line: #line
            )
            try assertTokenizer(
                tokenizer,
                text: "Héllo, 中文!",
                inputIDs: [101, 7592, 1010, 1746, 1861, 999, 102, 0, 0, 0, 0, 0],
                attentionMask: [1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0],
                file: #filePath,
                line: #line
            )
            try assertTokenizer(
                tokenizer,
                text: "unaffable",
                inputIDs: [101, 14477, 20961, 3468, 102, 0, 0, 0, 0, 0, 0, 0],
                attentionMask: [1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
                file: #filePath,
                line: #line
            )
        }

        guard comparedModels > 0 else {
            throw XCTSkip("generated comparison tokenizer fixtures are not present")
        }
    }

    #if canImport(CoreML)
    func testCoreMLEmbedderUsesTokenizerAndBackend() async throws {
        let model = try EmbeddingModelInfo(identifier: "coreml-test", dimension: 3)
        let embedder = CoreMLEmbedder(
            modelInfo: model,
            tokenizer: FakeTokenizer(),
            backend: FakeCoreMLBackend(dimension: 3)
        )

        let embedding = try await embedder.embed("alpha")
        let batch = try await embedder.embed(["alpha", "beta"])

        XCTAssertEqual(embedding, [5, 5, 5])
        XCTAssertEqual(batch, [[5, 5, 5], [4, 4, 4]])
        XCTAssertEqual(embedder.runtimeInfo.name, "Fake Core ML")
        XCTAssertEqual(embedder.runtimeInfo.actualCompute, .cpuAndNeuralEngine)
    }

    func testCoreMLEmbedderRejectsBackendDimensionMismatch() async throws {
        let model = try EmbeddingModelInfo(identifier: "coreml-test", dimension: 2)
        let embedder = CoreMLEmbedder(
            modelInfo: model,
            tokenizer: FakeTokenizer(),
            backend: FakeCoreMLBackend(dimension: 3)
        )

        do {
            _ = try await embedder.embed("alpha")
            XCTFail("expected dimension mismatch")
        } catch {
            XCTAssertEqual(error as? EmbeddingKitError, .invalidDimension(expected: 2, actual: 3))
        }
    }

    func testCoreMLConfigurationInitializerLoadsModelImmediately() throws {
        let model = try EmbeddingModelInfo(identifier: "coreml-test", dimension: 2)

        XCTAssertThrowsError(
            try CoreMLEmbedder(
                modelInfo: model,
                tokenizer: FakeTokenizer(),
                configuration: CoreMLModelConfiguration(
                    modelURL: URL(fileURLWithPath: "/tmp/missing.mlmodelc"),
                    compute: .cpuAndNeuralEngine
                )
            )
        )
    }

    func testCoreMLEmbedderRejectsWrongBatchCount() async throws {
        let model = try EmbeddingModelInfo(identifier: "coreml-test", dimension: 2)
        let embedder = CoreMLEmbedder(
            modelInfo: model,
            tokenizer: FakeTokenizer(),
            backend: WrongCountCoreMLBackend()
        )

        do {
            _ = try await embedder.embed(["alpha", "beta"])
            XCTFail("expected unsupported model interface")
        } catch {
            guard case EmbeddingKitError.unsupportedModelInterface(let message) = error else {
                return XCTFail("expected unsupported interface error, got \(error)")
            }
            XCTAssertTrue(message.contains("backend returned 1 embeddings for 2 inputs"))
        }
    }
    #endif
}

private func makeWordPieceTokenizerFixture() throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("tokenizer.json")
    let json = """
    {
      "version": "1.0",
      "normalizer": {
        "type": "BertNormalizer",
        "clean_text": true,
        "handle_chinese_chars": true,
        "strip_accents": null,
        "lowercase": true
      },
      "pre_tokenizer": {
        "type": "BertPreTokenizer"
      },
      "post_processor": {
        "type": "TemplateProcessing",
        "special_tokens": {
          "[CLS]": { "ids": [101] },
          "[SEP]": { "ids": [102] }
        }
      },
      "model": {
        "type": "WordPiece",
        "unk_token": "[UNK]",
        "continuing_subword_prefix": "##",
        "max_input_chars_per_word": 100,
        "vocab": {
          "[PAD]": 0,
          "[UNK]": 100,
          "[CLS]": 101,
          "[SEP]": 102,
          "!": 999,
          ",": 1010,
          "中": 1746,
          "文": 1861,
          "hello": 7592,
          "private": 2797,
          "notes": 3964,
          "search": 3945,
          "retrieval": 9207,
          "##kit": 23615
        }
      }
    }
    """
    try json.write(to: url, atomically: true, encoding: .utf8)
    return url
}

private func assertTokenizer(
    _ tokenizer: BertWordPieceTokenizer,
    text: String,
    inputIDs: [Int32],
    attentionMask: [Int32],
    file: StaticString = #filePath,
    line: UInt = #line
) throws {
    let tokenized = try tokenizer.tokenize(text)
    XCTAssertEqual(tokenized.inputIDs, inputIDs, file: file, line: line)
    XCTAssertEqual(tokenized.attentionMask, attentionMask, file: file, line: line)
    XCTAssertEqual(
        tokenized.tokenTypeIDs,
        Array(repeating: Int32(0), count: inputIDs.count),
        file: file,
        line: line
    )
}

private func repositoryRoot() -> URL {
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
}

#if canImport(CoreML)
private struct FakeTokenizer: TextTokenizer {
    let identifier = "fake-tokenizer"

    func tokenize(_ text: String) throws -> TokenizedText {
        guard !text.isEmpty else {
            throw EmbeddingKitError.emptyInput
        }
        return try TokenizedText(
            inputIDs: [Int32(text.count)],
            attentionMask: [1],
            tokenTypeIDs: [0]
        )
    }
}

private actor FakeCoreMLBackend: CoreMLEmbeddingBackend {
    var dimension: Int
    nonisolated let runtimeInfo = EmbeddingRuntimeInfo(
        name: "Fake Core ML",
        requestedCompute: .cpuAndNeuralEngine,
        actualCompute: .cpuAndNeuralEngine
    )

    init(dimension: Int) {
        self.dimension = dimension
    }

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        Array(repeating: Float(input.inputIDs[0]), count: dimension)
    }
}

private actor WrongCountCoreMLBackend: CoreMLEmbeddingBackend {
    nonisolated let runtimeInfo = EmbeddingRuntimeInfo(name: "Fake Core ML")

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        [1, 1]
    }

    func predictEmbeddings(for inputs: [TokenizedText]) async throws -> [[Float]] {
        [[1, 1]]
    }
}
#endif
