package ai.retrievalkit

/** Vector scoring used by the Rust exact-search engine. */
public enum class VectorMetric {
    DOT_PRODUCT,
    COSINE,
}

/** On-disk and in-memory vector representation. Query vectors always remain FloatArray values. */
public enum class VectorEncoding {
    F32,
    F16,
    BF16,
    I8_SCALAR_QUANTIZED,
}

public sealed interface MetadataValue {
    public data class Text(public val value: String) : MetadataValue
    public data class Integer(public val value: Long) : MetadataValue
    public data class Decimal(public val value: Double) : MetadataValue
    public data class Boolean(public val value: kotlin.Boolean) : MetadataValue
    public data class TimestampMillis(public val value: Long) : MetadataValue
}

public typealias Metadata = Map<String, MetadataValue>

public data class Document(
    public val id: String,
    public val text: String,
    public val metadata: Metadata = emptyMap(),
)

public data class EmbeddedDocument(
    public val document: Document,
    public val embedding: FloatArray,
) {
    override fun equals(other: Any?): kotlin.Boolean =
        other is EmbeddedDocument && document == other.document && embedding.contentEquals(other.embedding)

    override fun hashCode(): Int = 31 * document.hashCode() + embedding.contentHashCode()
}

public sealed interface Filter {
    public data class Equals(public val field: String, public val value: MetadataValue) : Filter
    public data class NotEquals(public val field: String, public val value: MetadataValue) : Filter
    public data class In(public val field: String, public val values: List<MetadataValue>) : Filter
    public data class Range(
        public val field: String,
        public val lower: MetadataValue? = null,
        public val upper: MetadataValue? = null,
    ) : Filter
    public data class Exists(public val field: String) : Filter
    public data class All(public val filters: List<Filter>) : Filter
    public data class AnyOf(public val filters: List<Filter>) : Filter
}

public data class SearchTrace(public val vectorScore: Float)

public data class SearchHit(
    public val documentId: String,
    public val recordId: String,
    public val text: String,
    public val score: Float,
    public val metadata: Metadata,
    public val trace: SearchTrace,
)

public data class HybridTrace(
    public val vectorRank: Int?,
    public val keywordRank: Int?,
    public val normalizedVectorScore: Float?,
    public val normalizedKeywordScore: Float?,
    public val matchedTerms: List<String>,
    public val alpha: Float,
)

public data class HybridHit(
    public val documentId: String,
    public val recordId: String,
    public val text: String,
    public val score: Float,
    public val vectorScore: Float?,
    public val keywordScore: Float?,
    public val metadata: Metadata,
    public val trace: HybridTrace,
)

public data class PersistenceReport(
    public val totalBytes: Long,
)

public data class CompactionReport(
    public val chunksBefore: Long,
    public val chunksAfter: Long,
    public val chunksRemoved: Long,
    public val estimatedBytesReclaimed: Long,
)

public open class RetrievalKitException(message: String, cause: Throwable? = null) :
    RuntimeException(message, cause)

public class InvalidIdentityException(message: String) : RetrievalKitException(message)
public class InvalidDimensionException(message: String) : RetrievalKitException(message)
public class MissingEmbeddingException(message: String) : RetrievalKitException(message)
public class InvalidFilterException(message: String) : RetrievalKitException(message)
public class InvalidQueryException(message: String) : RetrievalKitException(message)
public class PersistenceException(message: String) : RetrievalKitException(message)
public class CorruptIndexException(message: String) : RetrievalKitException(message)
public class StaleSelectionException(message: String) : RetrievalKitException(message)
public class ClosedResourceException(message: String) : RetrievalKitException(message)
public class NativeLibraryException(message: String, cause: Throwable? = null) :
    RetrievalKitException(message, cause)
