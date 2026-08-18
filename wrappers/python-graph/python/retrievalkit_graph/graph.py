"""Rust-backed graph and graph-retrieval databases."""

from __future__ import annotations

import json
import math
from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path
from typing import Any, cast, overload

from ._native import (
    GraphCancelledError,
    GraphError,
    GraphQueryError,
    GraphTimeoutError,
    InvalidGraphSchemaError,
    RetrievalCapabilityUnavailableError,
    StaleGraphSelectionError,
    _GraphCancellationToken,
    _GraphDatabase,
    _GraphDatabaseBuilder,
    _GraphRetrievalDatabase,
    _GraphRetrievalDatabaseBuilder,
    _GraphSelection,
)
from .graph_types import (
    Chunk,
    Embedding,
    Filter,
    GraphCandidateProjection,
    GraphChunkIdentity,
    GraphFileSizeReport,
    GraphHybridHit,
    GraphKeywordHit,
    GraphMatch,
    GraphNode,
    GraphQueryLimits,
    GraphQueryTrace,
    GraphRecordNode,
    GraphRelationship,
    GraphScalar,
    GraphSchema,
    GraphSearchHit,
    GraphTraversal,
    GraphValue,
    HydratedGraphChunk,
    HydratedGraphRecord,
    MetadataValue,
    Record,
    RecordInput,
    RetrievalConfiguration,
    TimestampMillis,
)


class GraphCancellationToken:
    """Thread-safe cooperative cancellation token for graph queries."""

    def __init__(self) -> None:
        self._native = _GraphCancellationToken()

    @property
    def cancelled(self) -> bool:
        return bool(self._native.cancelled)

    def cancel(self) -> None:
        self._native.cancel()


class MissingEmbeddingError(ValueError):
    """A retrieval upsert omitted one or more chunk embeddings."""


class UnexpectedEmbeddingError(ValueError):
    """A retrieval upsert supplied embeddings for unknown records or chunks."""


class GraphSelection:
    """Generation-bound result of one graph query."""

    def __init__(self, native: _GraphSelection) -> None:
        self._native = native
        self._data = cast(dict[str, Any], native.materialize())

    @property
    def matches(self) -> list[GraphMatch]:
        return cast(list[GraphMatch], self._data["matches"])

    @property
    def trace(self) -> GraphQueryTrace:
        return cast(GraphQueryTrace, self._data["trace"])

    @property
    def truncated(self) -> str | None:
        return cast(str | None, self._data["truncated"])

    @property
    def projected_chunk_count(self) -> int:
        return int(self._native.projected_chunk_count)

    @property
    def closed(self) -> bool:
        return bool(self._native.closed)

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> GraphSelection:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


class GraphDatabaseBuilder:
    """Collect records before building an immutable graph-only database."""

    def __init__(self, *, corpus_id: str, schema: GraphSchema) -> None:
        self._native = _GraphDatabaseBuilder(corpus_id, _schema_json(schema))

    @overload
    def upsert(self, input: Record) -> list[int]: ...

    @overload
    def upsert(self, input: RecordInput) -> list[int]: ...

    def upsert(self, input: Record | RecordInput) -> list[int]:
        if "record" in input:
            advanced = cast(RecordInput, input)
            return self._native.add(_records_json([advanced]))[0]
        return self._native.upsert_record(_record_json(input))

    def add(self, records: Iterable[RecordInput]) -> list[list[int]]:
        return self._native.add(_records_json(records))

    def build(self) -> GraphDatabase:
        return GraphDatabase(self._native.build())


