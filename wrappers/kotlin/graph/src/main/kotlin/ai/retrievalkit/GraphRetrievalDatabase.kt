package ai.retrievalkit

import ai.retrievalkit.internal.NativeBridge
import ai.retrievalkit.internal.NativeHandle
import ai.retrievalkit.internal.toNative
import java.nio.file.Path

/**
 * One canonical corpus with graph and exact/BM25 retrieval capabilities.
 *
 * All methods block and native access to an instance is serialized. Use an application-selected
 * dispatcher or executor when calling from Android UI code.
 */
public class GraphRetrievalDatabase private constructor(nativeHandle: Long) : AutoCloseable {
    private val handle = NativeHandle(nativeHandle, NativeBridge::closeHandle)

    public val dimension: Int
        get() = NativeBridge.retrievalDimension(handle.requireOpen("GraphRetrievalDatabase"))

    public fun query(query: GraphQuery): GraphSelection =
        GraphSelection(NativeBridge.graphQuery(handle.requireOpen("GraphRetrievalDatabase"), query))

    public fun search(
        embedding: FloatArray,
        limit: Int = 10,
        filter: Filter? = null,
        within: GraphSelection? = null,
    ): List<SearchHit> = NativeBridge.semanticSearch(
        handle.requireOpen("GraphRetrievalDatabase"),
        embedding,
        limit,
        filter,
        within?.handle?.requireOpen("GraphSelection") ?: 0,
    ).map { it.publicValue() }

    public fun search(
        text: String,
        embedding: FloatArray? = null,
        limit: Int = 10,
        alpha: Float = 0.6f,
        filter: Filter? = null,
        within: GraphSelection? = null,
        vectorCandidates: Int = 50,
        keywordCandidates: Int = 50,
    ): List<HybridHit> = NativeBridge.hybridSearch(
        handle.requireOpen("GraphRetrievalDatabase"),
        text,
        embedding,
        limit,
        alpha,
        filter,
        vectorCandidates,
        keywordCandidates,
        within?.handle?.requireOpen("GraphSelection") ?: 0,
    ).map { it.publicValue() }

    public fun search(
        text: String,
        limit: Int = 10,
        filter: Filter? = null,
        within: GraphSelection? = null,
    ): List<KeywordHit> = NativeBridge.keywordSearch(
        handle.requireOpen("GraphRetrievalDatabase"),
        text,
        limit,
        filter,
        within?.handle?.requireOpen("GraphSelection") ?: 0,
    ).map { it.publicValue() }

    public fun projectCandidates(
        selection: GraphSelection,
        filter: Filter? = null,
    ): CandidateProjection {
        val native = NativeBridge.projectCandidates(
            handle.requireOpen("GraphRetrievalDatabase"),
            selection.handle.requireOpen("GraphSelection"),
            filter,
        )
        return CandidateProjection(
            native.recordIds.indices.map {
                ChunkIdentity(native.recordIds[it], native.chunkKeys[it])
            },
            native.sourceNodes,
            native.beforeFilter,
            native.afterFilter,
        )
    }

    public fun save(path: Path): PersistenceReport =
        PersistenceReport(
            NativeBridge.saveGraph(
                handle.requireOpen("GraphRetrievalDatabase"),
                path.toAbsolutePath().toString(),
                true,
            ),
        )

    override fun close(): Unit = handle.close()

    public class Builder(
        corpusId: String,
        schema: GraphSchema,
        metric: VectorMetric = VectorMetric.COSINE,
        encoding: VectorEncoding = VectorEncoding.I8_SCALAR_QUANTIZED,
        bm25: Bm25Configuration = Bm25Configuration(),
    ) : AutoCloseable {
        private val handle = NativeHandle(
            NativeBridge.createGraphRetrievalBuilder(
                corpusId,
                schema,
                metric.ordinal,
                encoding.ordinal,
                bm25.k1,
                bm25.b,
                bm25.stopWords.toTypedArray(),
            ),
            NativeBridge::closeHandle,
        )
        private var consumed = false

        public fun upsert(record: Record): Builder {
            NativeBridge.graphRetrievalBuilderUpsert(
                handle.requireOpen("GraphRetrievalDatabase.Builder"),
                record,
                null,
                null,
            )
            return this
        }

        public fun upsert(record: Record, embedding: FloatArray): Builder {
            NativeBridge.graphRetrievalBuilderUpsert(
                handle.requireOpen("GraphRetrievalDatabase.Builder"),
                record,
                embedding,
                null,
            )
            return this
        }

        public fun upsert(record: Record, documents: List<EmbeddedDocument>): Builder {
            NativeBridge.graphRetrievalBuilderUpsert(
                handle.requireOpen("GraphRetrievalDatabase.Builder"),
                record,
                null,
                documents.toTypedArray(),
            )
            return this
        }

        public fun upsert(records: Iterable<GraphRecordInput>): Builder {
            records.forEach { input ->
                input.embedding?.let { upsert(input.record, it) } ?: upsert(input.record)
            }
            return this
        }

        internal fun upsertFixtureChunk(
            record: Record,
            chunkKey: String,
            text: String,
            embedding: FloatArray,
            metadata: Metadata = emptyMap(),
        ): Builder {
            NativeBridge.graphRetrievalBuilderUpsertFixtureChunk(
                handle.requireOpen("GraphRetrievalDatabase.Builder"),
                record,
                chunkKey,
                text,
                embedding,
                metadata.toNative(),
            )
            return this
        }

        public fun build(): GraphRetrievalDatabase {
            check(!consumed) { "GraphRetrievalDatabase.Builder has already built a database" }
            val database = NativeBridge.buildGraphRetrieval(
                handle.requireOpen("GraphRetrievalDatabase.Builder"),
            )
            consumed = true
            handle.close()
            return GraphRetrievalDatabase(database)
        }

        override fun close(): Unit = handle.close()
    }

    public companion object {
        public fun load(path: Path): GraphRetrievalDatabase =
            GraphRetrievalDatabase(NativeBridge.loadGraph(path.toAbsolutePath().toString(), true))

        public fun validate(path: Path): Unit =
            NativeBridge.validateGraph(path.toAbsolutePath().toString(), true)
    }
}
