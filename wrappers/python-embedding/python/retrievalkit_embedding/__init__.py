"""Optional, local FP32 text embeddings for RetrievalKit."""

from ._native import (
    BUILD_MODE,
    EMBEDDING_DIMENSION,
    ONNX_RUNTIME_VERSION,
    ArtifactError,
    DownloadError,
    EmbeddingError,
    EmbeddingInputError,
    EmbeddingRuntimeError,
    ModelInfo,
    ModelUnavailableError,
    OnnxEmbedder,
)

__all__ = [
    "BUILD_MODE",
    "EMBEDDING_DIMENSION",
    "ONNX_RUNTIME_VERSION",
    "ArtifactError",
    "DownloadError",
    "EmbeddingError",
    "EmbeddingInputError",
    "EmbeddingRuntimeError",
    "ModelInfo",
    "ModelUnavailableError",
    "OnnxEmbedder",
]
