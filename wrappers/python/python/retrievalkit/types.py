"""Public type shapes accepted and returned by the RetrievalKit Python API."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal, TypeAlias, TypedDict


@dataclass(frozen=True)
class TimestampMillis:
    """Metadata timestamp kept distinct from an ordinary integer."""

    value: int

    @property
    def __retrievalkit_timestamp_millis__(self) -> int:
        return self.value


MetadataValue: TypeAlias = str | int | float | bool | TimestampMillis
Metadata: TypeAlias = dict[str, MetadataValue]
Embedding: TypeAlias = Sequence[float]
RecordValue: TypeAlias = (
    None | bool | int | float | str | list["RecordValue"] | dict[str, "RecordValue"]
)
@dataclass(frozen=True)
class VectorIndexConfiguration:
    dimension: int
    metric: Literal["cosine", "dot_product"] = "cosine"
    encoding: Literal["f32", "f16", "bf16", "i8", "binary"] = "i8"


@dataclass(frozen=True)
class RetrievalConfiguration:
    semantic: VectorIndexConfiguration


class RecordRequired(TypedDict):
    id: str
    record_type: str


class Record(RecordRequired, total=False):
    fields: dict[str, RecordValue]
    metadata: Metadata
    content: str | None


class RecordChunkRequired(TypedDict):
    key: str
    text: str


class RecordChunk(RecordChunkRequired, total=False):
    metadata: Metadata


class RecordInputRequired(TypedDict):
    record: Record


class RecordInput(RecordInputRequired, total=False):
    chunks: list[RecordChunk]


class TextChunk(TypedDict):
    text: str
    start_byte: int
    end_byte: int


class ChunkInputRequired(TypedDict):
    text: str
    embedding: Embedding


class ChunkInput(ChunkInputRequired, total=False):
    metadata: Metadata


class DocumentInputRequired(TypedDict):
    id: str
    chunks: list[ChunkInput]


class DocumentInput(DocumentInputRequired, total=False):
    metadata: Metadata


FilterOperatorSpec = TypedDict(
    "FilterOperatorSpec",
    {
        "$eq": MetadataValue,
        "$ne": MetadataValue,
        "$in": list[MetadataValue],
        "$gte": MetadataValue,
        "$lte": MetadataValue,
        "$exists": bool,
    },
    total=False,
)
FilterCondition: TypeAlias = MetadataValue | FilterOperatorSpec
Filter: TypeAlias = dict[str, "FilterCondition | list[Filter]"]


class AddDocumentResult(TypedDict):
    id: str
    chunk_ids: list[int]


class SearchTrace(TypedDict):
    vector_score: float


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


class HybridTrace(TypedDict):
    alpha: float
    vector_rank: int | None
    keyword_rank: int | None
    normalized_vector_score: float | None
    normalized_keyword_score: float | None
    matched_terms: list[str]


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


class CompactionReport(TypedDict):
    chunks_before: int
    chunks_after: int
    chunks_removed: int
    estimated_bytes_before: int
    estimated_bytes_after: int
    estimated_bytes_reclaimed: int


__all__ = [
    "AddDocumentResult",
    "ChunkInput",
    "CompactionReport",
    "DocumentInput",
    "Embedding",
    "Filter",
    "FilterCondition",
    "FilterOperatorSpec",
    "FileSizeReport",
    "HybridHit",
    "HybridTrace",
    "KeywordHit",
    "Metadata",
    "MetadataValue",
    "Record",
    "RecordChunk",
    "RecordInput",
    "RecordValue",
    "RetrievalConfiguration",
    "SearchHit",
    "SearchTrace",
    "TextChunk",
    "TimestampMillis",
    "VectorIndexConfiguration",
]