class GraphRetrievalDatabaseBuilder:
    """Collect records before building a combined graph and retrieval database."""

    def __init__(
        self,
        *,
        corpus_id: str,
        graph: GraphSchema,
        retrieval: RetrievalConfiguration | None = None,
        metric: str = "cosine",
        encoding: str = "i8",
    ) -> None:
        if retrieval is not None:
            metric = retrieval.semantic.metric
            encoding = retrieval.semantic.encoding
        bm25 = retrieval.bm25 if retrieval is not None else None
        self._native = _GraphRetrievalDatabaseBuilder(
            corpus_id,
            _schema_json(graph),
            metric,
            encoding,
            1.2 if bm25 is None else bm25.k1,
            0.75 if bm25 is None else bm25.b,
            [] if bm25 is None else list(bm25.stop_words),
        )

    @overload
    def upsert(
        self,
        input: Record,
        *,
        embedding: Embedding | None = None,
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
        input: Record | RecordInput,
        *,
        embedding: Embedding | None = None,
        embeddings: Mapping[str, Embedding] | None = None,
    ) -> list[int]:
        if "record" not in input:
            if embeddings is not None:
                raise TypeError("Record upsert accepts embedding=, not embeddings=")
            return self._native.upsert_record(_record_json(input), embedding)
        advanced = cast(RecordInput, input)
        if embedding is not None:
            raise TypeError("advanced RecordInput upsert accepts embeddings=")
        if embeddings is None:
            raise TypeError("advanced RecordInput upsert requires embeddings=")
        record_id = advanced["record"]["id"]
        return self._native.add(_records_json([advanced], {record_id: embeddings}))[0]

    def add(
        self,
        records: Iterable[RecordInput],
        *,
        embeddings: Mapping[str, Mapping[str, Embedding]],
    ) -> list[list[int]]:
        return self._native.add(_records_json(records, embeddings))

    def build(self) -> GraphRetrievalDatabase:
        return GraphRetrievalDatabase(self._native.build())

    @property
    def dimension(self) -> int | None:
        """The Rust-inferred dimension, or ``None`` before the first embedding."""

        return self._native.dimension


class GraphQueries:
    def __init__(self, native: _GraphDatabase | _GraphRetrievalDatabase) -> None:
        self._native = native

    def query(
        self,
        *,
        seeds: Sequence[GraphNode],
        traversals: Sequence[GraphTraversal] = (),
        limits: GraphQueryLimits | None = None,
        cancellation: GraphCancellationToken | None = None,
        timeout: float | None = None,
    ) -> GraphSelection:
        """Run a bounded graph query starting from explicit node identities."""

        query_limits = limits or GraphQueryLimits()
        native = self._native.query_nodes(
            [(node.node_type, node.record_id, node.chunk_key) for node in seeds],
            [
                (
                    traversal.relationship,
                    traversal.direction,
                    traversal.min_hops,
                    traversal.max_hops,
                )
                for traversal in traversals
            ],
            _query_limits_tuple(query_limits),
            cancellation=None if cancellation is None else cancellation._native,
            timeout_ms=_timeout_milliseconds(timeout),
        )
        return GraphSelection(native)

    def query_equals(
        self,
        *,
        node_type: str,
        field: str | Sequence[str],
        values: GraphScalar | Sequence[GraphScalar],
        traversals: Sequence[GraphTraversal] = (),
        limits: GraphQueryLimits | None = None,
        cancellation: GraphCancellationToken | None = None,
        timeout: float | None = None,
    ) -> GraphSelection:
        """Seed a graph query through an equality-indexed record field."""

        normalized_values: list[GraphScalar]
        if isinstance(values, (str, int, bool)):
            normalized_values = [values]
        else:
            normalized_values = list(values)
        query_limits = limits or GraphQueryLimits()
        return GraphSelection(
            self._native.query_equals(
                node_type,
                _field_path(field),
                normalized_values,
                [
                    (
                        traversal.relationship,
                        traversal.direction,
                        traversal.min_hops,
                        traversal.max_hops,
                    )
                    for traversal in traversals
                ],
                _query_limits_tuple(query_limits),
                cancellation=None if cancellation is None else cancellation._native,
                timeout_ms=_timeout_milliseconds(timeout),
            )
        )

    def project_candidates(
        self,
        selection: GraphSelection,
        *,
        where: Filter | None = None,
    ) -> GraphCandidateProjection:
        """Materialize stable candidate identities through the owning Rust corpus."""

        value = cast(
            dict[str, Any],
            self._native.project_candidates(selection._native, where=where),
        )
        return GraphCandidateProjection(
            candidates=[
                GraphChunkIdentity(
                    record_id=candidate["record_id"],
                    chunk_key=candidate["chunk_key"],
                )
                for candidate in cast(list[dict[str, str]], value["candidates"])
            ],
            source_nodes=cast(int, value["source_nodes"]),
            projected_chunks_before_filter=cast(
                int, value["projected_chunks_before_filter"]
            ),
            projected_chunks_after_filter=cast(
                int, value["projected_chunks_after_filter"]
            ),
        )


