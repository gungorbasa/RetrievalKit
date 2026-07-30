package ai.retrievalkit.embedding

import ai.retrievalkit.embedding.internal.EmbeddingBridge
import ai.retrievalkit.embedding.internal.NativeLibraryLoader
import java.nio.file.Files
import kotlin.math.sqrt
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

class OnnxEmbedderTest {
    @Test
    fun loadPassesAcquisitionAndRuntimeOptionsAndReportsImmutableInfo() {
        val bridge = FakeBridge()
        val cache = Files.createTempDirectory("embedding-cache-").toFile()
        val runtime = Files.createTempFile("onnxruntime-", ".dylib").toFile()

        OnnxEmbedder.loadWithBridge(
            bridge,
            localOnly = true,
            cacheDirectory = cache,
            runtimeLibrary = runtime,
            intraThreads = 3,
            interThreads = 2,
        ).use { embedder ->
            assertEquals(cache.absoluteFile.normalize().path, bridge.loadedCacheDirectory)
            assertTrue(bridge.loadedLocalOnly)
            assertEquals(runtime.absoluteFile.normalize().path, bridge.loadedRuntimeLibrary)
            assertEquals(3, bridge.loadedIntraThreads)
            assertEquals(2, bridge.loadedInterThreads)
            assertEquals(
                EmbeddingModelInfo(
                    identifier = "sentence-transformers/all-MiniLM-L6-v2",
                    revision = "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
                    precision = ModelPrecision.FP32,
                    dimension = 384,
                    maxInputTokens = 256,
                    producesNormalizedEmbeddings = true,
                    runtimeVersion = "1.24.3",
                ),
                embedder.modelInfo,
            )
        }
        assertEquals(listOf(41L), bridge.closedHandles)
    }

    @Test
    fun prefetchIsExplicitAndPreservesLocalOnly() {
        val bridge = FakeBridge()
        val cache = Files.createTempDirectory("embedding-prefetch-").toFile()
        OnnxEmbedder.prefetchWithBridge(bridge, cache, localOnly = true)
        assertEquals(cache.absoluteFile.normalize().path, bridge.prefetchedCacheDirectory)
        assertTrue(bridge.prefetchedLocalOnly)
        assertEquals(0, bridge.loadCalls)
    }

    @Test
    fun embeddingAndBatchReturnValidatedPrimitiveArrays() {
        val bridge = FakeBridge()
        OnnxEmbedder.loadWithBridge(bridge).use { embedder ->
            val single = embedder.embed("Merhaba, dünya 🌍")
            assertEquals(384, single.size)
            assertContentEquals(bridge.embedding, single)

            val batch = embedder.embedBatch(listOf("first", "ikinci"))
            assertEquals(2, batch.size)
            assertContentEquals(bridge.embedding, batch[0])
            assertContentEquals(bridge.embedding, batch[1])
            assertEquals(listOf("Merhaba, dünya 🌍"), bridge.embeddedTexts)
            assertEquals(listOf(listOf("first", "ikinci")), bridge.embeddedBatches)
        }
    }

    @Test
    fun emptyBatchIsRejectedWithoutCrossingNativeBoundary() {
        val bridge = FakeBridge()
        OnnxEmbedder.loadWithBridge(bridge).use { embedder ->
            assertFailsWith<InvalidEmbeddingInputException> {
                embedder.embedBatch(emptyList())
            }
        }
        assertTrue(bridge.embeddedBatches.isEmpty())
    }

    @Test
    fun blankInputAndInvalidThreadCountsUseTypedErrors() {
        val bridge = FakeBridge()
        OnnxEmbedder.loadWithBridge(bridge).use { embedder ->
            assertFailsWith<InvalidEmbeddingInputException> { embedder.embed("") }
            assertFailsWith<InvalidEmbeddingInputException> { embedder.embed(" \n\t") }
            assertFailsWith<InvalidEmbeddingInputException> {
                embedder.embedBatch(listOf("valid", " "))
            }
        }
        assertFailsWith<InvalidEmbeddingInputException> {
            OnnxEmbedder.loadWithBridge(bridge, intraThreads = 0)
        }
        assertFailsWith<InvalidEmbeddingInputException> {
            OnnxEmbedder.loadWithBridge(bridge, interThreads = 0)
        }
    }

    @Test
    fun outputShapeFinitenessAndNormalizationAreEnforced() {
        fun failureFor(embedding: FloatArray): EmbeddingInferenceException {
            val bridge = FakeBridge().apply { this.embedding = embedding }
            return OnnxEmbedder.loadWithBridge(bridge).use { embedder ->
                assertFailsWith<EmbeddingInferenceException> { embedder.embed("query") }
            }
        }

        assertTrue(failureFor(FloatArray(383)).message.orEmpty().contains("384"))
        val nonFinite = unitEmbedding().also { it[9] = Float.NaN }
        assertTrue(failureFor(nonFinite).message.orEmpty().contains("not finite"))
        assertTrue(
            failureFor(FloatArray(384) { 0.25f }).message.orEmpty().contains("not L2-normalized"),
        )
    }

    @Test
    fun batchCardinalityIsEnforced() {
        val bridge = FakeBridge().apply { batchCardinalityOffset = -1 }
        OnnxEmbedder.loadWithBridge(bridge).use { embedder ->
            val error = assertFailsWith<EmbeddingInferenceException> {
                embedder.embedBatch(listOf("one", "two"))
            }
            assertTrue(error.message.orEmpty().contains("1 embeddings for 2 texts"))
        }
    }

