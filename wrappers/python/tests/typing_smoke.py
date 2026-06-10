from __future__ import annotations

from vectorkit import DocumentInput, Filter, HybridHit, Index, SearchHit, where


def typed_inputs_and_results(index: Index, query_embedding: list[float]) -> None:
    documents: list[DocumentInput] = [
        {
            "id": "doc-1",
            "metadata": {"project": "vectorkit"},
            "chunks": [
                {
                    "text": "python wrapper",
                    "embedding": [1.0, 0.0, 0.0, 0.0],
                    "metadata": {"archived": False},
                }
            ],
        }
    ]
    filters: Filter = where.all(
        where.eq("project", "vectorkit"),
        where.eq("archived", False),
    )

    index.add(documents)
    vector_hits: list[SearchHit] = index.search(query_embedding, where=filters)
    hybrid_hits: list[HybridHit] = index.hybrid_search(
        "python wrapper",
        query_embedding,
        where=filters,
    )

    if vector_hits:
        score: float = vector_hits[0]["score"]
        document_id: str = vector_hits[0]["document_id"]
        filter_matched: bool = vector_hits[0]["trace"]["filter_matched"]
        _ = (score, document_id, filter_matched)

    if hybrid_hits:
        matched_terms: list[str] = hybrid_hits[0]["matched_terms"]
        fusion_kind: str = hybrid_hits[0]["trace"]["fusion"]["kind"]
        _ = (matched_terms, fusion_kind)
