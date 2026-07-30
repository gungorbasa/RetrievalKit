package ai.retrievalkit.embedding

/** The model weight precision. Database vector encoding is a separate choice. */
enum class ModelPrecision {
    FP32,
}

/** Immutable identity and output contract reported by the loaded native model. */
data class EmbeddingModelInfo(
    val identifier: String,
    val revision: String,
    val precision: ModelPrecision,
    val dimension: Int,
    val maxInputTokens: Int,
    val producesNormalizedEmbeddings: Boolean,
    val runtimeVersion: String,
)
