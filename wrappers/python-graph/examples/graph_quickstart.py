from __future__ import annotations

from retrievalkit_graph import (
    GraphDatabaseBuilder,
    GraphRecordNode,
    GraphSchema,
)

builder = GraphDatabaseBuilder(
    corpus_id="topics",
    schema=GraphSchema(record_nodes=[GraphRecordNode("Topic", "Topic", ["title"])]),
)
builder.upsert(
    {
        "id": "retrieval",
        "record_type": "Topic",
        "fields": {"title": "Local retrieval"},
        "content": "Local semantic and lexical retrieval.",
        "metadata": {"tenant": "blue"},
    }
)
database = builder.build()

selection = database.graph.query_equals(
    node_type="Topic",
    field="title",
    values="Local retrieval",
)
projection = database.graph.project_candidates(
    selection,
    where={"tenant": "blue"},
)

print(f"graph-only={selection.matches[0]['node']['record_id']}")
print(f"stable-candidates={len(projection.candidates)}")
