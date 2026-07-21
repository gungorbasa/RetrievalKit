"""README quickstart using deterministic demo embeddings."""

from vectorkit import (
    RetrievalConfiguration,
    RetrievalDatabaseBuilder,
    VectorIndexConfiguration,
)

builder = RetrievalDatabaseBuilder(
    corpus_id="docs",
    retrieval=RetrievalConfiguration(
        semantic=VectorIndexConfiguration(dimension=3)
    ),
)
builder.upsert(
    {
        "record": {"id": "local-first", "record_type": "Article"},
        "chunks": [{"key": "summary", "text": "Private retrieval on device."}],
    },
    embeddings={"summary": [1.0, 0.0, 0.0]},
)
database = builder.build()
hits = database.retrieval.semantic_search([1.0, 0.0, 0.0], limit=1)
print(hits[0]["document_id"])
