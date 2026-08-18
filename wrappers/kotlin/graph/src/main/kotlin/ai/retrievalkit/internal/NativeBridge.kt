package ai.retrievalkit.internal

import ai.retrievalkit.EmbeddedDocument
import ai.retrievalkit.Filter
import ai.retrievalkit.GraphQuery
import ai.retrievalkit.GraphSchema
import ai.retrievalkit.Record

internal object NativeBridge {
    init {
        NativeLibraryLoader.load("retrievalkit_jni_graph")
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

    @JvmStatic external fun createGraphBuilder(corpusId: String, schema: GraphSchema): Long
    @JvmStatic external fun graphBuilderUpsert(handle: Long, record: Record)
    @JvmStatic external fun buildGraph(handle: Long): Long
    @JvmStatic external fun createGraphRetrievalBuilder(
        corpusId: String,
        schema: GraphSchema,
        metric: Int,
        encoding: Int,
        bm25K1: Float,
        bm25B: Float,
        stopWords: Array<String>,
    ): Long
    @JvmStatic external fun graphRetrievalBuilderUpsert(
        handle: Long,
        record: Record,
        embedding: FloatArray?,
        documents: Array<EmbeddedDocument>?,
    )
    @JvmStatic external fun graphRetrievalBuilderUpsertFixtureChunk(
        handle: Long,
        record: Record,
        chunkKey: String,
        text: String,
        embedding: FloatArray,
        metadata: Array<NativeMetadataEntry>,
    )
    @JvmStatic external fun buildGraphRetrieval(handle: Long): Long
    @JvmStatic external fun graphQuery(handle: Long, query: GraphQuery): Long
    @JvmStatic external fun graphSelection(handle: Long): NativeGraphSelection
    @JvmStatic external fun projectCandidates(
        handle: Long,
        selectionHandle: Long,
        filter: Filter?,
    ): NativeProjection
    @JvmStatic external fun saveGraph(handle: Long, path: String, retrieval: Boolean): Long
    @JvmStatic external fun loadGraph(path: String, retrieval: Boolean): Long
    @JvmStatic external fun validateGraph(path: String, retrieval: Boolean)
    @JvmStatic external fun closeHandle(handle: Long)
}
