from __future__ import annotations

import json
from pathlib import Path
from typing import NoReturn

import pytest

from retrievalkit_graph import (
    Bm25Configuration,
    GraphCancellationToken,
    GraphCancelledError,
    GraphCandidateProjection,
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
        ),
        bm25=Bm25Configuration(k1=1.7, b=0.4, stop_words=("graph",)),
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
    projection = database.graph.project_candidates(
        selection,
        where={"tenant": "blue"},
    )
    assert projection.source_nodes == 2
    assert projection.projected_chunks_before_filter == 2
    assert projection.projected_chunks_after_filter == 2
    assert database.records(["beta", "missing"])[1] is None
    chunk = database.chunks([chunk_ids[0][0]])[0]
    assert chunk is not None and chunk["text"] == "alpha graph retrieval"

    database.save(tmp_path)
    GraphDatabase.validate(tmp_path)
    loaded = GraphDatabase.load(tmp_path)
    result = loaded.graph.query_equals(node_type="Topic", field="title", values="Gamma")
    assert result.matches[0]["node"]["record_id"] == "gamma"


def test_progressive_graph_builders_hide_chunks_and_infer_dimension() -> None:
    schema = topic_graph_schema()
    graph_builder = GraphDatabaseBuilder(
        corpus_id="python-progressive-graph",
        schema=schema,
    )
    graph_builder.upsert(
        {
            "id": "alpha",
            "record_type": "Topic",
            "fields": {"title": "Alpha"},
            "content": "alpha graph",
            "metadata": {"tenant": "blue"},
        }
    )
    graph_database = graph_builder.build()
    graph_selection = graph_database.graph.query_equals(
        node_type="Topic",
        field="title",
        values="Alpha",
    )
    graph_projection = graph_database.graph.project_candidates(graph_selection)
    assert [
        (candidate.record_id, candidate.chunk_key)
        for candidate in graph_projection.candidates
    ] == [("alpha", "alpha")]

    combined_builder = GraphRetrievalDatabaseBuilder(
        corpus_id="python-progressive-combined",
        graph=schema,
        metric="dot_product",
        encoding="f32",
    )
    combined_builder.upsert(
        {
            "id": "alpha",
            "record_type": "Topic",
            "fields": {"title": "Alpha", "related_id": "beta"},
        }
    )
    assert combined_builder.dimension is None
    combined_builder.upsert(
        {
            "id": "beta",
            "record_type": "Topic",
            "fields": {"title": "Beta"},
            "content": "beta retrieval",
            "metadata": {"tenant": "blue"},
        },
        embedding=[1.0, 0.0],
    )
    assert combined_builder.dimension == 2
    combined = combined_builder.build()
    selection = combined.graph.query(
        seeds=[GraphNode("Topic", "alpha")],
        traversals=[GraphTraversal("related_to")],
    )
    assert (
        combined.retrieval.semantic_search(
            [1.0, 0.0],
            within=selection,
        )[0]["document_id"]
        == "beta"
    )


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
    assert [
        hit["document_id"]
        for hit in database.retrieval.keyword_search("gamma", within=selection)
    ] == ["gamma"]
    assert database.retrieval.keyword_search("graph", within=selection) == []
    hybrid = database.retrieval.hybrid_search(
        "gamma", [0.0, 1.0], within=selection, where={"tenant": "blue"}
    )
    assert hybrid[0]["document_id"] == "gamma"
    assert hybrid[0]["metadata"]["tenant"] == "blue"
    assert hybrid[0]["trace"]["alpha"] == pytest.approx(0.6)
    with pytest.raises(ValueError, match="invalid query parameter 'alpha'"):
        database.retrieval.hybrid_search(
            "gamma", [0.0, 1.0], within=selection, alpha=-0.1
        )

    database.save(tmp_path)
    GraphRetrievalDatabase.validate(tmp_path)
    loaded = GraphRetrievalDatabase.load(tmp_path)
    assert (
        loaded.retrieval.semantic_search([0.0, 1.0], limit=1)[0]["document_id"]
        == "gamma"
    )
    assert loaded.retrieval.keyword_search("graph") == []


def test_candidate_projection_is_filtered_stable_and_lexically_ordered() -> None:
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="python-candidate-projection",
        graph=topic_graph_schema(),
        retrieval=retrieval_configuration(),
    )
    builder.add(topic_graph_records(), embeddings=topic_graph_embeddings())
    database = builder.build()
    selection = database.graph.query_equals(
        node_type="Topic",
        field="title",
        values=["Gamma", "Alpha", "Beta"],
    )

    projection: GraphCandidateProjection = database.graph.project_candidates(
        selection,
        where={"tenant": "blue"},
    )

    assert projection.source_nodes == 3
    assert projection.projected_chunks_before_filter == 3
    assert projection.projected_chunks_after_filter == 2
    assert [
        (candidate.record_id, candidate.chunk_key)
        for candidate in projection.candidates
    ] == [("beta", "summary"), ("gamma", "summary")]


def test_candidate_projection_rejects_stale_and_cross_corpus_selections() -> None:
    original_builder = GraphDatabaseBuilder(
        corpus_id="candidate-generation",
        schema=GraphSchema([GraphRecordNode("Topic", "Topic", ["title"])]),
    )
    original_builder.upsert(topic_graph_records()[0])
    original = original_builder.build()
    selection = original.graph.query_equals(
        node_type="Topic",
        field="title",
        values="Alpha",
    )

    newer_builder = GraphDatabaseBuilder(
        corpus_id="candidate-generation",
        schema=GraphSchema([GraphRecordNode("Topic", "Topic", ["title"])]),
    )
    newer_builder.add(topic_graph_records()[:2])
    newer = newer_builder.build()
    with pytest.raises(StaleGraphSelectionError, match="stale graph result"):
        newer.graph.project_candidates(selection)

    other_builder = GraphDatabaseBuilder(
        corpus_id="other-corpus",
        schema=GraphSchema([GraphRecordNode("Topic", "Topic", ["title"])]),
    )
    other_builder.upsert(topic_graph_records()[0])
    other = other_builder.build()
    with pytest.raises(StaleGraphSelectionError, match="stale graph result"):
        other.graph.project_candidates(selection)


def test_graph_query_transport_does_not_use_json(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="python-typed-query-transport",
        graph=topic_graph_schema(),
        retrieval=retrieval_configuration(),
    )
    builder.add(topic_graph_records(), embeddings=topic_graph_embeddings())
    database = builder.build()

    def fail_json(*_args: object, **_kwargs: object) -> NoReturn:
        raise AssertionError("graph query transport must not use JSON")

    monkeypatch.setattr(json, "dumps", fail_json)
    monkeypatch.setattr(json, "loads", fail_json)

    selection = database.graph.query(
        seeds=[GraphNode("Topic", "alpha")],
        traversals=[GraphTraversal("related_to", max_hops=2)],
    )
    assert [match["node"]["record_id"] for match in selection.matches] == [
        "beta",
        "gamma",
    ]
    assert database.graph.project_candidates(selection).source_nodes == 2
    assert database.retrieval.semantic_search(
        [1.0, 0.0],
        within=selection,
    )


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
