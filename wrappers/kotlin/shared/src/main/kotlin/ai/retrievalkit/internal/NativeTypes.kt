package ai.retrievalkit.internal

import ai.retrievalkit.Document
import ai.retrievalkit.HybridHit
import ai.retrievalkit.HybridTrace
import ai.retrievalkit.Metadata
import ai.retrievalkit.MetadataValue
import ai.retrievalkit.SearchHit
import ai.retrievalkit.SearchTrace

internal data class NativeMetadataEntry(
    val key: String,
    val type: Int,
    val stringValue: String?,
    val longValue: Long,
    val doubleValue: Double,
    val booleanValue: Boolean,
) {
    fun toValue(): MetadataValue = when (type) {
        0 -> MetadataValue.Text(requireNotNull(stringValue))
        1 -> MetadataValue.Integer(longValue)
        2 -> MetadataValue.Decimal(doubleValue)
        3 -> MetadataValue.Boolean(booleanValue)
        4 -> MetadataValue.TimestampMillis(longValue)
        else -> error("native metadata type $type is unsupported")
    }
}

internal fun Metadata.toNative(): Array<NativeMetadataEntry> = entries
    .sortedBy(Map.Entry<String, MetadataValue>::key)
    .map { (key, value) ->
        when (value) {
            is MetadataValue.Text -> NativeMetadataEntry(key, 0, value.value, 0, 0.0, false)
            is MetadataValue.Integer -> NativeMetadataEntry(key, 1, null, value.value, 0.0, false)
            is MetadataValue.Decimal -> NativeMetadataEntry(key, 2, null, 0, value.value, false)
            is MetadataValue.Boolean -> NativeMetadataEntry(key, 3, null, 0, 0.0, value.value)
            is MetadataValue.TimestampMillis -> NativeMetadataEntry(key, 4, null, value.value, 0.0, false)
        }
    }.toTypedArray()

internal fun Array<NativeMetadataEntry>.toMetadata(): Metadata =
    associate { it.key to it.toValue() }

internal data class NativeDocument(
    val id: String,
    val text: String,
    val metadata: Array<NativeMetadataEntry>,
)

internal fun Document.toNative(): NativeDocument = NativeDocument(id, text, metadata.toNative())

internal data class NativeSearchHit(
    val recordId: String,
    val chunkKey: String,
    val text: String,
    val score: Float,
    val vectorScore: Float,
    val metadata: Array<NativeMetadataEntry>,
) {
    fun publicValue(): SearchHit = SearchHit(
        documentId = chunkKey,
        recordId = recordId,
        text = text,
        score = score,
        metadata = metadata.toMetadata(),
        trace = SearchTrace(vectorScore),
    )
}

internal data class NativeHybridHit(
    val recordId: String,
    val chunkKey: String,
    val text: String,
    val score: Float,
    val vectorScore: Float?,
    val keywordScore: Float?,
    val metadata: Array<NativeMetadataEntry>,
    val vectorRank: Int?,
    val keywordRank: Int?,
    val normalizedVectorScore: Float?,
    val normalizedKeywordScore: Float?,
    val matchedTerms: Array<String>,
    val alpha: Float,
) {
    fun publicValue(): HybridHit = HybridHit(
        documentId = chunkKey,
        recordId = recordId,
        text = text,
        score = score,
        vectorScore = vectorScore,
        keywordScore = keywordScore,
        metadata = metadata.toMetadata(),
        trace = HybridTrace(
            vectorRank = vectorRank,
            keywordRank = keywordRank,
            normalizedVectorScore = normalizedVectorScore,
            normalizedKeywordScore = normalizedKeywordScore,
            matchedTerms = matchedTerms.toList(),
            alpha = alpha,
        ),
    )
}
