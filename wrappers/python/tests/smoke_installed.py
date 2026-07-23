from __future__ import annotations

import math

from retrievalkit import (
    Index,
    RetrievalConfiguration,
    RetrievalDatabaseBuilder,
    VectorIndexConfiguration,
    hybrid_search_text,
)


def embed(texts: list[str]) -> list[list[float]]:
    values = {
        "alpha": [1.0, 0.0, 0.0, 0.0],
        "query alpha": [1.0, 0.0, 0.0, 0.0],
    }
    return [values.get(text, [0.0, 1.0, 0.0, 0.0]) for text in texts]


def main() -> None:
    index = Index(dimension=4, metric="cosine")
    index.add(
        documents=[
            {
                "id": "doc-alpha",
                "metadata": {"project": "retrievalkit"},
                "chunks": [
                    {
                        "text": "alpha",
                        "embedding": [1.0, 0.0, 0.0, 0.0],
                    }
                ],
            }
        ]
    )

    hits = hybrid_search_text(
        index,
        "query alpha",
        embed=embed,
        where={"project": "retrievalkit"},
    )
    assert [hit["document_id"] for hit in hits] == ["doc-alpha"]
    fusion = hits[0]["trace"]["fusion"]
    assert fusion["kind"] == "weighted_normalized"
    assert math.isclose(fusion["vector_weight"], 0.6, rel_tol=1e-6)
    assert math.isclose(fusion["keyword_weight"], 0.4, rel_tol=1e-6)

    builder = RetrievalDatabaseBuilder(
        corpus_id="wheel-smoke",
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(dimension=2, metric="dot_product")
        ),
    )
    builder.upsert(
        {
            "record": {"id": "topic-alpha", "record_type": "Topic"},
            "chunks": [{"key": "summary", "text": "alpha retrieval"}],
        },
        embeddings={"summary": [1.0, 0.0]},
    )
    database = builder.build()
    assert database.retrieval.semantic_search([1.0, 0.0])[0][
        "document_id"
    ] == "topic-alpha"

if __name__ == "__main__":
    main()
