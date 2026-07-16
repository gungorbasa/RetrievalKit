from __future__ import annotations

import json
from pathlib import Path
from typing import Any, cast

from vectorkit_graph import (
    GraphRecordInput,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphTraversal,
    RetrievalConfiguration,
    VectorIndexConfiguration,
)

FIXTURE = (
    Path(__file__).resolve().parents[3]
    / "benchmarks"
    / "graph-conformance"
    / "v1"
    / "fixture.json"
)


def test_python_matches_cross_wrapper_graph_fixture() -> None:
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    schema = _schema(fixture["schema"])
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id=fixture["corpus_id"],
        graph=schema,
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(
                dimension=fixture["dimension"],
                metric="dot_product",
                encoding="f32",
            )
        ),
    )
    builder.add(
        [_record(record) for record in fixture["records"]],
        embeddings={
            record["record"]["id"]: {
                chunk["key"]: chunk["embedding"] for chunk in record["chunks"]
            }
            for record in fixture["records"]
        },
    )
    database = builder.build()

    equality = fixture["expectations"]["equality"]
    equality_selection = database.graph.query_equals(
        node_type=equality["node_type"],
        field=equality["field"],
        values=equality["value"],
    )
    assert _record_ids(equality_selection.matches) == equality["node_ids"]
    assert equality_selection.projected_chunk_count == equality["resolved_chunks"]

    traversal = fixture["expectations"]["traversal"]
    traversal_selection = database.graph.query(
        seeds=[
            _node(
                node_type="Topic",
                record_id=traversal["seed_record_id"],
            )
        ],
        traversals=[
            GraphTraversal(
                traversal["relationship"],
                min_hops=traversal["min_hops"],
                max_hops=traversal["max_hops"],
            )
        ],
    )
    assert _record_ids(traversal_selection.matches) == traversal["node_ids"]
    assert [
        [edge["relationship"] for edge in match["path"]]
        for match in traversal_selection.matches
    ] == traversal["paths"]

    filtered = fixture["expectations"]["filtered_exact"]
    all_topics = database.graph.query_equals(
        node_type="Topic",
        field="title",
        values=filtered["seed_titles"],
    )
    hits = database.retrieval.semantic_search(
        filtered["embedding"],
        within=all_topics,
        where={filtered["filter_field"]: filtered["filter_value"]},
    )
    assert [hit["document_id"] for hit in hits] == filtered["record_ids"]


def _schema(value: dict[str, Any]) -> GraphSchema:
    return GraphSchema(
        record_nodes=[
            GraphRecordNode(
                node["record_type"],
                node["node_type"],
                node["queryable_fields"],
            )
            for node in value["record_nodes"]
        ],
        relationships=[
            GraphRelationship(
                relationship["relationship_type"],
                relationship["source_node_type"],
                relationship["target_node_type"],
                relationship["source_field"],
                {
                    "One": "one",
                    "OptionalOne": "optional_one",
                    "Many": "many",
                }[relationship["cardinality"]],
                missing_target={"Error": "error", "OmitEdge": "omit_edge"}[
                    relationship["missing_target"]
                ],
                duplicate_references={
                    "Error": "error",
                    "Deduplicate": "deduplicate",
                }[relationship["duplicate_references"]],
                allow_self_edge=relationship["allow_self_edge"],
                inverse_relationship=relationship["inverse_relationship"],
            )
            for relationship in value["relationships"]
        ],
    )


def _record(value: dict[str, Any]) -> GraphRecordInput:
    record = value["record"]
    return {
        "record": {
            "id": record["id"],
            "record_type": record["record_type"],
            "fields": {
                key: _decode_graph_value(item)
                for key, item in record["fields"].items()
            },
            "content": record["content"],
            "metadata": {
                key: cast(str | int | float | bool, next(iter(item.values())))
                for key, item in value["projected_metadata"].items()
            },
        },
        "chunks": [
            {
                "key": chunk["key"],
                "text": chunk["text"],
                "metadata": {
                    key: cast(str | int | float | bool, next(iter(item.values())))
                    for key, item in chunk["metadata"].items()
                },
            }
            for chunk in value["chunks"]
        ],
    }


def _decode_graph_value(value: Any) -> Any:
    if value == "Null":
        return None
    tag, payload = next(iter(value.items()))
    if tag == "List":
        return [_decode_graph_value(item) for item in payload]
    if tag == "Map":
        return {key: _decode_graph_value(item) for key, item in payload.items()}
    return payload


def _node(*, node_type: str, record_id: str) -> Any:
    from vectorkit_graph import GraphNode

    return GraphNode(node_type, record_id)


def _record_ids(matches: list[Any]) -> list[str]:
    return [match["node"]["record_id"] for match in matches]
