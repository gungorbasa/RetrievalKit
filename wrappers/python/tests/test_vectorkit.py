from __future__ import annotations

import pytest

from vectorkit import (
    ChunkInput,
    DimensionMismatchError,
    DocumentInput,
    Filter,
    HybridHit,
    Index,
    KeywordHit,
    SearchHit,
    chunk_text,
    hybrid_search_text,
    search_text,
    where,
)


def test_chunk_text_uses_shared_rust_implementation() -> None:
    assert chunk_text(
        "abçdef", max_characters=4, overlap_characters=1, strategy="fixed"
    ) == [
        {"text": "abçd", "start_byte": 0, "end_byte": 5},
        {"text": "def", "start_byte": 4, "end_byte": 7},
    ]


def test_chunk_text_prefers_sentences_and_validates_configuration() -> None:
    assert [
        chunk["text"]
        for chunk in chunk_text(
            "First sentence. Second sentence. Third.", max_characters=25
        )
    ] == ["First sentence.", "Second sentence. Third."]

    with pytest.raises(ValueError, match="max_characters"):
        chunk_text("text", max_characters=0)


def embed(texts: list[str]) -> list[list[float]]:
    values = {
        "alpha": [1.0, 0.0, 0.0, 0.0],
        "beta": [0.0, 1.0, 0.0, 0.0],
        "query alpha": [1.0, 0.0, 0.0, 0.0],
    }
    return [values.get(text, [0.0, 0.0, 1.0, 0.0]) for text in texts]


def test_add_search_filter_and_delete() -> None:
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
                        "metadata": {"kind": "note", "archived": False},
                    }
                ],
            },
            {
                "id": "doc-beta",
                "metadata": {"project": "other"},
                "chunks": [
                    {
                        "text": "beta",
                        "embedding": [0.0, 1.0, 0.0, 0.0],
                        "metadata": {"kind": "note", "archived": False},
                    }
                ],
            },
        ]
    )

    hits = index.search(
        [1.0, 0.0, 0.0, 0.0],
        limit=5,
        where={"project": "vectorkit", "archived": False},
    )

    assert len(hits) == 1
    assert hits[0]["document_id"] == "doc-alpha"
    assert hits[0]["text"] == "alpha"
    assert hits[0]["metadata"]["kind"] == "note"
    assert hits[0]["trace"]["filter_matched"] is True

    assert index.delete_document("doc-alpha") == 1
    assert (
        index.search(
            [1.0, 0.0, 0.0, 0.0],
            limit=5,
            where={"project": "vectorkit"},
        )
        == []
    )


def test_where_helpers_and_keyword_search() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-1",
                "chunks": [
                    {
                        "text": "rust python wrapper",
                        "embedding": [1.0, 0.0, 0.0, 0.0],
                        "metadata": {"score": 10, "source": "notes"},
                    }
                ],
            }
        ]
    )

    hits = index.search(
        [1.0, 0.0, 0.0, 0.0],
        where=where.all(where.eq("source", "notes"), where.range("score", gte=5)),
    )
    assert [hit["document_id"] for hit in hits] == ["doc-1"]

    keyword_hits = index.keyword_search("python", where={"source": {"$exists": True}})
    assert [hit["document_id"] for hit in keyword_hits] == ["doc-1"]
    assert "python" in keyword_hits[0]["matched_terms"]


