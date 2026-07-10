from __future__ import annotations

from pathlib import Path

from .types import (
    AddDocumentResult,
    DocumentInput,
    Embedding,
    FileSizeReport,
    Filter,
    HybridHit,
    KeywordHit,
    SearchHit,
    TextChunk,
)

def chunk_text(
    text: str,
    *,
    max_characters: int,
    overlap_characters: int = 0,
    strategy: str = "sentence",
) -> list[TextChunk]: ...

class VectorKitError(Exception): ...


class DimensionMismatchError(VectorKitError): ...


class PersistenceError(VectorKitError): ...


class FilterError(VectorKitError): ...


class UnsupportedFormatError(VectorKitError): ...


class Index:
    def __init__(
        self,
        dimension: int,
        metric: str = "cosine",
        encoding: str = "f32",
    ) -> None: ...

    @staticmethod
    def load(path: str | Path) -> Index: ...

    @property
    def dimension(self) -> int: ...

    @property
    def active_chunk_count(self) -> int: ...

    @property
    def total_chunk_count(self) -> int: ...

    def add(self, documents: list[DocumentInput]) -> list[AddDocumentResult]: ...

    def delete_document(self, document_id: str) -> int: ...

    def search(
        self,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
    ) -> list[SearchHit]: ...

    def keyword_search(
        self,
        text: str,
        *,
        limit: int = 10,
        where: Filter | None = None,
    ) -> list[KeywordHit]: ...

    def hybrid_search(
        self,
        text: str,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        fusion: str = "weighted",
        vector_weight: float = 0.6,
        keyword_weight: float = 0.4,
        rrf_k: float = 60.0,
    ) -> list[HybridHit]: ...

    def save(
        self,
        path: str | Path,
        *,
        include_bm25: bool = True,
    ) -> FileSizeReport: ...
