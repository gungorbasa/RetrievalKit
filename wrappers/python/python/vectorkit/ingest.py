"""Text ingestion helpers backed by the shared Rust implementation."""

from __future__ import annotations

from ._native import chunk_text
from .types import TextChunk

__all__ = ["TextChunk", "chunk_text"]