    @Test
    fun closeIsIdempotentAndUseAfterCloseIsTyped() {
        val bridge = FakeBridge()
        val embedder = OnnxEmbedder.loadWithBridge(bridge)
        embedder.close()
        embedder.close()
        assertEquals(listOf(41L), bridge.closedHandles)
        assertFailsWith<ClosedEmbedderException> { embedder.embed("query") }
        assertFailsWith<ClosedEmbedderException> { embedder.embedBatch(listOf("query")) }
    }

    @Test
    fun badModelContractClosesNativeHandle() {
        val bridge = FakeBridge().apply { dimension = 768 }
        val error = assertFailsWith<ModelLoadException> {
            OnnxEmbedder.loadWithBridge(bridge)
        }
        assertTrue(error.message.orEmpty().contains("384-dimensional"))
        assertEquals(listOf(41L), bridge.closedHandles)
    }

    @Test
    fun unsupportedPrecisionClosesNativeHandle() {
        val bridge = FakeBridge().apply { precision = "fp16" }
        assertFailsWith<ModelLoadException> {
            OnnxEmbedder.loadWithBridge(bridge)
        }
        assertEquals(listOf(41L), bridge.closedHandles)
    }

    @Test
    fun typedNativeFailuresArePreservedAndUnknownFailuresAreWrapped() {
        val typed = FakeBridge().apply {
            loadFailure = ModelIntegrityException("sha-256 mismatch")
        }
        assertFailsWith<ModelIntegrityException> {
            OnnxEmbedder.loadWithBridge(typed)
        }

        val unknown = FakeBridge().apply {
            loadFailure = IllegalStateException("native panic")
        }
        val wrapped = assertFailsWith<EmbeddingException> {
            OnnxEmbedder.loadWithBridge(unknown)
        }
        assertIs<IllegalStateException>(wrapped.cause)
        assertTrue(wrapped.message.orEmpty().contains("loading the embedding provider"))
    }

    @Test
    fun explicitRuntimeMustMatchTheQualifiedIdentityWithoutDeletingCallerFile() {
        val runtime = Files.createTempFile("unqualified-onnxruntime-", ".dylib")
        Files.writeString(runtime, "not the qualified runtime")

        val error = assertFailsWith<NativeLibraryException> {
            NativeLibraryLoader.resolveRuntimeLibrary(runtime.toString())
        }
        assertTrue(error.message.orEmpty().contains("size mismatch"))
        assertTrue(Files.exists(runtime))
    }

    @Test
    fun noRetrievalDependencyIsPresentInApiClassLoader() {
        val loader = OnnxEmbedder::class.java.classLoader
        val retrievalPresent = runCatching {
            Class.forName("ai.retrievalkit.RetrievalDatabase", false, loader)
        }.isSuccess
        assertFalse(retrievalPresent)
    }
}

private class FakeBridge : EmbeddingBridge {
    var loadedCacheDirectory: String? = null
    var loadedLocalOnly: Boolean = false
    var loadedRuntimeLibrary: String? = null
    var loadedIntraThreads: Int = 0
    var loadedInterThreads: Int = 0
    var loadCalls: Int = 0
    var prefetchedCacheDirectory: String? = null
    var prefetchedLocalOnly: Boolean = false
    var loadFailure: Throwable? = null
    var dimension: Int = 384
    var precision: String = "fp32"
    var embedding: FloatArray = unitEmbedding()
    var batchCardinalityOffset: Int = 0
    val embeddedTexts = mutableListOf<String>()
    val embeddedBatches = mutableListOf<List<String>>()
    val closedHandles = mutableListOf<Long>()

    override fun load(
        cacheDirectory: String?,
        localOnly: Boolean,
        runtimeLibraryPath: String?,
        intraThreads: Int,
        interThreads: Int,
    ): Long {
        loadCalls += 1
        loadFailure?.let { throw it }
        loadedCacheDirectory = cacheDirectory
        loadedLocalOnly = localOnly
        loadedRuntimeLibrary = runtimeLibraryPath
        loadedIntraThreads = intraThreads
        loadedInterThreads = interThreads
        return 41L
    }

    override fun prefetch(cacheDirectory: String?, localOnly: Boolean) {
        prefetchedCacheDirectory = cacheDirectory
        prefetchedLocalOnly = localOnly
    }

    override fun embed(handle: Long, text: String): FloatArray {
        assertEquals(41L, handle)
        embeddedTexts += text
        return embedding.copyOf()
    }

    override fun embedBatch(handle: Long, texts: Array<String>): Array<FloatArray> {
        assertEquals(41L, handle)
        embeddedBatches += texts.toList()
        val count = (texts.size + batchCardinalityOffset).coerceAtLeast(0)
        return Array(count) { embedding.copyOf() }
    }

    override fun modelIdentifier(handle: Long): String =
        "sentence-transformers/all-MiniLM-L6-v2"

    override fun modelRevision(handle: Long): String =
        "c9745ed1d9f207416be6d2e6f8de32d1f16199bf"

    override fun modelPrecision(handle: Long): String = precision
    override fun modelDimension(handle: Long): Int = dimension
    override fun modelMaxInputTokens(handle: Long): Int = 256
    override fun modelProducesNormalizedEmbeddings(handle: Long): Boolean = true
    override fun runtimeVersion(handle: Long): String = "1.24.3"
    override fun close(handle: Long) {
        closedHandles += handle
    }
}

private fun unitEmbedding(): FloatArray {
    val value = (1.0 / sqrt(384.0)).toFloat()
    return FloatArray(384) { value }
}
