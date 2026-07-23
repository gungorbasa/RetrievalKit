from __future__ import annotations

from retrievalkit_graph import (
    GraphNode,
    GraphRecordInput,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabase,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphSearchHit,
    GraphTraversal,
    RetrievalConfiguration,
    VectorIndexConfiguration,
)


def typed_graph_database() -> GraphRetrievalDatabase:
    schema = GraphSchema(
        record_nodes=[GraphRecordNode("Topic", "Topic", ["title"])],
        relationships=[
            GraphRelationship(
                "related_to",
                "Topic",
                "Topic",
                "related_id",
                "optional_one",
            )
        ],
    )
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="typed-graph",
        graph=schema,
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(dimension=2)
        ),
    )
    records: list[GraphRecordInput] = [
        {
            "record": {
                "id": "alpha",
                "record_type": "Topic",
                "fields": {"title": "Alpha"},
            },
            "chunks": [{"key": "summary", "text": "alpha"}],
        }
    ]
    builder.add(records, embeddings={"alpha": {"summary": [1.0, 0.0]}})
    database = builder.build()
    selection = database.graph.query(
        seeds=[GraphNode("Topic", "alpha")],
        traversals=[GraphTraversal("related_to", min_hops=0, max_hops=0)],
    )
    hits: list[GraphSearchHit] = database.retrieval.semantic_search(
        [1.0, 0.0], within=selection
    )
    _ = hits
    return database
