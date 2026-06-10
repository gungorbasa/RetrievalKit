from __future__ import annotations

from vectorkit import Index, hybrid_search_text


def embed(texts: list[str]) -> list[list[float]]:
    values = {
        "alpha": [1.0, 0.0, 0.0, 0.0],
        "query alpha": [1.0, 0.0, 0.0, 0.0],
    }
    return [values.get(text, [0.0, 1.0, 0.0, 0.0]) for text in texts]


def main() -> None:
    index = Index(dimension=4, metric="cosine", encoding="i8")
    index.add(
        documents=[
            {
                "id": "doc-alpha",
                "metadata": {"project": "vectorkit"},
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
        where={"project": "vectorkit"},
    )
    assert [hit["document_id"] for hit in hits] == ["doc-alpha"]
    assert hits[0]["trace"]["fusion"]["kind"] == "weighted_normalized"


if __name__ == "__main__":
    main()
