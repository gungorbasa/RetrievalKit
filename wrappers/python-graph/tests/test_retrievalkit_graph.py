from __future__ import annotations

from pathlib import Path

import pytest

from retrievalkit_graph import (
    GraphCancellationToken,
    GraphCancelledError,
    GraphDatabase,
    GraphDatabaseBuilder,
    GraphError,
    GraphNode,
    GraphQueryLimits,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabase,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphTimeoutError,
    GraphTraversal,
    RecordInput,
    RetrievalConfiguration,
    StaleGraphSelectionError,
    VectorIndexConfiguration,
)


def topic_graph_schema() -> GraphSchema:
    return GraphSchema(
        record_nodes=[GraphRecordNode("Topic", "Topic", ["title"])],
        relationships=[
            GraphRelationship(
                "related_to",
                "Topic",
                "Topic",
                "related_id",
                "optional_one",
                inverse_relationship="related_from",
            )
        ],
    )


def topic_graph_records() -> list[RecordInput]:
    records: list[RecordInput] = []
    rows = [
        ("alpha", "Alpha", "beta", "red", [1.0, 0.0]),
        ("beta", "Beta", "gamma", "blue", [0.6, 0.8]),
        ("gamma", "Gamma", None, "blue", [0.0, 1.0]),
    ]
    for record_id, title, related_id, tenant, _embedding in rows:
        fields: dict[str, object] = {"title": title}
        if related_id is not None:
            fields["related_id"] = related_id
        records.append(
            {
                "record": {
                    "id": record_id,
                    "record_type": "Topic",
                    "fields": fields,
                    "metadata": {"tenant": tenant},
                },
                "chunks": [
                    {
                        "key": "summary",
                        "text": f"{title.lower()} graph retrieval",
                    }
                ],
            }
        )
    return records


def topic_graph_embeddings() -> dict[str, dict[str, list[float]]]:
    return {
        "alpha": {"summary": [1.0, 0.0]},
        "beta": {"summary": [0.6, 0.8]},
        "gamma": {"summary": [0.0, 1.0]},
    }


def retrieval_configuration() -> RetrievalConfiguration:
    return RetrievalConfiguration(
        semantic=VectorIndexConfiguration(
            dimension=2,
            metric="dot_product",
            encoding="f32",
        )
    )


def test_graph_only_database_queries_hydrates_and_persists(tmp_path: Path) -> None:
    builder = GraphDatabaseBuilder(
        corpus_id="python-graph", schema=topic_graph_schema()
    )
    chunk_ids = builder.add(topic_graph_records())
    database = builder.build()

    selection = database.graph.query(
        seeds=[GraphNode("Topic", "alpha")],
        traversals=[GraphTraversal("related_to", max_hops=2)],
        limits=GraphQueryLimits(max_results=10),
    )
    assert [match["node"]["record_id"] for match in selection.matches] == [
        "beta",
        "gamma",
    ]
    assert database.records(["beta", "missing"])[1] is None
    chunk = database.chunks([chunk_ids[0][0]])[0]
    assert chunk is not None and chunk["text"] == "alpha graph retrieval"

    database.save(tmp_path)
    GraphDatabase.validate(tmp_path)
    loaded = GraphDatabase.load(tmp_path)
    result = loaded.graph.query_equals(
        node_type="Topic", field="title", values="Gamma"
    )
    assert result.matches[0]["node"]["record_id"] == "gamma"


def test_graph_retrieval_keeps_query_namespaces_separate(tmp_path: Path) -> None:
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="python-graph-retrieval",
        graph=topic_graph_schema(),
        retrieval=retrieval_configuration(),
    )
    builder.add(topic_graph_records(), embeddings=topic_graph_embeddings())
    database = builder.build()

    selection = database.graph.query(
        seeds=[GraphNode("Topic", "alpha")],
        traversals=[GraphTraversal("related_to", max_hops=2)],
    )
    scoped = database.retrieval.semantic_search(
        [1.0, 0.0], within=selection, where={"tenant": "blue"}
    )
    assert [hit["document_id"] for hit in scoped] == ["beta", "gamma"]
    hybrid = database.retrieval.hybrid_search(
        "gamma", [0.0, 1.0], within=selection, where={"tenant": "blue"}
    )
    assert hybrid[0]["document_id"] == "gamma"
    with pytest.raises(ValueError, match="invalid query parameter 'alpha'"):
        database.retrieval.hybrid_search(
            "gamma", [0.0, 1.0], within=selection, alpha=-0.1
        )

    database.save(tmp_path)
    GraphRetrievalDatabase.validate(tmp_path)
    loaded = GraphRetrievalDatabase.load(tmp_path)
    assert loaded.retrieval.semantic_search([0.0, 1.0], limit=1)[0][
        "document_id"
    ] == "gamma"


def test_graph_lifecycle_iterables_cancellation_and_timeout() -> None:
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="python-graph-lifecycle",
        graph=topic_graph_schema(),
        retrieval=retrieval_configuration(),
    )
    builder.add(
        (record for record in topic_graph_records()),
        embeddings=topic_graph_embeddings(),
    )
    database = builder.build()

    with database.graph.query_equals(
        node_type="Topic", field="title", values=["Alpha", "Beta"]
    ) as selection:
        assert database.retrieval.semantic_search([1.0, 0.0], within=selection)
    with pytest.raises(StaleGraphSelectionError, match="closed"):
        database.retrieval.semantic_search([1.0, 0.0], within=selection)

    cancellation = GraphCancellationToken()
    cancellation.cancel()
    with pytest.raises(GraphCancelledError, match="cancelled"):
        database.graph.query(
            seeds=[GraphNode("Topic", "alpha")], cancellation=cancellation
        )
    with pytest.raises(GraphTimeoutError, match="timeout of 0 ms"):
        database.graph.query(seeds=[GraphNode("Topic", "alpha")], timeout=0)

    database.close()
    with pytest.raises(GraphError, match="closed"):
        database.records(["alpha"])
