"""Python bindings for VectorKit.

VectorKit expects caller-provided embeddings. The Python package stays thin:
it validates Python inputs, passes them to the Rust core, and returns
Rust-produced results.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence

from . import where
from ._native import (
    DimensionMismatchError,
    FilterError,
    Index,
    PersistenceError,
    UnsupportedFormatError,
    VectorKitError,
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
    RrfFusionTrace,
    SearchHit,
    SearchTrace,
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

    This helper is intentionally provider-based so VectorKit does not require a
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
    fusion: str = "weighted",
    vector_weight: float = 0.6,
    keyword_weight: float = 0.4,
    rrf_k: float = 60.0,
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
        fusion=fusion,
        vector_weight=vector_weight,
        keyword_weight=keyword_weight,
        rrf_k=rrf_k,
    )


__all__ = [
    "AddDocumentResult",
    "ChunkInput",
    "CompactionReport",
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
    "KeywordHit",
    "Metadata",
    "MetadataValue",
    "PersistenceError",
    "RrfFusionTrace",
    "SearchHit",
    "SearchTrace",
    "UnsupportedFormatError",
    "VectorKitError",
    "WeightedNormalizedFusionTrace",
    "hybrid_search_text",
    "search_text",
    "where",
]