class GraphDatabase:
    """Immutable local graph database without vector or BM25 state."""

    def __init__(self, native: _GraphDatabase) -> None:
        self._native = native
        self.graph = GraphQueries(native)

    @classmethod
    def load(cls, path: str | Path) -> GraphDatabase:
        return cls(_GraphDatabase.load(Path(path)))

    @staticmethod
    def validate(path: str | Path) -> None:
        _GraphDatabase.validate(Path(path))

    def save(self, path: str | Path) -> GraphFileSizeReport:
        return cast(GraphFileSizeReport, self._native.save(Path(path)))

    def records(self, record_ids: Sequence[str]) -> list[HydratedGraphRecord | None]:
        return _hydrate_records(self._native, record_ids)

    def chunks(self, chunk_ids: Sequence[int]) -> list[HydratedGraphChunk | None]:
        return _hydrate_chunks(self._native, chunk_ids)

    @property
    def closed(self) -> bool:
        return bool(self._native.closed)

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> GraphDatabase:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


class GraphRetrievalQueries:
    def __init__(self, native: _GraphRetrievalDatabase) -> None:
        self._native = native

    def semantic_search(
        self,
        embedding: Sequence[float],
        *,
        limit: int = 10,
        where: Filter | None = None,
        within: GraphSelection | None = None,
    ) -> list[GraphSearchHit]:
        return self._native.search(
            embedding,
            limit=limit,
            where=where,
            selection=None if within is None else within._native,
        )

    def keyword_search(
        self,
        text: str,
        *,
        limit: int = 10,
        where: Filter | None = None,
        within: GraphSelection | None = None,
    ) -> list[GraphKeywordHit]:
        """Perform embedding-free BM25 search, optionally within a graph selection."""

        return self._native.keyword_search(
            text,
            limit=limit,
            where=where,
            selection=None if within is None else within._native,
        )

    def hybrid_search(
        self,
        text: str,
        embedding: Sequence[float],
        *,
        limit: int = 10,
        where: Filter | None = None,
        within: GraphSelection | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        alpha: float = 0.6,
    ) -> list[GraphHybridHit]:
        return self._native.hybrid_search(
            text,
            embedding,
            limit=limit,
            where=where,
            selection=None if within is None else within._native,
            vector_candidates=vector_candidates,
            keyword_candidates=keyword_candidates,
            alpha=alpha,
        )


class GraphRetrievalDatabase:
    """Immutable local database with separate graph and retrieval query views."""

    def __init__(self, native: _GraphRetrievalDatabase) -> None:
        self._native = native
        self.graph = GraphQueries(native)
        self.retrieval = GraphRetrievalQueries(native)

    @classmethod
    def load(cls, path: str | Path) -> GraphRetrievalDatabase:
        return cls(_GraphRetrievalDatabase.load(Path(path)))

    @staticmethod
    def validate(path: str | Path) -> None:
        _GraphRetrievalDatabase.validate(Path(path))

    def save(self, path: str | Path) -> GraphFileSizeReport:
        return cast(GraphFileSizeReport, self._native.save(Path(path)))

    def records(self, record_ids: Sequence[str]) -> list[HydratedGraphRecord | None]:
        return _hydrate_records(self._native, record_ids)

    def chunks(self, chunk_ids: Sequence[int]) -> list[HydratedGraphChunk | None]:
        return _hydrate_chunks(self._native, chunk_ids)

    @property
    def closed(self) -> bool:
        return bool(self._native.closed)

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> GraphRetrievalDatabase:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


