import EmbeddingKit
import RetrievalKit
import RetrievalKitPipeline

@main
enum PipelineExample {
    static func main() async throws {
        let document = "RetrievalKit connects Rust retrieval to Swift."
        let query = "Rust retrieval"
        let model = try EmbeddingModelInfo(identifier: "quickstart", dimension: 4)
        let embedder = try PrecomputedEmbedder(
            modelInfo: model,
            embeddings: [
                document: [1, 0, 0, 0],
                query: [1, 0, 0, 0],
            ]
        )
        let pipeline = Pipeline(
            index: try VectorIndex(dimension: model.dimension),
            embedder: embedder
        )

        _ = try await pipeline.add(document: Document(id: "quickstart", text: document))
        let hits = try await pipeline.search(query, topK: 3)
        print("Found \(hits.count) result(s) for '\(query)'.")
    }
}
