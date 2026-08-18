"""Typed public shapes for the optional RetrievalKit graph capability."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Literal, TypeAlias, TypedDict


@dataclass(frozen=True)
class TimestampMillis:
    value: int

    @property
    def __retrievalkit_timestamp_millis__(self) -> int:
        return self.value


MetadataValue: TypeAlias = str | int | float | bool | TimestampMillis
Metadata: TypeAlias = dict[str, MetadataValue]
Embedding: TypeAlias = Sequence[float]
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


@dataclass(frozen=True)
class VectorIndexConfiguration:
    dimension: int | None = None
    metric: Literal["cosine", "dot_product"] = "cosine"
    encoding: Literal["f32", "f16", "bf16", "i8"] = "i8"


@dataclass(frozen=True)
class Bm25Configuration:
    k1: float = 1.2
    b: float = 0.75
    stop_words: tuple[str, ...] = ()


@dataclass(frozen=True)
class RetrievalConfiguration:
    semantic: VectorIndexConfiguration
    bm25: Bm25Configuration = field(default_factory=Bm25Configuration)


GraphValue: TypeAlias = (
    None | bool | int | float | str | list["GraphValue"] | dict[str, "GraphValue"]
)
GraphScalar: TypeAlias = bool | int | str
GraphCardinality: TypeAlias = Literal["one", "optional_one", "many"]
GraphMissingTargetPolicy: TypeAlias = Literal["error", "omit_edge"]
GraphDuplicatePolicy: TypeAlias = Literal["error", "deduplicate"]
GraphDirection: TypeAlias = Literal["outgoing", "incoming"]


@dataclass(frozen=True)
class GraphRecordNode:
    record_type: str
    node_type: str
    queryable_fields: list[str | list[str]] = field(default_factory=list)


@dataclass(frozen=True)
class GraphRelationship:
    relationship_type: str
    source_node_type: str
    target_node_type: str
    source_field: str | list[str]
    cardinality: GraphCardinality
    missing_target: GraphMissingTargetPolicy = "error"
    duplicate_references: GraphDuplicatePolicy = "error"
    allow_self_edge: bool = False
    inverse_relationship: str | None = None


@dataclass(frozen=True)
class GraphChunkNode:
    node_type: str
    owns_relationship: str
    inverse_relationship: str | None = None


@dataclass(frozen=True)
class GraphSchema:
    record_nodes: list[GraphRecordNode]
    relationships: list[GraphRelationship] = field(default_factory=list)
    chunk_nodes: GraphChunkNode | None = None


@dataclass(frozen=True)
class GraphNode:
    node_type: str
    record_id: str
    chunk_key: str | None = None


@dataclass(frozen=True)
class GraphTraversal:
    relationship: str
    direction: GraphDirection = "outgoing"
    min_hops: int = 1
    max_hops: int = 1


@dataclass(frozen=True)
class GraphQueryLimits:
    max_hops: int = 8
    max_visited: int = 100_000
    max_results: int = 10_000
    max_working_bytes: int = 64 * 1024 * 1024


@dataclass(frozen=True)
class GraphChunkIdentity:
    record_id: str
    chunk_key: str


@dataclass(frozen=True)
class GraphCandidateProjection:
    candidates: list[GraphChunkIdentity]
    source_nodes: int
    projected_chunks_before_filter: int
    projected_chunks_after_filter: int


class ChunkRequired(TypedDict):
    key: str
    text: str


class Chunk(ChunkRequired, total=False):
    metadata: Metadata


class RecordRequired(TypedDict):
    id: str
    record_type: str


class Record(RecordRequired, total=False):
    fields: dict[str, GraphValue]
    content: str | None
    metadata: Metadata


class RecordInputRequired(TypedDict):
    record: Record


class RecordInput(RecordInputRequired, total=False):
    chunks: list[Chunk]


GraphChunkInput: TypeAlias = Chunk
GraphRecordInput: TypeAlias = RecordInput


class GraphNodeDict(TypedDict):
    node_type: str
    record_id: str
    chunk_key: str | None


class GraphEdgeProvenance(TypedDict):
    schema_rule_index: int
    source_record_id: str
    source_field: list[str] | None
    derived_inverse: bool
    built_in: bool


class GraphPathEdge(TypedDict):
    relationship: str
    source: GraphNodeDict
    target: GraphNodeDict
    occurrence_ordinal: int
    provenance: GraphEdgeProvenance


class GraphMatch(TypedDict):
    node: GraphNodeDict
    depth: int
    path: list[GraphPathEdge]


class GraphQueryTrace(TypedDict):
    seed_count: int
    visited_states: int
    traversed_edges: int
    result_count: int
    diagnostics: int
    projected_chunk_count: int


class HydratedGraphRecord(TypedDict):
    id: str
    record_type: str
    fields: dict[str, GraphValue]
    content: str | None


class HydratedGraphChunk(TypedDict):
    chunk_id: int
    document_id: str
    text: str
    metadata: Metadata
    deleted: bool
    version: int


class GraphFileSizeReport(TypedDict):
    corpus_bytes: int
    schema_bytes: int
    graph_bytes: int


GraphSearchHit: TypeAlias = SearchHit
GraphKeywordHit: TypeAlias = KeywordHit
GraphHybridHit: TypeAlias = HybridHit


__all__ = [
    "Chunk",
    "Bm25Configuration",
    "GraphCardinality",
    "GraphCandidateProjection",
    "GraphChunkIdentity",
    "GraphChunkInput",
    "GraphChunkNode",
    "GraphDirection",
    "GraphDuplicatePolicy",
    "GraphEdgeProvenance",
    "GraphFileSizeReport",
    "GraphHybridHit",
    "GraphKeywordHit",
    "GraphMatch",
    "GraphMissingTargetPolicy",
    "GraphNode",
    "GraphNodeDict",
    "GraphPathEdge",
    "GraphQueryLimits",
    "GraphQueryTrace",
    "GraphRecordInput",
    "GraphRecordNode",
    "GraphRelationship",
    "GraphScalar",
    "GraphSchema",
    "GraphSearchHit",
    "GraphTraversal",
    "GraphValue",
    "HydratedGraphChunk",
    "HydratedGraphRecord",
    "Filter",
    "Metadata",
    "MetadataValue",
    "Record",
    "RecordInput",
    "RetrievalConfiguration",
    "TimestampMillis",
    "VectorIndexConfiguration",
]