def test_hybrid_search_returns_scores_trace_and_candidate_limits() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-vector",
                "chunks": [
                    {
                        "text": "semantic only",
                        "embedding": [3.0, 0.0, 0.0, 0.0],
                    }
                ],
            },
            {
                "id": "doc-keyword",
                "chunks": [
                    {
                        "text": "rare keyword keyword",
                        "embedding": [0.0, 1.0, 0.0, 0.0],
                    }
                ],
            },
        ]
    )

    hits = index.hybrid_search(
        "rare keyword",
        [1.0, 0.0, 0.0, 0.0],
        limit=10,
        vector_candidates=1,
        keyword_candidates=1,
        vector_weight=0.25,
        keyword_weight=0.75,
    )

    assert {hit["document_id"] for hit in hits} == {"doc-vector", "doc-keyword"}
    keyword_hit = next(hit for hit in hits if hit["document_id"] == "doc-keyword")
    assert keyword_hit["keyword_score"] is not None
    assert keyword_hit["vector_score"] is None
    assert keyword_hit["matched_terms"] == ["keyword", "rare"]
    assert keyword_hit["trace"]["keyword_rank"] == 1
    assert keyword_hit["trace"]["vector_rank"] is None
    assert keyword_hit["trace"]["fusion"] == {
        "kind": "weighted_normalized",
        "vector_weight": 0.25,
        "keyword_weight": 0.75,
    }

    vector_only = index.hybrid_search(
        "missing",
        [1.0, 0.0, 0.0, 0.0],
        limit=10,
        vector_candidates=1,
        keyword_candidates=0,
    )
    assert [hit["document_id"] for hit in vector_only] == ["doc-vector"]


def test_hybrid_search_supports_rrf_and_filters() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-1",
                "metadata": {"project": "vectorkit"},
                "chunks": [
                    {
                        "text": "python wrapper",
                        "embedding": [1.0, 0.0, 0.0, 0.0],
                    }
                ],
            },
            {
                "id": "doc-2",
                "metadata": {"project": "other"},
                "chunks": [
                    {
                        "text": "python wrapper",
                        "embedding": [1.0, 0.0, 0.0, 0.0],
                    }
                ],
            },
        ]
    )

    hits = index.hybrid_search(
        "python",
        [1.0, 0.0, 0.0, 0.0],
        where={"project": "vectorkit"},
        fusion="rrf",
        rrf_k=42.0,
    )

    assert [hit["document_id"] for hit in hits] == ["doc-1"]
    assert hits[0]["trace"]["filter_matched"] is True
    assert hits[0]["trace"]["fusion"] == {"kind": "rrf", "rrf_k": 42.0}


def test_save_load_round_trip(tmp_path) -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-1",
                "chunks": [
                    {
                        "text": "alpha",
                        "embedding": [1.0, 0.0, 0.0, 0.0],
                        "metadata": {"project": "vectorkit"},
                    }
                ],
            }
        ]
    )

    report = index.save(tmp_path)
    assert report["total_bytes"] > 0

    loaded = Index.load(tmp_path)
    hits = loaded.search([1.0, 0.0, 0.0, 0.0], where={"project": "vectorkit"})
    assert [hit["document_id"] for hit in hits] == ["doc-1"]


def test_dimension_mismatch_is_specific_error() -> None:
    index = Index(dimension=4)
    with pytest.raises(DimensionMismatchError):
        index.search([1.0, 0.0])


def test_search_text_uses_provider() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-alpha",
                "chunks": [
                    {"text": "alpha", "embedding": [1.0, 0.0, 0.0, 0.0]},
                ],
            }
        ]
    )

    hits = search_text(index, "query alpha", embed=embed)
    assert [hit["document_id"] for hit in hits] == ["doc-alpha"]


def test_hybrid_search_text_uses_provider() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-alpha",
                "chunks": [
                    {"text": "alpha", "embedding": [1.0, 0.0, 0.0, 0.0]},
                ],
            }
        ]
    )

    hits = hybrid_search_text(index, "query alpha", embed=embed)
    assert [hit["document_id"] for hit in hits] == ["doc-alpha"]


def test_public_input_and_result_types_are_exported() -> None:
    assert ChunkInput.__name__ == "ChunkInput"
    assert DocumentInput.__name__ == "DocumentInput"
    assert str(Filter).startswith("dict[str,")
    assert SearchHit.__name__ == "SearchHit"
    assert KeywordHit.__name__ == "KeywordHit"
    assert HybridHit.__name__ == "HybridHit"
