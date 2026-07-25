package ai.retrievalkit

import ai.retrievalkit.internal.NativeBridge
import ai.retrievalkit.internal.NativeHandle
import ai.retrievalkit.internal.toNative
import java.nio.file.Path

/**
 * A local exact-vector and BM25 retrieval database.
 *
 * Methods are blocking and operations on one instance are serialized in the native boundary.
 * Call disk, build, or query work from an application-selected background executor on Android.
 */
public class RetrievalDatabase private constructor(nativeHandle: Long) : AutoCloseable {
    private val handle = NativeHandle(nativeHandle, NativeBridge::closeHandle)

    public val dimension: Int
        get() = NativeBridge.retrievalDimension(handle.requireOpen("RetrievalDatabase"))

    public fun search(
        embedding: FloatArray,
        limit: Int = 10,
        filter: Filter? = null,
    ): List<SearchHit> = NativeBridge.semanticSearch(
        handle.requireOpen("RetrievalDatabase"),
        embedding,
        limit,
        filter,
        0,
    ).map { it.publicValue() }

    public fun search(
        text: String,
        embedding: FloatArray? = null,
        limit: Int = 10,
        alpha: Float = 0.6f,
        filter: Filter? = null,
        vectorCandidates: Int = 50,
        keywordCandidates: Int = 50,
    ): List<HybridHit> = NativeBridge.hybridSearch(
        handle.requireOpen("RetrievalDatabase"),
        text,
        embedding,
        limit,
        alpha,
        filter,
        vectorCandidates,
        keywordCandidates,
        0,
    ).map { it.publicValue() }

    public fun search(
        text: String,
        limit: Int = 10,
        filter: Filter? = null,
    ): List<HybridHit> = search(text, null, limit, 0.0f, filter)

    public fun delete(recordId: String): Int =
        NativeBridge.deleteRecord(handle.requireOpen("RetrievalDatabase"), recordId)

    public fun save(path: Path, includeBm25: Boolean = true): PersistenceReport =
        PersistenceReport(
            NativeBridge.saveRetrieval(
                handle.requireOpen("RetrievalDatabase"),
                path.toAbsolutePath().toString(),
                includeBm25,
            ),
        )

    public fun compact(): CompactionReport {
        val values = NativeBridge.compact(handle.requireOpen("RetrievalDatabase"))
        return CompactionReport(values[0], values[1], values[2], values[3])
    }

    override fun close(): Unit = handle.close()

    public class Builder(
        corpusId: String,
        metric: VectorMetric = VectorMetric.COSINE,
        encoding: VectorEncoding = VectorEncoding.I8_SCALAR_QUANTIZED,
    ) : AutoCloseable {
        private val handle = NativeHandle(
            NativeBridge.createRetrievalBuilder(corpusId, metric.ordinal, encoding.ordinal),
            NativeBridge::closeHandle,
        )
        private var consumed = false

        public fun upsert(document: Document, embedding: FloatArray): Builder {
            NativeBridge.retrievalBuilderUpsert(
                handle.requireOpen("RetrievalDatabase.Builder"),
                document.toNative(),
                embedding,
            )
            return this
        }

        public fun upsert(documents: Iterable<EmbeddedDocument>): Builder {
            documents.forEach { upsert(it.document, it.embedding) }
            return this
        }

        public fun build(): RetrievalDatabase {
            check(!consumed) { "RetrievalDatabase.Builder has already built a database" }
            val database = NativeBridge.buildRetrieval(handle.requireOpen("RetrievalDatabase.Builder"))
            consumed = true
            handle.close()
            return RetrievalDatabase(database)
        }

        override fun close(): Unit = handle.close()
    }

    public companion object {
        public fun load(path: Path): RetrievalDatabase =
            RetrievalDatabase(NativeBridge.loadRetrieval(path.toAbsolutePath().toString()))

        public fun validate(path: Path): Unit =
            NativeBridge.validateRetrieval(path.toAbsolutePath().toString())
    }
}
