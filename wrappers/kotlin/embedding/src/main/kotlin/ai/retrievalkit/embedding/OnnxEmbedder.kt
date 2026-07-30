package ai.retrievalkit.embedding

import ai.retrievalkit.embedding.internal.EmbeddingBridge
import ai.retrievalkit.embedding.internal.JniEmbeddingBridge
import java.io.File
import kotlin.math.abs
import kotlin.math.sqrt

/**
 * Blocking FP32 all-MiniLM-L6-v2 embedding provider.
 *
 * `load` and `prefetch` are the only methods that may access the network.
 * Calls on one instance are serialized; callers choose their own dispatcher.
 */
class OnnxEmbedder private constructor(
    private val bridge: EmbeddingBridge,
    private var handle: Long,
    val modelInfo: EmbeddingModelInfo,
) : AutoCloseable {
    private val lock = Any()

    fun embed(text: String): FloatArray {
        validateText(text)
        return synchronized(lock) {
            val current = requireOpen()
            validateEmbedding(call("embedding inference") { bridge.embed(current, text) })
        }
    }

    fun embedBatch(texts: List<String>): List<FloatArray> {
        if (texts.isEmpty()) {
            throw InvalidEmbeddingInputException("embedding batch cannot be empty")
        }
        texts.forEach(::validateText)
        return synchronized(lock) {
            val current = requireOpen()
            val embeddings = call("batch embedding inference") {
                bridge.embedBatch(current, texts.toTypedArray())
            }
            if (embeddings.size != texts.size) {
                throw EmbeddingInferenceException(
                    "native provider returned ${embeddings.size} embeddings for ${texts.size} texts",
                )
            }
            embeddings.map(::validateEmbedding)
        }
    }

    override fun close() {
        synchronized(lock) {
            val current = handle
            if (current == CLOSED_HANDLE) return
            handle = CLOSED_HANDLE
            call("closing the embedding provider") { bridge.close(current) }
        }
    }

    private fun requireOpen(): Long =
        handle.takeUnless { it == CLOSED_HANDLE } ?: throw ClosedEmbedderException()

    private fun validateEmbedding(embedding: FloatArray): FloatArray {
        if (embedding.size != modelInfo.dimension || embedding.size != EXPECTED_DIMENSION) {
            throw EmbeddingInferenceException(
                "expected $EXPECTED_DIMENSION embedding values, received ${embedding.size}",
            )
        }
        var squaredNorm = 0.0
        embedding.forEachIndexed { index, value ->
            if (!value.isFinite()) {
                throw EmbeddingInferenceException("embedding value $index is not finite")
            }
            squaredNorm += value.toDouble() * value.toDouble()
        }
        val norm = sqrt(squaredNorm)
        if (abs(norm - 1.0) > NORMALIZATION_TOLERANCE) {
            throw EmbeddingInferenceException(
                "embedding is not L2-normalized (norm=$norm)",
            )
        }
        return embedding
    }

    companion object {
        private const val CLOSED_HANDLE = 0L
        private const val EXPECTED_DIMENSION = 384
        private const val EXPECTED_MAX_INPUT_TOKENS = 256
        private const val NORMALIZATION_TOLERANCE = 1e-3

        /**
         * Loads the canonical FP32 model, downloading verified artifacts only
         * when `localOnly` is false and the selected cache is incomplete.
         */
        @JvmStatic
        @JvmOverloads
        fun load(
            localOnly: Boolean = false,
            cacheDirectory: File? = null,
            runtimeLibrary: File? = null,
            intraThreads: Int = defaultIntraThreads(),
            interThreads: Int = 1,
        ): OnnxEmbedder = loadWithBridge(
            bridge = JniEmbeddingBridge,
            localOnly = localOnly,
            cacheDirectory = cacheDirectory,
            runtimeLibrary = runtimeLibrary,
            intraThreads = intraThreads,
            interThreads = interThreads,
        )

        /** Acquires and verifies the canonical model without creating a session. */
        @JvmStatic
        @JvmOverloads
        fun prefetch(
            cacheDirectory: File? = null,
            localOnly: Boolean = false,
        ) {
            prefetchWithBridge(JniEmbeddingBridge, cacheDirectory, localOnly)
        }

        internal fun loadWithBridge(
            bridge: EmbeddingBridge,
            localOnly: Boolean = false,
            cacheDirectory: File? = null,
            runtimeLibrary: File? = null,
            intraThreads: Int = defaultIntraThreads(),
            interThreads: Int = 1,
        ): OnnxEmbedder {
            requireThreadCount(intraThreads, "intraThreads")
            requireThreadCount(interThreads, "interThreads")
            cacheDirectory?.let { validateDirectoryArgument(it, "cacheDirectory") }
            runtimeLibrary?.let {
                if (!it.isFile) {
                    throw NativeLibraryException("runtimeLibrary must identify a regular file: $it")
                }
            }
            val runtimePath = call("resolving the ONNX Runtime library") {
                bridge.resolveRuntimeLibrary(
                    runtimeLibrary?.absoluteFile?.normalize()?.path,
                )
            }
            val handle = call("loading the embedding provider") {
                bridge.load(
                    cacheDirectory?.absoluteFile?.normalize()?.path,
                    localOnly,
                    runtimePath,
                    intraThreads,
                    interThreads,
                )
            }
            if (handle == CLOSED_HANDLE) {
                throw ModelLoadException("native provider returned an invalid handle")
            }
            return try {
                val info = modelInfo(bridge, handle)
                OnnxEmbedder(bridge, handle, info)
            } catch (error: Throwable) {
                try {
                    bridge.close(handle)
                } catch (closeError: Throwable) {
                    error.addSuppressed(closeError)
                }
                throw error
            }
        }

        internal fun prefetchWithBridge(
            bridge: EmbeddingBridge,
            cacheDirectory: File? = null,
            localOnly: Boolean = false,
        ) {
            cacheDirectory?.let { validateDirectoryArgument(it, "cacheDirectory") }
            call("prefetching the embedding model") {
                bridge.prefetch(cacheDirectory?.absoluteFile?.normalize()?.path, localOnly)
            }
        }

        private fun modelInfo(bridge: EmbeddingBridge, handle: Long): EmbeddingModelInfo {
            val precision = when (val value = bridge.modelPrecision(handle).lowercase()) {
                "fp32" -> ModelPrecision.FP32
                else -> throw ModelLoadException("unsupported model precision '$value'; expected fp32")
            }
            val info = EmbeddingModelInfo(
                identifier = bridge.modelIdentifier(handle),
                revision = bridge.modelRevision(handle),
                precision = precision,
                dimension = bridge.modelDimension(handle),
                maxInputTokens = bridge.modelMaxInputTokens(handle),
                producesNormalizedEmbeddings = bridge.modelProducesNormalizedEmbeddings(handle),
                runtimeVersion = bridge.runtimeVersion(handle),
            )
            if (info.identifier.isBlank() || info.revision.isBlank()) {
                throw ModelLoadException("native provider returned incomplete model identity")
            }
            if (
                info.dimension != EXPECTED_DIMENSION ||
                info.maxInputTokens != EXPECTED_MAX_INPUT_TOKENS ||
                !info.producesNormalizedEmbeddings
            ) {
                throw ModelLoadException(
                    "expected FP32 $EXPECTED_DIMENSION-dimensional normalized output with a " +
                        "$EXPECTED_MAX_INPUT_TOKENS-token limit, received $info",
                )
            }
            return info
        }

        private fun defaultIntraThreads(): Int =
            Runtime.getRuntime().availableProcessors().coerceIn(1, 4)

        private fun requireThreadCount(value: Int, name: String) {
            if (value < 1) {
                throw InvalidEmbeddingInputException("$name must be at least 1")
            }
        }

        private fun validateDirectoryArgument(file: File, name: String) {
            if (file.exists() && !file.isDirectory) {
                throw InvalidEmbeddingInputException("$name must be a directory: $file")
            }
        }
    }
}

private fun validateText(text: String) {
    if (text.isBlank()) {
        throw InvalidEmbeddingInputException("text must not be empty or blank")
    }
}

private inline fun <T> call(operation: String, block: () -> T): T {
    try {
        return block()
    } catch (error: EmbeddingException) {
        throw error
    } catch (error: UnsatisfiedLinkError) {
        throw NativeLibraryException("$operation failed: ${error.message}", error)
    } catch (error: Throwable) {
        throw EmbeddingException("$operation failed: ${error.message}", error)
    }
}
