package ai.retrievalkit.embedding.internal

internal interface EmbeddingBridge {
    fun resolveRuntimeLibrary(explicitPath: String?): String? = explicitPath

    fun load(
        cacheDirectory: String?,
        localOnly: Boolean,
        runtimeLibraryPath: String?,
        intraThreads: Int,
        interThreads: Int,
    ): Long

    fun prefetch(cacheDirectory: String?, localOnly: Boolean)
    fun embed(handle: Long, text: String): FloatArray
    fun embedBatch(handle: Long, texts: Array<String>): Array<FloatArray>
    fun modelIdentifier(handle: Long): String
    fun modelRevision(handle: Long): String
    fun modelPrecision(handle: Long): String
    fun modelDimension(handle: Long): Int
    fun modelMaxInputTokens(handle: Long): Int
    fun modelProducesNormalizedEmbeddings(handle: Long): Boolean
    fun runtimeVersion(handle: Long): String
    fun close(handle: Long)
}

internal object JniEmbeddingBridge : EmbeddingBridge {
    override fun resolveRuntimeLibrary(explicitPath: String?): String =
        NativeLibraryLoader.resolveRuntimeLibrary(explicitPath)

    override fun load(
        cacheDirectory: String?,
        localOnly: Boolean,
        runtimeLibraryPath: String?,
        intraThreads: Int,
        interThreads: Int,
    ): Long = NativeBridge.load(
        cacheDirectory,
        localOnly,
        runtimeLibraryPath,
        intraThreads,
        interThreads,
    )

    override fun prefetch(cacheDirectory: String?, localOnly: Boolean) =
        NativeBridge.prefetch(cacheDirectory, localOnly)

    override fun embed(handle: Long, text: String): FloatArray = NativeBridge.embed(handle, text)
    override fun embedBatch(handle: Long, texts: Array<String>): Array<FloatArray> =
        NativeBridge.embedBatch(handle, texts)

    override fun modelIdentifier(handle: Long): String = NativeBridge.modelIdentifier(handle)
    override fun modelRevision(handle: Long): String = NativeBridge.modelRevision(handle)
    override fun modelPrecision(handle: Long): String = NativeBridge.modelPrecision(handle)
    override fun modelDimension(handle: Long): Int = NativeBridge.modelDimension(handle)
    override fun modelMaxInputTokens(handle: Long): Int = NativeBridge.modelMaxInputTokens(handle)
    override fun modelProducesNormalizedEmbeddings(handle: Long): Boolean =
        NativeBridge.modelProducesNormalizedEmbeddings(handle)

    override fun runtimeVersion(handle: Long): String = NativeBridge.runtimeVersion(handle)
    override fun close(handle: Long) = NativeBridge.close(handle)
}