def _hydrate_records(
    native: _GraphDatabase | _GraphRetrievalDatabase,
    record_ids: Sequence[str],
) -> list[HydratedGraphRecord | None]:
    encoded = json.loads(native.records_json(list(record_ids)))
    return [
        None if record is None else _decode_record(cast(dict[str, Any], record))
        for record in encoded
    ]


def _hydrate_chunks(
    native: _GraphDatabase | _GraphRetrievalDatabase,
    chunk_ids: Sequence[int],
) -> list[HydratedGraphChunk | None]:
    encoded = json.loads(native.chunks_json(list(chunk_ids)))
    return [
        None if chunk is None else _decode_chunk(cast(dict[str, Any], chunk))
        for chunk in encoded
    ]


def _schema_json(schema: GraphSchema) -> str:
    record_nodes = [_record_node_schema(node) for node in schema.record_nodes]
    relationships = [
        _relationship_schema(relationship) for relationship in schema.relationships
    ]
    chunk_nodes = None
    if schema.chunk_nodes is not None:
        chunk_nodes = {
            "node_type": schema.chunk_nodes.node_type,
            "owns_relationship": schema.chunk_nodes.owns_relationship,
            "inverse_relationship": schema.chunk_nodes.inverse_relationship,
        }
    return json.dumps(
        {
            "version": 1,
            "record_nodes": record_nodes,
            "relationships": relationships,
            "chunk_nodes": chunk_nodes,
        }
    )


def _record_node_schema(node: GraphRecordNode) -> dict[str, Any]:
    return {
        "record_type": node.record_type,
        "node_type": node.node_type,
        "queryable_fields": [_field_path(path) for path in node.queryable_fields],
    }


def _relationship_schema(relationship: GraphRelationship) -> dict[str, Any]:
    return {
        "relationship_type": relationship.relationship_type,
        "source_node_type": relationship.source_node_type,
        "target_node_type": relationship.target_node_type,
        "source_field": _field_path(relationship.source_field),
        "cardinality": {
            "one": "One",
            "optional_one": "OptionalOne",
            "many": "Many",
        }[relationship.cardinality],
        "missing_target": {
            "error": "Error",
            "omit_edge": "OmitEdge",
        }[relationship.missing_target],
        "duplicate_references": {
            "error": "Error",
            "deduplicate": "Deduplicate",
        }[relationship.duplicate_references],
        "allow_self_edge": relationship.allow_self_edge,
        "inverse_relationship": relationship.inverse_relationship,
    }


def _field_path(value: str | Sequence[str]) -> list[str]:
    return [value] if isinstance(value, str) else list(value)


def _records_json(
    records: Iterable[RecordInput],
    embeddings: Mapping[str, Mapping[str, Embedding]] | None = None,
) -> str:
    materialized = list(records)
    if embeddings is not None:
        record_ids = {input["record"]["id"] for input in materialized}
        unexpected_records = sorted(set(embeddings).difference(record_ids))
        if unexpected_records:
            raise UnexpectedEmbeddingError(
                "embeddings contain unknown record IDs: "
                + ", ".join(unexpected_records)
            )
    return json.dumps(
        [
            _record_batch(
                input,
                None
                if embeddings is None
                else embeddings.get(input["record"]["id"], {}),
            )
            for input in materialized
        ]
    )


def _record_json(record: Record) -> str:
    input: RecordInput = {"record": record}
    return _records_json([input])


def _timeout_milliseconds(timeout: float | None) -> int | None:
    if timeout is None:
        return None
    if not math.isfinite(timeout) or timeout < 0:
        raise ValueError("timeout must be a finite non-negative number of seconds")
    if timeout > 86_400:
        raise ValueError("timeout must not exceed 86400 seconds")
    return math.ceil(timeout * 1000)


def _query_limits_tuple(
    limits: GraphQueryLimits,
) -> tuple[int, int, int, int]:
    return (
        limits.max_hops,
        limits.max_visited,
        limits.max_results,
        limits.max_working_bytes,
    )


