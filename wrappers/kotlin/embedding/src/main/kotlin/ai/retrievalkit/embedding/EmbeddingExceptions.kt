package ai.retrievalkit.embedding

/**
 * Base class for failures reported by the optional embedding provider.
 *
 * Retrieval and database exceptions intentionally do not inherit from this
 * hierarchy: model acquisition and inference are a separate capability.
 */
open class EmbeddingException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    RuntimeException(message, cause)

class InvalidEmbeddingInputException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class ModelAcquisitionException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class ModelIntegrityException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class ModelLoadException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class EmbeddingInferenceException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class NativeLibraryException @JvmOverloads constructor(message: String, cause: Throwable? = null) :
    EmbeddingException(message, cause)

class ClosedEmbedderException(message: String = "the embedder is closed") :
    EmbeddingException(message)
