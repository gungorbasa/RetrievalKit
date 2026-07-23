from __future__ import annotations

from pathlib import Path

from .types import (
    AddDocumentResult,
    CompactionReport,
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

class RetrievalKitError(Exception): ...
class DimensionMismatchError(RetrievalKitError): ...
class PersistenceError(RetrievalKitError): ...
class FilterError(RetrievalKitError): ...
class UnsupportedFormatError(RetrievalKitError): ...
class CorruptIndexError(RetrievalKitError): ...
class InvalidIdentityError(RetrievalKitError): ...
class RetrievalCapabilityUnavailableError(RetrievalKitError): ...

class _RetrievalDatabaseBuilder:
    def __init__(
        self,
        dimension: int,
        corpus_id: str,
        metric: str = "cosine",
        encoding: str = "i8",
    ) -> None: ...
    def add(self, records_json: str) -> list[list[int]]: ...
    def build(self) -> _RetrievalDatabase: ...

class _RetrievalDatabase:
    @staticmethod
    def load(path: str | Path) -> _RetrievalDatabase: ...
    @staticmethod
    def validate(path: str | Path) -> None: ...
    def save(self, path: str | Path) -> FileSizeReport: ...
    def semantic_search(
        self,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
    ) -> list[SearchHit]: ...
    def hybrid_search(
        self,
        text: str,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        alpha: float = 0.6,
    ) -> list[HybridHit]: ...
    @property
    def closed(self) -> bool: ...
    def close(self) -> None: ...

class Index:
    def __init__(
        self,
        dimension: int,
        metric: str = "cosine",
        encoding: str = "f32",
    ) -> None: ...
    @staticmethod
    def load(path: str | Path) -> Index: ...
    @staticmethod
    def validate(path: str | Path) -> None: ...
    @property
    def dimension(self) -> int: ...
    @property
    def active_chunk_count(self) -> int: ...
    @property
    def total_chunk_count(self) -> int: ...
    @property
    def tombstoned_chunk_count(self) -> int: ...
    def add(self, documents: list[DocumentInput]) -> list[AddDocumentResult]: ...
    def delete_document(self, document_id: str) -> int: ...
    def compact(self) -> CompactionReport: ...
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
        alpha: float = 0.6,
    ) -> list[HybridHit]: ...
    def save(
        self,
        path: str | Path,
        *,
        include_bm25: bool = True,
    ) -> FileSizeReport: ...
