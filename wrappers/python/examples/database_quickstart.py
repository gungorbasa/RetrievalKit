"""Hybrid retrieval quickstart using deterministic demo embeddings."""

from retrievalkit import (
    Document,
    RetrievalDatabaseBuilder,
)

builder = RetrievalDatabaseBuilder(
    corpus_id="project-notes",
    metric="dot_product",
    encoding="f32",
)
builder.upsert(
    Document(
        id="decision-swift",
        text="We chose Swift for Project Apollo's Apple platform client.",
        metadata={"project": "apollo", "status": "approved"},
    ),
    embedding=[1.0, 0.0],
)
builder.upsert(
    Document(
        id="launch-checklist",
        text="Project Apollo launch checklist and release owners.",
        metadata={"project": "apollo", "status": "draft"},
    ),
    embedding=[0.0, 1.0],
)
database = builder.build()
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    limit=1,
)
print(f"hybrid={hits[0]['document_id']}")
