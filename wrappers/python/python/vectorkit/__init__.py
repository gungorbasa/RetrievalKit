"""Python bindings for VectorKit.

VectorKit expects caller-provided embeddings. The Python package stays thin:
it validates Python inputs, passes them to the Rust core, and returns
Rust-produced results.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import Any

from ._native import (
    DimensionMismatchError,
    FilterError,
    Index,
    PersistenceError,
    UnsupportedFormatError,
    VectorKitError,
)
from . import where


EmbeddingProvider = Callable[[Sequence[str]], Sequence[Sequence[float]]]


def search_text(
    index: Index,
    text: str,
    *,
    embed: EmbeddingProvider,
    limit: int = 10,
    where: Any | None = None,
) -> list[dict[str, Any]]:
    """Embed one query string and search an index.

    This helper is intentionally provider-based so VectorKit does not require a
    specific embedding model or network dependency.
    """

    embeddings = embed([text])
    if len(embeddings) != 1:
        raise ValueError("embedding provider must return exactly one query embedding")
    return index.search(embeddings[0], limit=limit, where=where)


__all__ = [
    "DimensionMismatchError",
    "EmbeddingProvider",
    "FilterError",
    "Index",
    "PersistenceError",
    "UnsupportedFormatError",
    "VectorKitError",
    "search_text",
    "where",
]
