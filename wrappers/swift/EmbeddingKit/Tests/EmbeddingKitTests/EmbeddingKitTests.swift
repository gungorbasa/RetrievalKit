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
        XCTAssertEqual(KnownEmbeddingModels.jinaEmbeddingsV2SmallEn.dimension, 512)
        XCTAssertEqual(KnownEmbeddingModels.bgeBaseEnV15.dimension, 768)
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

    func testCoreMLConfigurationInitializerReportsUnsupportedModelInterface() async throws {
        let model = try EmbeddingModelInfo(identifier: "coreml-test", dimension: 2)
        let embedder = CoreMLEmbedder(
            modelInfo: model,
            tokenizer: FakeTokenizer(),
            configuration: CoreMLModelConfiguration(
                modelURL: URL(fileURLWithPath: "/tmp/missing.mlmodelc"),
                compute: .cpuAndNeuralEngine
            )
        )

        XCTAssertEqual(embedder.runtimeInfo.name, "Core ML")
        XCTAssertEqual(embedder.runtimeInfo.requestedCompute, .cpuAndNeuralEngine)

        do {
            _ = try await embedder.embed("alpha")
            XCTFail("expected unsupported model interface")
        } catch {
            guard case EmbeddingKitError.unsupportedModelInterface(let message) = error else {
                return XCTFail("expected unsupported interface error, got \(error)")
            }
            XCTAssertTrue(message.contains("Core ML model loading is not implemented yet"))
        }
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

private struct FakeCoreMLBackend: CoreMLEmbeddingBackend {
    var dimension: Int
    var runtimeInfo = EmbeddingRuntimeInfo(
        name: "Fake Core ML",
        requestedCompute: .cpuAndNeuralEngine,
        actualCompute: .cpuAndNeuralEngine
    )

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        Array(repeating: Float(input.inputIDs[0]), count: dimension)
    }
}

private struct WrongCountCoreMLBackend: CoreMLEmbeddingBackend {
    var runtimeInfo = EmbeddingRuntimeInfo(name: "Fake Core ML")

    func predictEmbedding(for input: TokenizedText) async throws -> [Float] {
        [1, 1]
    }

    func predictEmbeddings(for inputs: [TokenizedText]) async throws -> [[Float]] {
        [[1, 1]]
    }
}
#endif
