"""Python bindings for RetrievalKit.

RetrievalKit expects caller-provided embeddings. The Python package stays thin:
it validates Python inputs, passes them to the Rust core, and returns
Rust-produced results.
"""

from __future__ import annotations

import sys
from collections.abc import Callable, Sequence

if "retrievalkit_graph._native" in sys.modules:
    raise ImportError(
        "retrievalkit and retrievalkit_graph are mutually exclusive native "
        "distributions; use one capability package per process"
    )

from . import where
from ._native import (
    CorruptIndexError,
    DimensionMismatchError,
    FilterError,
    Index,
    InvalidIdentityError,
    PersistenceError,
    RetrievalCapabilityUnavailableError,
    RetrievalKitError,
    UnsupportedFormatError,
)
from .retrieval import (
    MissingEmbeddingError,
    RetrievalDatabase,
    RetrievalDatabaseBuilder,
    RetrievalQueries,
    UnexpectedEmbeddingError,
)
from .types import (
    AddDocumentResult,
    ChunkInput,
    CompactionReport,
    DocumentInput,
    Embedding,
    FileSizeReport,
    Filter,
    FilterCondition,
    FilterOperatorSpec,
    HybridFusionTrace,
    HybridHit,
    HybridTrace,
    KeywordHit,
    Metadata,
    MetadataValue,
    Record,
    RecordChunk,
    RecordInput,
    RecordValue,
    RetrievalConfiguration,
    RrfFusionTrace,
    SearchHit,
    SearchTrace,
    TimestampMillis,
    VectorIndexConfiguration,
    WeightedNormalizedFusionTrace,
)

EmbeddingProvider = Callable[[Sequence[str]], Sequence[Sequence[float]]]


def search_text(
    index: Index,
    text: str,
    *,
    embed: EmbeddingProvider,
    limit: int = 10,
    where: Filter | None = None,
) -> list[SearchHit]:
    """Embed one query string and search an index.

    This helper is intentionally provider-based so RetrievalKit does not require a
    specific embedding model or network dependency.
    """

    embeddings = embed([text])
    if len(embeddings) != 1:
        raise ValueError("embedding provider must return exactly one query embedding")
    return index.search(embeddings[0], limit=limit, where=where)


def hybrid_search_text(
    index: Index,
    text: str,
    *,
    embed: EmbeddingProvider,
    limit: int = 10,
    where: Filter | None = None,
    vector_candidates: int | None = None,
    keyword_candidates: int | None = None,
    alpha: float = 0.6,
) -> list[HybridHit]:
    """Embed one query string and run hybrid vector + keyword search."""

    embeddings = embed([text])
    if len(embeddings) != 1:
        raise ValueError("embedding provider must return exactly one query embedding")
    return index.hybrid_search(
        text,
        embeddings[0],
        limit=limit,
        where=where,
        vector_candidates=vector_candidates,
        keyword_candidates=keyword_candidates,
        alpha=alpha,
    )


__all__ = [
    "AddDocumentResult",
    "ChunkInput",
    "CompactionReport",
    "CorruptIndexError",
    "DimensionMismatchError",
    "DocumentInput",
    "Embedding",
    "EmbeddingProvider",
    "Filter",
    "FilterCondition",
    "FilterOperatorSpec",
    "FileSizeReport",
    "FilterError",
    "HybridFusionTrace",
    "HybridHit",
    "HybridTrace",
    "Index",
    "InvalidIdentityError",
    "KeywordHit",
    "Metadata",
    "MetadataValue",
    "MissingEmbeddingError",
    "PersistenceError",
    "RrfFusionTrace",
    "Record",
    "RecordChunk",
    "RecordInput",
    "RecordValue",
    "RetrievalConfiguration",
    "RetrievalCapabilityUnavailableError",
    "RetrievalDatabase",
    "RetrievalDatabaseBuilder",
    "RetrievalQueries",
    "SearchHit",
    "SearchTrace",
    "TimestampMillis",
    "UnsupportedFormatError",
    "UnexpectedEmbeddingError",
    "VectorIndexConfiguration",
    "RetrievalKitError",
    "WeightedNormalizedFusionTrace",
    "hybrid_search_text",
    "search_text",
    "where",
]