def _record_batch(
    input: RecordInput,
    embeddings: Mapping[str, Embedding] | None = None,
) -> dict[str, Any]:
    record = input["record"]
    chunks = input.get("chunks", [])
    if embeddings is not None:
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
                key: _encode_graph_value(value)
                for key, value in record.get("fields", {}).items()
            },
            "content": record.get("content"),
        },
        "projected_metadata": {
            key: _encode_metadata_value(value)
            for key, value in record.get("metadata", {}).items()
        },
        "chunks": [
            _chunk_batch(
                chunk,
                None if embeddings is None else embeddings[chunk["key"]],
            )
            for chunk in chunks
        ],
    }


def _chunk_batch(chunk: Chunk, embedding: Embedding | None) -> dict[str, Any]:
    result: dict[str, Any] = {
        "key": chunk["key"],
        "text": chunk["text"],
        "metadata": {
            key: _encode_metadata_value(value)
            for key, value in chunk.get("metadata", {}).items()
        },
    }
    if embedding is not None:
        result["embedding"] = list(embedding)
    return result


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


def _encode_graph_value(value: GraphValue) -> Any:
    if value is None:
        return "Null"
    if isinstance(value, bool):
        return {"Bool": value}
    if isinstance(value, int):
        return {"I64": value}
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("graph floats must be finite")
        return {"F64": value}
    if isinstance(value, str):
        return {"String": value}
    if isinstance(value, list):
        return {"List": [_encode_graph_value(item) for item in value]}
    if isinstance(value, dict):
        return {"Map": {key: _encode_graph_value(item) for key, item in value.items()}}
    raise TypeError(f"unsupported graph value: {type(value).__name__}")


def _decode_graph_value(value: Any) -> GraphValue:
    if value == "Null":
        return None
    if not isinstance(value, dict) or len(value) != 1:
        raise ValueError("invalid graph value returned by the native core")
    tag, payload = next(iter(value.items()))
    if tag in {"Bool", "I64", "F64", "String"}:
        return cast(GraphValue, payload)
    if tag == "List":
        return [_decode_graph_value(item) for item in payload]
    if tag == "Map":
        return {key: _decode_graph_value(item) for key, item in payload.items()}
    raise ValueError(f"unsupported native graph value tag: {tag}")


def _decode_metadata_value(value: Any) -> MetadataValue:
    if not isinstance(value, dict) or len(value) != 1:
        raise ValueError("invalid metadata value returned by the native core")
    tag, payload = next(iter(value.items()))
    if tag not in {"String", "Integer", "Float", "Boolean", "TimestampMillis"}:
        raise ValueError(f"unsupported native metadata value tag: {tag}")
    if tag == "TimestampMillis":
        return TimestampMillis(cast(int, payload))
    return cast(str | int | float | bool, payload)


def _decode_record(value: dict[str, Any]) -> HydratedGraphRecord:
    return {
        "id": value["id"],
        "record_type": value["record_type"],
        "fields": {
            key: _decode_graph_value(item) for key, item in value["fields"].items()
        },
        "content": value.get("content"),
    }


def _decode_chunk(value: dict[str, Any]) -> HydratedGraphChunk:
    return {
        "chunk_id": value["chunk_id"],
        "document_id": value["document_id"],
        "text": value["text"],
        "metadata": {
            key: _decode_metadata_value(item) for key, item in value["metadata"].items()
        },
        "deleted": value["deleted"],
        "version": value["version"],
    }


__all__ = [
    "GraphDatabase",
    "GraphDatabaseBuilder",
    "GraphCancellationToken",
    "GraphCancelledError",
    "GraphError",
    "GraphQueryError",
    "GraphQueries",
    "GraphRetrievalDatabase",
    "GraphRetrievalDatabaseBuilder",
    "GraphRetrievalQueries",
    "GraphSelection",
    "GraphTimeoutError",
    "InvalidGraphSchemaError",
    "MissingEmbeddingError",
    "RetrievalCapabilityUnavailableError",
    "StaleGraphSelectionError",
    "UnexpectedEmbeddingError",
]
