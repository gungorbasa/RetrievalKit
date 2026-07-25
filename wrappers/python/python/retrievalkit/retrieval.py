"""Pythonic capability API backed by Rust's RetrievalDatabase."""

from __future__ import annotations

import json
import math
from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path
from typing import Any, overload

from ._native import _RetrievalDatabase, _RetrievalDatabaseBuilder
from .types import (
    Document,
    Embedding,
    FileSizeReport,
    Filter,
    HybridHit,
    MetadataValue,
    RecordInput,
    RecordValue,
    RetrievalConfiguration,
    SearchHit,
    TimestampMillis,
)


class MissingEmbeddingError(ValueError):
    """A retrieval upsert omitted one or more chunk embeddings."""


class UnexpectedEmbeddingError(ValueError):
    """A retrieval upsert supplied embeddings for unknown chunk keys."""


class RetrievalDatabaseBuilder:
    """Build an immutable corpus with semantic and optional hybrid retrieval."""

    def __init__(
        self,
        *,
        corpus_id: str,
        retrieval: RetrievalConfiguration | None = None,
        metric: str = "cosine",
        encoding: str = "i8",
    ) -> None:
        if retrieval is not None:
            metric = retrieval.semantic.metric
            encoding = retrieval.semantic.encoding
        self._native = _RetrievalDatabaseBuilder(
            corpus_id,
            metric,
            encoding,
        )

    @overload
    def upsert(
        self,
        input: Document,
        *,
        embedding: Embedding,
    ) -> list[int]: ...

    @overload
    def upsert(
        self,
        input: RecordInput,
        *,
        embeddings: Mapping[str, Embedding],
    ) -> list[int]: ...

    def upsert(
        self,
        input: Document | RecordInput,
        *,
        embedding: Embedding | None = None,
        embeddings: Mapping[str, Embedding] | None = None,
    ) -> list[int]:
        """Insert a simple document or use the advanced keyed-record surface."""

        if isinstance(input, Document):
            if embedding is None:
                raise TypeError("Document upsert requires embedding=")
            if embeddings is not None:
                raise TypeError("Document upsert accepts embedding=, not embeddings=")
            return self._native.upsert_document(
                input.id,
                input.text,
                input.metadata,
                embedding,
            )
        if embedding is not None:
            raise TypeError("advanced RecordInput upsert accepts embeddings=")
        if embeddings is None:
            raise TypeError("advanced RecordInput upsert requires embeddings=")
        record_embeddings = {input["record"]["id"]: embeddings}
        return self._native.add(_records_json([input], record_embeddings))[0]

    @property
    def dimension(self) -> int | None:
        """The Rust-inferred dimension, or ``None`` before the first embedding."""

        return self._native.dimension

    def add(
        self,
        records: Iterable[RecordInput],
        *,
        embeddings: Mapping[str, Mapping[str, Embedding]],
    ) -> list[list[int]]:
        """Bulk upsert records using record ID then chunk key embedding maps."""

        materialized = list(records)
        return self._native.add(_records_json(materialized, embeddings))

    def build(self) -> RetrievalDatabase:
        return RetrievalDatabase(self._native.build())


class RetrievalQueries:
    """Semantic and hybrid queries for one retrieval database."""

    def __init__(self, native: _RetrievalDatabase) -> None:
        self._native = native

    def semantic_search(
        self,
        embedding: Sequence[float],
        *,
        limit: int = 10,
        where: Filter | None = None,
    ) -> list[SearchHit]:
        return self._native.semantic_search(embedding, limit=limit, where=where)

    def hybrid_search(
        self,
        text: str,
        embedding: Sequence[float],
        *,
        limit: int = 10,
        where: Filter | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        alpha: float = 0.6,
    ) -> list[HybridHit]:
        return self._native.hybrid_search(
            text,
            embedding,
            limit=limit,
            where=where,
            vector_candidates=vector_candidates,
            keyword_candidates=keyword_candidates,
            alpha=alpha,
        )


