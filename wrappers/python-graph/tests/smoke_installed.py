from __future__ import annotations

from retrievalkit_graph import (
    GraphNode,
    GraphRecordNode,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
    RetrievalConfiguration,
    VectorIndexConfiguration,
)


def main() -> None:
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id="wheel-smoke",
        graph=GraphSchema([GraphRecordNode("Topic", "Topic", ["title"])]),
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(
                dimension=2,
                metric="dot_product",
                encoding="f32",
            )
        ),
    )
    builder.add(
        [
            {
                "record": {
                    "id": "topic-alpha",
                    "record_type": "Topic",
                    "fields": {"title": "Alpha"},
                },
                "chunks": [
                    {
                        "key": "summary",
                        "text": "alpha graph",
                    }
                ],
            }
        ],
        embeddings={"topic-alpha": {"summary": [1.0, 0.0]}},
    )
    database = builder.build()
    selection = database.graph.query(seeds=[GraphNode("Topic", "topic-alpha")])
    projection = database.graph.project_candidates(selection)
    assert [
        (candidate.record_id, candidate.chunk_key)
        for candidate in projection.candidates
    ] == [("topic-alpha", "summary")]
    hits = database.retrieval.semantic_search([1.0, 0.0], within=selection)
    assert [hit["document_id"] for hit in hits] == ["topic-alpha"]


if __name__ == "__main__":
    main()
