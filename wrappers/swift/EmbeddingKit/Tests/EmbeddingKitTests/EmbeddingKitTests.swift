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
}
