from __future__ import annotations

from collections.abc import Sequence

from retrievalkit import (
    CompactionReport,
    Document,
    DocumentInput,
    Filter,
    HybridHit,
    Index,
    RecordInput,
    RetrievalConfiguration,
    RetrievalDatabase,
    RetrievalDatabaseBuilder,
    SearchHit,
    VectorIndexConfiguration,
    where,
)
from retrievalkit.ingest import RustTextChunker
from retrievalkit.pipeline import Pipeline


def typed_inputs_and_results(index: Index, query_embedding: list[float]) -> None:
    documents: list[DocumentInput] = [
        {
            "id": "doc-1",
            "metadata": {"project": "retrievalkit"},
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
        where.eq("project", "retrievalkit"),
        where.eq("archived", False),
    )

    index.add(documents)
    compaction: CompactionReport = index.compact()
    vector_hits: list[SearchHit] = index.search(query_embedding, where=filters)
    hybrid_hits: list[HybridHit] = index.hybrid_search(
        "python wrapper",
        query_embedding,
        where=filters,
    )

    if vector_hits:
        score: float = vector_hits[0]["score"]
        document_id: str = vector_hits[0]["document_id"]
        vector_score: float = vector_hits[0]["trace"]["vector_score"]
        _ = (score, document_id, vector_score, compaction)

    if hybrid_hits:
        matched_terms: list[str] = hybrid_hits[0]["matched_terms"]
        alpha: float = hybrid_hits[0]["trace"]["alpha"]
        _ = (matched_terms, alpha)


def typed_pipeline(pipeline: Pipeline) -> None:
    result = pipeline.add("doc-1", "typed pipeline")
    chunk_ids: list[int] = result["chunk_ids"]
    hits: list[HybridHit] = pipeline.search("typed query")
    _ = (chunk_ids, hits)


def build_typed_pipeline(index: Index) -> Pipeline:
    def embed(texts: Sequence[str]) -> list[list[float]]:
        return [[0.0, 0.0] for _ in texts]

    return Pipeline(
        index,
        embed=embed,
        chunker=RustTextChunker(max_characters=500),
        count_tokens=lambda text: len(text.split()),
        max_tokens=256,
    )


def typed_retrieval_database() -> RetrievalDatabase:
    progressive = RetrievalDatabaseBuilder(corpus_id="typed-documents")
    progressive.upsert(
        Document(id="document", text="typed document"),
        embedding=[1.0, 0.0],
    )
    _ = progressive.dimension

    builder = RetrievalDatabaseBuilder(
        corpus_id="typed-retrieval",
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(dimension=2)
        ),
    )
    record: RecordInput = {
        "record": {"id": "alpha", "record_type": "Topic"},
        "chunks": [{"key": "summary", "text": "alpha"}],
    }
    builder.upsert(record, embeddings={"summary": [1.0, 0.0]})
    database = builder.build()
    hits: list[SearchHit] = database.retrieval.semantic_search([1.0, 0.0])
    _ = hits
    return database
