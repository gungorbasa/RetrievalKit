from __future__ import annotations

from retrievalkit_graph import (
    GraphNode,
    GraphRecordNode,
    GraphRelationship,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    GraphTraversal,
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
    metric="dot_product",
    encoding="f32",
)
builder.upsert(
    {
        "id": "apollo",
        "record_type": "Project",
        "fields": {
            "title": "Project Apollo",
            "note_ids": ["decision-swift", "launch-checklist"],
        },
    }
)
builder.upsert(
    {
        "id": "decision-swift",
        "record_type": "Note",
        "fields": {"title": "Apple client architecture decision"},
        "content": "We chose Swift for Project Apollo's Apple platform client.",
        "metadata": {"status": "approved"},
    },
    embedding=[1.0, 0.0],
)
builder.upsert(
    {
        "id": "launch-checklist",
        "record_type": "Note",
        "fields": {"title": "Launch checklist"},
        "content": "Project Apollo launch checklist and release owners.",
        "metadata": {"status": "draft"},
    },
    embedding=[0.0, 1.0],
)
database = builder.build()

selection = database.graph.query(
    seeds=[GraphNode("Project", "apollo")],
    traversals=[GraphTraversal("contains")],
)
projection = database.graph.project_candidates(
    selection,
    where={"status": "approved"},
)
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    within=selection,
    where={"status": "approved"},
    limit=1,
)

print(f"graph-hybrid={hits[0]['document_id']}")
print(
    "graph-candidates="
    f"{projection.projected_chunks_after_filter}/"
    f"{projection.projected_chunks_before_filter}"
)
