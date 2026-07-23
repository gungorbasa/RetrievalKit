"""Hybrid retrieval quickstart using deterministic demo embeddings."""

from retrievalkit import (
    RetrievalConfiguration,
    RetrievalDatabaseBuilder,
    VectorIndexConfiguration,
)

builder = RetrievalDatabaseBuilder(
    corpus_id="project-notes",
    retrieval=RetrievalConfiguration(
        semantic=VectorIndexConfiguration(dimension=2)
    ),
)
builder.upsert(
    {
        "record": {
            "id": "decision-swift",
            "record_type": "Note",
            "metadata": {"project": "apollo", "status": "approved"},
        },
        "chunks": [
            {
                "key": "body",
                "text": "We chose Swift for Project Apollo's Apple platform client.",
            }
        ],
    },
    embeddings={"body": [1.0, 0.0]},
)
builder.upsert(
    {
        "record": {
            "id": "launch-checklist",
            "record_type": "Note",
            "metadata": {"project": "apollo", "status": "draft"},
        },
        "chunks": [
            {
                "key": "body",
                "text": "Project Apollo launch checklist and release owners.",
            }
        ],
    },
    embeddings={"body": [0.0, 1.0]},
)
database = builder.build()
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    limit=1,
)
print(f"hybrid={hits[0]['document_id']}")
