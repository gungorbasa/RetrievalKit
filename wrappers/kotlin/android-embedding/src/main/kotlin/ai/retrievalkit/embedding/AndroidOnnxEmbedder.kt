package ai.retrievalkit.embedding

import android.content.Context
import java.io.File

/**
 * Android entry point that places verified model artifacts in the
 * application cache. Model acquisition remains limited to [load] and
 * [prefetch]; retrieval database construction and search never use this API.
 */
object AndroidOnnxEmbedder {
    @JvmStatic
    @JvmOverloads
    fun load(
        context: Context,
        localOnly: Boolean = false,
        runtimeLibrary: File? = null,
        intraThreads: Int = Runtime.getRuntime().availableProcessors().coerceIn(1, 4),
        interThreads: Int = 1,
    ): OnnxEmbedder = OnnxEmbedder.load(
        localOnly = localOnly,
        cacheDirectory = cacheDirectory(context),
        runtimeLibrary = runtimeLibrary,
        intraThreads = intraThreads,
        interThreads = interThreads,
    )

    @JvmStatic
    @JvmOverloads
    fun prefetch(
        context: Context,
        localOnly: Boolean = false,
    ) {
        OnnxEmbedder.prefetch(
            cacheDirectory = cacheDirectory(context),
            localOnly = localOnly,
        )
    }

    private fun cacheDirectory(context: Context): File =
        File(context.applicationContext.cacheDir, "retrievalkit/embedding")
}
