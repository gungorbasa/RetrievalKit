package ai.retrievalkit.internal

import ai.retrievalkit.Filter

internal object NativeBridge {
    init {
        NativeLibraryLoader.load("retrievalkit_jni")
    }

    @JvmStatic external fun createRetrievalBuilder(
        corpusId: String,
        metric: Int,
        encoding: Int,
        bm25K1: Float,
        bm25B: Float,
        stopWords: Array<String>,
    ): Long

    @JvmStatic external fun retrievalBuilderUpsert(
        handle: Long,
        document: NativeDocument,
        embedding: FloatArray,
    )

    @JvmStatic external fun buildRetrieval(handle: Long): Long
    @JvmStatic external fun loadRetrieval(path: String): Long
    @JvmStatic external fun validateRetrieval(path: String)
    @JvmStatic external fun retrievalDimension(handle: Long): Int

    @JvmStatic external fun semanticSearch(
        handle: Long,
        embedding: FloatArray,
        limit: Int,
        filter: Filter?,
        selectionHandle: Long,
    ): Array<NativeSearchHit>

    @JvmStatic external fun keywordSearch(
        handle: Long,
        text: String,
        limit: Int,
        filter: Filter?,
        selectionHandle: Long,
    ): Array<NativeKeywordHit>

    @JvmStatic external fun hybridSearch(
        handle: Long,
        text: String,
        embedding: FloatArray?,
        limit: Int,
        alpha: Float,
        filter: Filter?,
        vectorCandidates: Int,
        keywordCandidates: Int,
        selectionHandle: Long,
    ): Array<NativeHybridHit>

    @JvmStatic external fun deleteRecord(handle: Long, recordId: String): Int
    @JvmStatic external fun saveRetrieval(handle: Long, path: String, includeBm25: Boolean): Long
    @JvmStatic external fun compact(handle: Long): LongArray
    @JvmStatic external fun closeHandle(handle: Long)
}
