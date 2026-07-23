from __future__ import annotations

from retrievalkit_graph import (
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
    record_nodes=[
        GraphRecordNode("Project", "Project", ["title"]),
        GraphRecordNode("Note", "Note", ["title"]),
    ],
    relationships=[
        GraphRelationship(
            "contains",
            "Project",
            "Note",
            "note_ids",
            "many",
        )
    ],
)

builder = GraphRetrievalDatabaseBuilder(
    corpus_id="project-notes",
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
                "id": "apollo",
                "record_type": "Project",
                "fields": {
                    "title": "Project Apollo",
                    "note_ids": ["decision-swift", "launch-checklist"],
                },
            },
            "chunks": [],
        },
        {
            "record": {
                "id": "decision-swift",
                "record_type": "Note",
                "fields": {"title": "Apple client architecture decision"},
                "metadata": {"status": "approved"},
            },
            "chunks": [
                {
                    "key": "body",
                    "text": (
                        "We chose Swift for Project Apollo's Apple platform client."
                    ),
                }
            ],
        },
        {
            "record": {
                "id": "launch-checklist",
                "record_type": "Note",
                "fields": {"title": "Launch checklist"},
                "metadata": {"status": "draft"},
            },
            "chunks": [
                {
                    "key": "body",
                    "text": "Project Apollo launch checklist and release owners.",
                }
            ],
        },
    ],
    embeddings={
        "apollo": {},
        "decision-swift": {"body": [1.0, 0.0]},
        "launch-checklist": {"body": [0.0, 1.0]},
    },
)
database = builder.build()

selection = database.graph.query(
    seeds=[GraphNode("Project", "apollo")],
    traversals=[GraphTraversal("contains")],
)
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    within=selection,
    where={"status": "approved"},
    limit=1,
)

print(f"graph-hybrid={hits[0]['document_id']}")
