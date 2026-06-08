from __future__ import annotations

from pathlib import Path
from typing import Any


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

    def add(self, documents: list[dict[str, Any]]) -> list[dict[str, Any]]: ...

    def delete_document(self, document_id: str) -> int: ...

    def search(
        self,
        embedding: list[float],
        *,
        limit: int = 10,
        where: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]: ...

    def keyword_search(
        self,
        text: str,
        *,
        limit: int = 10,
        where: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]: ...

    def save(
        self,
        path: str | Path,
        *,
        include_bm25: bool = True,
    ) -> dict[str, int]: ...
