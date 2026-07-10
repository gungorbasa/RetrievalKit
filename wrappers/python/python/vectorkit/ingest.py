"""Text ingestion helpers backed by the shared Rust implementation."""

from __future__ import annotations

from ._native import chunk_text
from .types import TextChunk


class RustTextChunker:
    """Configured wrapper around VectorKit's shared Rust chunker."""

    def __init__(
        self,
        *,
        max_characters: int,
        overlap_characters: int = 0,
        strategy: str = "sentence",
    ) -> None:
        self.max_characters = max_characters
        self.overlap_characters = overlap_characters
        self.strategy = strategy

    def chunks(self, text: str) -> list[TextChunk]:
        return chunk_text(
            text,
            max_characters=self.max_characters,
            overlap_characters=self.overlap_characters,
            strategy=self.strategy,
        )


__all__ = ["RustTextChunker", "TextChunk", "chunk_text"]
