package ai.retrievalkit.internal

import ai.retrievalkit.GraphMatch
import ai.retrievalkit.GraphNodeId
import ai.retrievalkit.GraphEdgeProvenance
import ai.retrievalkit.GraphPathEdge
import ai.retrievalkit.FieldPath
import ai.retrievalkit.GraphQueryTrace
import ai.retrievalkit.GraphSelectionSnapshot
import ai.retrievalkit.GraphTruncationReason

internal data class NativeGraphMatch(
    val nodeType: String,
    val recordId: String,
    val chunkKey: String?,
    val depth: Int,
    val path: Array<NativeGraphPathEdge>,
) {
    fun publicValue(): GraphMatch =
        GraphMatch(GraphNodeId(nodeType, recordId, chunkKey), depth, path.map { it.publicValue() })
}

internal data class NativeGraphPathEdge(
    val relationship: String,
    val sourceNodeType: String,
    val sourceRecordId: String,
    val sourceChunkKey: String?,
    val targetNodeType: String,
    val targetRecordId: String,
    val targetChunkKey: String?,
    val occurrenceOrdinal: Long,
    val schemaRuleIndex: Long,
    val provenanceRecordId: String,
    val sourceField: Array<String>?,
    val derivedInverse: Boolean,
    val builtIn: Boolean,
) {
    fun publicValue(): GraphPathEdge = GraphPathEdge(
        relationship = relationship,
        source = GraphNodeId(sourceNodeType, sourceRecordId, sourceChunkKey),
        target = GraphNodeId(targetNodeType, targetRecordId, targetChunkKey),
        occurrenceOrdinal = occurrenceOrdinal,
        provenance = GraphEdgeProvenance(
            schemaRuleIndex,
            provenanceRecordId,
            sourceField?.let { FieldPath(it.toList()) },
            derivedInverse,
            builtIn,
        ),
    )
}

internal data class NativeGraphSelection(
    val matches: Array<NativeGraphMatch>,
    val truncation: Int,
    val seedCount: Int,
    val visitedStates: Int,
    val traversedEdges: Int,
    val resultCount: Int,
    val diagnostics: Int,
) {
    fun publicValue(): GraphSelectionSnapshot = GraphSelectionSnapshot(
        matches = matches.map { it.publicValue() },
        truncated = if (truncation < 0) null else GraphTruncationReason.entries[truncation],
        trace = GraphQueryTrace(seedCount, visitedStates, traversedEdges, resultCount, diagnostics),
    )
}

internal data class NativeProjection(
    val recordIds: Array<String>,
    val chunkKeys: Array<String>,
    val sourceNodes: Int,
    val beforeFilter: Int,
    val afterFilter: Int,
)
