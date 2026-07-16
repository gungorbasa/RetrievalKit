from __future__ import annotations

from vectorkit_graph import (
    GraphNode,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphTraversal,
    RetrievalConfiguration,
    VectorIndexConfiguration,
)

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
    corpus_id="quickstart",
    graph=schema,
    retrieval=RetrievalConfiguration(
        semantic=VectorIndexConfiguration(
            dimension=2,
            metric="dot_product",
            encoding="f32",
        )
    ),
)
builder.add(
    [
        {
            "record": {
                "id": "alpha",
                "record_type": "Topic",
                "fields": {"title": "Alpha", "related_id": "beta"},
                "metadata": {"tenant": "red"},
            },
            "chunks": [
                {
                    "key": "summary",
                    "text": "alpha local search",
                }
            ],
        },
        {
            "record": {
                "id": "beta",
                "record_type": "Topic",
                "fields": {"title": "Beta"},
                "metadata": {"tenant": "blue"},
            },
            "chunks": [
                {
                    "key": "summary",
                    "text": "beta graph retrieval",
                }
            ],
        },
    ],
    embeddings={
        "alpha": {"summary": [1.0, 0.0]},
        "beta": {"summary": [0.0, 1.0]},
    },
)
database = builder.build()

selection = database.graph.query(
    seeds=[GraphNode("Topic", "alpha")],
    traversals=[GraphTraversal("related_to")],
)
hits = database.retrieval.semantic_search(
    [0.0, 1.0],
    within=selection,
    where={"tenant": "blue"},
)

for hit in hits:
    print(hit["document_id"], hit["score"], hit["text"])
