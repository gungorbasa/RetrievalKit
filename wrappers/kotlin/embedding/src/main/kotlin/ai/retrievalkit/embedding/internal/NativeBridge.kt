package ai.retrievalkit.embedding.internal

internal object NativeBridge {
    init {
        NativeLibraryLoader.load("retrievalkit_embedding_jni")
    }

    @JvmStatic external fun load(
        cacheDirectory: String?,
        localOnly: Boolean,
        runtimeLibraryPath: String?,
        intraThreads: Int,
        interThreads: Int,
    ): Long

    @JvmStatic external fun prefetch(cacheDirectory: String?, localOnly: Boolean)
    @JvmStatic external fun embed(handle: Long, text: String): FloatArray
    @JvmStatic external fun embedBatch(handle: Long, texts: Array<String>): Array<FloatArray>
    @JvmStatic external fun modelIdentifier(handle: Long): String
    @JvmStatic external fun modelRevision(handle: Long): String
    @JvmStatic external fun modelPrecision(handle: Long): String
    @JvmStatic external fun modelDimension(handle: Long): Int
    @JvmStatic external fun modelMaxInputTokens(handle: Long): Int
    @JvmStatic external fun modelProducesNormalizedEmbeddings(handle: Long): Boolean
    @JvmStatic external fun runtimeVersion(handle: Long): String
    @JvmStatic external fun close(handle: Long)
}
