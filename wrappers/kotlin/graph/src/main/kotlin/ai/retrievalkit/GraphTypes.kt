package ai.retrievalkit

public sealed interface RecordValue {
    public data object Null : RecordValue
    public data class Text(public val value: String) : RecordValue
    public data class Integer(public val value: Long) : RecordValue
    public data class Decimal(public val value: Double) : RecordValue
    public data class Boolean(public val value: kotlin.Boolean) : RecordValue
    public data class ListValue(public val values: List<RecordValue>) : RecordValue
    public data class ObjectValue(public val values: Map<String, RecordValue>) : RecordValue
}

public data class Record(
    public val id: String,
    public val type: String,
    public val fields: Map<String, RecordValue> = emptyMap(),
    public val content: String? = null,
    public val metadata: Metadata = emptyMap(),
)

/** One record and its optional direct embedding for combined bulk ingestion. */
public data class GraphRecordInput(
    public val record: Record,
    public val embedding: FloatArray? = null,
) {
    override fun equals(other: Any?): kotlin.Boolean =
        other is GraphRecordInput &&
            record == other.record &&
            when {
                embedding == null -> other.embedding == null
                other.embedding == null -> false
                else -> embedding.contentEquals(other.embedding)
            }

    override fun hashCode(): Int = 31 * record.hashCode() + (embedding?.contentHashCode() ?: 0)
}

public data class FieldPath(public val segments: List<String>) {
    public constructor(vararg segments: String) : this(segments.toList())
}

public enum class Cardinality { ONE, OPTIONAL_ONE, MANY }
public enum class MissingTargetPolicy { ERROR, OMIT_EDGE }
public enum class DuplicateReferencePolicy { ERROR, DEDUPLICATE }

public data class RecordNodeSchema(
    public val recordType: String,
    public val nodeType: String,
    public val queryableFields: List<FieldPath> = emptyList(),
)

public data class RelationshipSchema(
    public val relationshipType: String,
    public val sourceNodeType: String,
    public val targetNodeType: String,
    public val sourceField: FieldPath,
    public val cardinality: Cardinality,
    public val missingTarget: MissingTargetPolicy = MissingTargetPolicy.ERROR,
    public val duplicateReferences: DuplicateReferencePolicy = DuplicateReferencePolicy.ERROR,
    public val allowSelfEdge: Boolean = false,
    public val inverseRelationship: String? = null,
)

public data class ChunkNodeSchema(
    public val nodeType: String,
    public val ownsRelationship: String,
    public val inverseRelationship: String? = null,
)

public data class GraphSchema(
    public val recordNodes: List<RecordNodeSchema>,
    public val relationships: List<RelationshipSchema> = emptyList(),
    public val chunkNodes: ChunkNodeSchema? = null,
)

public data class GraphNodeId(
    public val nodeType: String,
    public val recordId: String,
    public val chunkKey: String? = null,
)

public enum class GraphDirection { OUTGOING, INCOMING }

public data class GraphTraversal(
    public val relationship: String,
    public val direction: GraphDirection = GraphDirection.OUTGOING,
    public val minHops: Int = 1,
    public val maxHops: Int = 1,
)

public data class GraphQueryLimits(
    public val maxHops: Int = 8,
    public val maxVisited: Int = 100_000,
    public val maxResults: Int = 10_000,
    public val maxWorkingBytes: Int = 64 * 1024 * 1024,
)

public sealed interface GraphScalar {
    public data class Text(public val value: String) : GraphScalar
    public data class Integer(public val value: Long) : GraphScalar
    public data class Boolean(public val value: kotlin.Boolean) : GraphScalar
}

public sealed interface GraphSeed {
    public data class Nodes(public val nodes: List<GraphNodeId>) : GraphSeed
    public data class Equals(
        public val nodeType: String,
        public val field: FieldPath,
        public val values: List<GraphScalar>,
    ) : GraphSeed
}

public data class GraphQuery(
    public val seed: GraphSeed,
    public val traversals: List<GraphTraversal> = emptyList(),
    public val limits: GraphQueryLimits = GraphQueryLimits(),
)

public enum class GraphTruncationReason { MAX_HOPS, MAX_VISITED, MAX_RESULTS, MAX_WORKING_BYTES }

public data class GraphMatch(
    public val nodeId: GraphNodeId,
    public val depth: Int,
    public val path: List<GraphPathEdge>,
)

public data class GraphEdgeProvenance(
    public val schemaRuleIndex: Long,
    public val sourceRecordId: String,
    public val sourceField: FieldPath?,
    public val derivedInverse: Boolean,
    public val builtIn: Boolean,
)

public data class GraphPathEdge(
    public val relationship: String,
    public val source: GraphNodeId,
    public val target: GraphNodeId,
    public val occurrenceOrdinal: Long,
    public val provenance: GraphEdgeProvenance,
)

public data class GraphQueryTrace(
    public val seedCount: Int,
    public val visitedStates: Int,
    public val traversedEdges: Int,
    public val resultCount: Int,
    public val diagnostics: Int,
)

public data class GraphSelectionSnapshot(
    public val matches: List<GraphMatch>,
    public val truncated: GraphTruncationReason?,
    public val trace: GraphQueryTrace,
)

public data class ChunkIdentity(public val recordId: String, public val chunkKey: String)

public data class CandidateProjection(
    public val candidates: List<ChunkIdentity>,
    public val sourceNodes: Int,
    public val projectedChunksBeforeFilter: Int,
    public val projectedChunksAfterFilter: Int,
)

public open class GraphException(message: String) : RetrievalKitException(message)
public class InvalidGraphSchemaException(message: String) : GraphException(message)
public class InvalidGraphRecordException(message: String) : GraphException(message)
public class InvalidGraphQueryException(message: String) : GraphException(message)
public class GraphLimitException(message: String) : GraphException(message)
public class GraphPersistenceException(message: String) : GraphException(message)