class RetrievalDatabase:
    """Immutable local database exposing a dedicated retrieval query view."""

    def __init__(self, native: _RetrievalDatabase) -> None:
        self._native = native
        self.retrieval = RetrievalQueries(native)

    @classmethod
    def load(cls, path: str | Path) -> RetrievalDatabase:
        return cls(_RetrievalDatabase.load(Path(path)))

    @staticmethod
    def validate(path: str | Path) -> None:
        _RetrievalDatabase.validate(Path(path))

    def save(self, path: str | Path) -> FileSizeReport:
        return self._native.save(Path(path))

    @property
    def closed(self) -> bool:
        return bool(self._native.closed)

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> RetrievalDatabase:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


def _records_json(
    records: list[RecordInput],
    embeddings: Mapping[str, Mapping[str, Embedding]],
) -> str:
    record_ids = {input["record"]["id"] for input in records}
    unexpected_records = sorted(set(embeddings).difference(record_ids))
    if unexpected_records:
        raise UnexpectedEmbeddingError(
            "embeddings contain unknown record IDs: " + ", ".join(unexpected_records)
        )
    return json.dumps(
        [
            _record_batch(input, embeddings.get(input["record"]["id"], {}))
            for input in records
        ]
    )


def _record_batch(
    input: RecordInput,
    embeddings: Mapping[str, Embedding],
) -> dict[str, Any]:
    record = input["record"]
    chunks = input.get("chunks", [])
    expected = {chunk["key"] for chunk in chunks}
    actual = set(embeddings)
    missing = sorted(expected.difference(actual))
    if missing:
        raise MissingEmbeddingError(
            "missing embeddings for chunk keys: " + ", ".join(missing)
        )
    unexpected = sorted(actual.difference(expected))
    if unexpected:
        raise UnexpectedEmbeddingError(
            "embeddings contain unknown chunk keys: " + ", ".join(unexpected)
        )
    return {
        "record": {
            "id": record["id"],
            "record_type": record["record_type"],
            "fields": {
                key: _encode_record_value(value)
                for key, value in record.get("fields", {}).items()
            },
            "content": record.get("content"),
        },
        "projected_metadata": {
            key: _encode_metadata_value(value)
            for key, value in record.get("metadata", {}).items()
        },
        "chunks": [
            {
                "key": chunk["key"],
                "text": chunk["text"],
                "embedding": list(embeddings[chunk["key"]]),
                "metadata": {
                    key: _encode_metadata_value(value)
                    for key, value in chunk.get("metadata", {}).items()
                },
            }
            for chunk in chunks
        ],
    }


def _encode_metadata_value(value: MetadataValue) -> dict[str, Any]:
    if isinstance(value, TimestampMillis):
        return {"TimestampMillis": value.value}
    if isinstance(value, bool):
        return {"Boolean": value}
    if isinstance(value, int):
        return {"Integer": value}
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("metadata floats must be finite")
        return {"Float": value}
    if isinstance(value, str):
        return {"String": value}
    raise TypeError(f"unsupported metadata value: {type(value).__name__}")


def _encode_record_value(value: RecordValue) -> Any:
    if value is None:
        return "Null"
    if isinstance(value, bool):
        return {"Bool": value}
    if isinstance(value, int):
        return {"I64": value}
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("record floats must be finite")
        return {"F64": value}
    if isinstance(value, str):
        return {"String": value}
    if isinstance(value, list):
        return {"List": [_encode_record_value(item) for item in value]}
    if isinstance(value, dict):
        return {"Map": {key: _encode_record_value(item) for key, item in value.items()}}
    raise TypeError(f"unsupported record value: {type(value).__name__}")


__all__ = [
    "MissingEmbeddingError",
    "RetrievalDatabase",
    "RetrievalDatabaseBuilder",
    "RetrievalQueries",
    "UnexpectedEmbeddingError",
]
