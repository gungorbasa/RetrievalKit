"""Public type shapes returned by the VectorKit Python API."""

from __future__ import annotations

from typing import Literal, TypeAlias, TypedDict


MetadataValue: TypeAlias = str | int | float | bool
Metadata: TypeAlias = dict[str, MetadataValue]


class AddDocumentResult(TypedDict):
    id: str
    chunk_ids: list[int]


class SearchTrace(TypedDict):
    vector_score: float | None
    keyword_score: float | None
    filter_matched: bool


class SearchHit(TypedDict):
    chunk_id: int
    document_id: str
    text: str
    metadata: Metadata
    score: float
    trace: SearchTrace


class KeywordHit(TypedDict):
    chunk_id: int
    document_id: str
    text: str
    metadata: Metadata
    score: float
    matched_terms: list[str]


class RrfFusionTrace(TypedDict):
    kind: Literal["rrf"]
    rrf_k: float


class WeightedNormalizedFusionTrace(TypedDict):
    kind: Literal["weighted_normalized"]
    vector_weight: float
    keyword_weight: float


HybridFusionTrace: TypeAlias = RrfFusionTrace | WeightedNormalizedFusionTrace


class HybridTrace(TypedDict):
    vector_rank: int | None
    keyword_rank: int | None
    normalized_vector_score: float | None
    normalized_keyword_score: float | None
    matched_terms: list[str]
    filter_matched: bool
    fusion: HybridFusionTrace


class HybridHit(TypedDict):
    chunk_id: int
    document_id: str
    text: str
    metadata: Metadata
    score: float
    vector_score: float | None
    keyword_score: float | None
    matched_terms: list[str]
    trace: HybridTrace


class FileSizeReport(TypedDict):
    manifest_bytes: int
    vectors_bytes: int
    chunks_bytes: int
    bm25_bytes: int
    tombstones_bytes: int
    total_bytes: int


__all__ = [
    "AddDocumentResult",
    "FileSizeReport",
    "HybridFusionTrace",
    "HybridHit",
    "HybridTrace",
    "KeywordHit",
    "Metadata",
    "MetadataValue",
    "RrfFusionTrace",
    "SearchHit",
    "SearchTrace",
    "WeightedNormalizedFusionTrace",
]
