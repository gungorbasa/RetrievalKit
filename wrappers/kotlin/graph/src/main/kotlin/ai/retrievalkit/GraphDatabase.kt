package ai.retrievalkit

import ai.retrievalkit.internal.NativeBridge
import ai.retrievalkit.internal.NativeHandle
import java.nio.file.Path

public class GraphSelection internal constructor(nativeHandle: Long) : AutoCloseable {
    internal val handle = NativeHandle(nativeHandle, NativeBridge::closeHandle)

    public val snapshot: GraphSelectionSnapshot
        get() = NativeBridge.graphSelection(handle.requireOpen("GraphSelection")).publicValue()

    override fun close(): Unit = handle.close()
}

/**
 * Graph-only database. Its native aggregate contains no vector or BM25 capability.
 */
public class GraphDatabase private constructor(nativeHandle: Long) : AutoCloseable {
    private val handle = NativeHandle(nativeHandle, NativeBridge::closeHandle)

    public fun query(query: GraphQuery): GraphSelection =
        GraphSelection(NativeBridge.graphQuery(handle.requireOpen("GraphDatabase"), query))

    public fun projectCandidates(
        selection: GraphSelection,
        filter: Filter? = null,
    ): CandidateProjection {
        val native = NativeBridge.projectCandidates(
            handle.requireOpen("GraphDatabase"),
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
                handle.requireOpen("GraphDatabase"),
                path.toAbsolutePath().toString(),
                false,
            ),
        )

    override fun close(): Unit = handle.close()

    public class Builder(corpusId: String, schema: GraphSchema) : AutoCloseable {
        private val handle = NativeHandle(
            NativeBridge.createGraphBuilder(corpusId, schema),
            NativeBridge::closeHandle,
        )
        private var consumed = false

        public fun upsert(record: Record): Builder {
            NativeBridge.graphBuilderUpsert(handle.requireOpen("GraphDatabase.Builder"), record)
            return this
        }

        public fun upsert(records: Iterable<Record>): Builder {
            records.forEach(::upsert)
            return this
        }

        public fun build(): GraphDatabase {
            check(!consumed) { "GraphDatabase.Builder has already built a database" }
            val database = NativeBridge.buildGraph(handle.requireOpen("GraphDatabase.Builder"))
            consumed = true
            handle.close()
            return GraphDatabase(database)
        }

        override fun close(): Unit = handle.close()
    }

    public companion object {
        public fun load(path: Path): GraphDatabase =
            GraphDatabase(NativeBridge.loadGraph(path.toAbsolutePath().toString(), false))

        public fun validate(path: Path): Unit =
            NativeBridge.validateGraph(path.toAbsolutePath().toString(), false)
    }
}
