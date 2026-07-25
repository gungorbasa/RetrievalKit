from __future__ import annotations

from pathlib import Path

from .graph_types import (
    Embedding,
    Filter,
    GraphDirection,
    GraphScalar,
    HybridHit,
    SearchHit,
)

_NodeInput = tuple[str, str, str | None]
_TraversalInput = tuple[str, GraphDirection, int, int]
_LimitsInput = tuple[int, int, int, int]

class RetrievalKitError(Exception): ...
class DimensionMismatchError(RetrievalKitError): ...
class InvalidIdentityError(RetrievalKitError): ...
class RetrievalCapabilityUnavailableError(RetrievalKitError): ...
class GraphError(RetrievalKitError): ...
class InvalidGraphSchemaError(GraphError): ...
class GraphQueryError(GraphError): ...
class StaleGraphSelectionError(GraphError): ...
class GraphCancelledError(GraphError): ...
class GraphTimeoutError(GraphError): ...

class _GraphCancellationToken:
    def __init__(self) -> None: ...
    @property
    def cancelled(self) -> bool: ...
    def cancel(self) -> None: ...

class _GraphSelection:
    @property
    def projected_chunk_count(self) -> int: ...
    @property
    def closed(self) -> bool: ...
    def materialize(self) -> dict[str, object]: ...
    def close(self) -> None: ...

class _GraphDatabaseBuilder:
    def __init__(self, corpus_id: str, schema_json: str) -> None: ...
    def add(self, records_json: str) -> list[list[int]]: ...
    def build(self) -> _GraphDatabase: ...

class _GraphRetrievalDatabaseBuilder:
    def __init__(
        self,
        dimension: int,
        corpus_id: str,
        schema_json: str,
        metric: str = "cosine",
        encoding: str = "i8",
    ) -> None: ...
    def add(self, records_json: str) -> list[list[int]]: ...
    def build(self) -> _GraphRetrievalDatabase: ...

class _GraphDatabase:
    @staticmethod
    def load(path: str | Path) -> _GraphDatabase: ...
    @staticmethod
    def validate(path: str | Path) -> None: ...
    def save(self, path: str | Path) -> dict[str, int]: ...
    def query_nodes(
        self,
        nodes: list[_NodeInput],
        traversals: list[_TraversalInput],
        limits: _LimitsInput,
        *,
        cancellation: _GraphCancellationToken | None = None,
        timeout_ms: int | None = None,
    ) -> _GraphSelection: ...
    def query_equals(
        self,
        node_type: str,
        field: list[str],
        values: list[GraphScalar],
        traversals: list[_TraversalInput],
        limits: _LimitsInput,
        *,
        cancellation: _GraphCancellationToken | None = None,
        timeout_ms: int | None = None,
    ) -> _GraphSelection: ...
    def project_candidates(
        self,
        selection: _GraphSelection,
        *,
        where: Filter | None = None,
    ) -> dict[str, object]: ...
    def records_json(self, record_ids: list[str]) -> str: ...
    def chunks_json(self, chunk_ids: list[int]) -> str: ...
    @property
    def closed(self) -> bool: ...
    def close(self) -> None: ...

class _GraphRetrievalDatabase:
    @staticmethod
    def load(path: str | Path) -> _GraphRetrievalDatabase: ...
    @staticmethod
    def validate(path: str | Path) -> None: ...
    def save(self, path: str | Path) -> dict[str, int]: ...
    def query_nodes(
        self,
        nodes: list[_NodeInput],
        traversals: list[_TraversalInput],
        limits: _LimitsInput,
        *,
        cancellation: _GraphCancellationToken | None = None,
        timeout_ms: int | None = None,
    ) -> _GraphSelection: ...
    def query_equals(
        self,
        node_type: str,
        field: list[str],
        values: list[GraphScalar],
        traversals: list[_TraversalInput],
        limits: _LimitsInput,
        *,
        cancellation: _GraphCancellationToken | None = None,
        timeout_ms: int | None = None,
    ) -> _GraphSelection: ...
    def project_candidates(
        self,
        selection: _GraphSelection,
        *,
        where: Filter | None = None,
    ) -> dict[str, object]: ...
    def search(
        self,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
        selection: _GraphSelection | None = None,
    ) -> list[SearchHit]: ...
    def hybrid_search(
        self,
        text: str,
        embedding: Embedding,
        *,
        limit: int = 10,
        where: Filter | None = None,
        selection: _GraphSelection | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        alpha: float = 0.6,
    ) -> list[HybridHit]: ...
    def records_json(self, record_ids: list[str]) -> str: ...
    def chunks_json(self, chunk_ids: list[int]) -> str: ...
    @property
    def closed(self) -> bool: ...
    def close(self) -> None: ...
