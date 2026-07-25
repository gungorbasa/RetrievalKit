from __future__ import annotations

import concurrent.futures
import json
import sys
import threading
from pathlib import Path

import pytest

from retrievalkit import (
    ChunkInput,
    CorruptIndexError,
    DimensionMismatchError,
    DocumentInput,
    Filter,
    HybridHit,
    Index,
    KeywordHit,
    MissingEmbeddingError,
    PersistenceError,
    RetrievalConfiguration,
    RetrievalDatabase,
    RetrievalDatabaseBuilder,
    SearchHit,
    TimestampMillis,
    UnexpectedEmbeddingError,
    VectorIndexConfiguration,
    hybrid_search_text,
    search_text,
    where,
)
from retrievalkit.ingest import RustTextChunker, TextChunk, chunk_text
from retrievalkit.pipeline import (
    EmbeddingDimensionMismatchError,
    EmptyDocumentError,
    InvalidChunkError,
    Pipeline,
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


def _decode_fixture_metadata(
    metadata: dict[str, dict[str, object]],
) -> dict[str, object]:
    decoded: dict[str, object] = {}
    for key, tagged in metadata.items():
        tag, value = next(iter(tagged.items()))
        decoded[key] = (
            TimestampMillis(int(value)) if tag == "TimestampMillis" else value
        )
    return decoded


def test_timestamp_metadata_remains_typed_and_filters() -> None:
    index = Index(dimension=2, metric="dot_product", encoding="f32")
    timestamp = TimestampMillis(120_000)
    index.add(
        [
            {
                "id": "timestamped",
                "chunks": [
                    {
                        "text": "timestamped chunk",
                        "embedding": [1.0, 0.0],
                        "metadata": {"created_at": timestamp},
                    }
                ],
            }
        ]
    )

    hits = index.search(
        [1.0, 0.0],
        where={"created_at": {"$gte": TimestampMillis(100_000)}},
    )
    assert hits[0]["metadata"]["created_at"] == timestamp


def test_retrieval_database_uses_capability_separated_api(tmp_path) -> None:
    builder = RetrievalDatabaseBuilder(
        corpus_id="python-retrieval",
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(
                dimension=2,
                metric="dot_product",
                encoding="f32",
            )
        ),
    )
    chunk_ids = builder.upsert(
        {
            "record": {
                "id": "alpha",
                "record_type": "Topic",
                "fields": {"title": "Alpha"},
                "metadata": {"tenant": "blue"},
            },
            "chunks": [{"key": "summary", "text": "alpha retrieval"}],
        },
        embeddings={"summary": [1.0, 0.0]},
    )
    assert len(chunk_ids) == 1
    database = builder.build()

    hits = database.retrieval.semantic_search(
        [1.0, 0.0], where={"tenant": "blue"}
    )
    assert [hit["document_id"] for hit in hits] == ["alpha"]
    assert database.retrieval.hybrid_search("alpha", [1.0, 0.0])[0][
        "document_id"
    ] == "alpha"
    with pytest.raises(ValueError, match="invalid query parameter 'alpha'"):
        database.retrieval.hybrid_search("alpha", [1.0, 0.0], alpha=1.1)

    database.save(tmp_path)
    RetrievalDatabase.validate(tmp_path)
    loaded = RetrievalDatabase.load(tmp_path)
    assert loaded.retrieval.semantic_search([1.0, 0.0])[0]["text"] == "alpha retrieval"


def test_retrieval_builder_requires_exact_embedding_keys() -> None:
    builder = RetrievalDatabaseBuilder(
        corpus_id="python-retrieval-errors",
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(dimension=2)
        ),
    )
    record = {
        "record": {"id": "alpha", "record_type": "Topic"},
        "chunks": [{"key": "summary", "text": "alpha"}],
    }
    with pytest.raises(MissingEmbeddingError, match="summary"):
        builder.upsert(record, embeddings={})
    with pytest.raises(UnexpectedEmbeddingError, match="unknown"):
        builder.upsert(
            record,
            embeddings={"summary": [1.0, 0.0], "unknown": [0.0, 1.0]},
        )


def test_retrieval_database_exposes_hybrid_without_extra() -> None:
    builder = RetrievalDatabaseBuilder(
        corpus_id="python-semantic-only",
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(dimension=2)
        ),
    )
    builder.upsert(
        {
            "record": {"id": "alpha", "record_type": "Topic"},
            "chunks": [{"key": "summary", "text": "alpha"}],
        },
        embeddings={"summary": [1.0, 0.0]},
    )
    database = builder.build()
    hits = database.retrieval.hybrid_search("alpha", [1.0, 0.0], alpha=0.6)
    assert hits[0]["document_id"] == "alpha"


def test_pipeline_ingests_raw_document_and_searches_text() -> None:
    index = Index(dimension=4)
    pipeline = Pipeline(index, embed=embed)

    result = pipeline.add("doc-1", "alpha. beta.", metadata={"source": "notes"})
    hits = pipeline.search("query alpha", limit=1)

    assert len(result["chunk_ids"]) == 1
    assert hits[0]["document_id"] == "doc-1"
    assert hits[0]["metadata"]["source"] == "notes"
    assert hits[0]["metadata"]["retrievalkit.chunk.start_byte"] == 0


def test_pipeline_embedding_failure_leaves_existing_document_unchanged() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-1",
                "chunks": [{"text": "existing", "embedding": [1.0, 0.0, 0.0, 0.0]}],
            }
        ]
    )

    def fail(_texts: list[str]) -> list[list[float]]:
        raise RuntimeError("intentional embedding failure")

    pipeline = Pipeline(index, embed=fail, chunker=RustTextChunker(max_characters=20))

    with pytest.raises(RuntimeError, match="intentional"):
        pipeline.add("doc-1", "replacement")

    assert index.search([1.0, 0.0, 0.0, 0.0])[0]["text"] == "existing"


def test_pipeline_rejects_empty_documents_and_wrong_dimensions() -> None:
    index = Index(dimension=4)
    pipeline = Pipeline(
        index,
        embed=lambda _texts: [[1.0, 0.0]],
        chunker=RustTextChunker(max_characters=20),
    )

    with pytest.raises(EmptyDocumentError, match="produced no chunks"):
        pipeline.add("empty", " \n ")

    with pytest.raises(
        EmbeddingDimensionMismatchError,
        match="Embedding dimension mismatch",
    ):
        pipeline.add("bad", "content")


def test_pipeline_accepts_application_defined_chunker() -> None:
    class CustomChunker:
        def chunks(self, _text: str) -> list[TextChunk]:
            return [
                {"text": "alpha", "start_byte": 0, "end_byte": 5},
                {"text": "beta", "start_byte": 6, "end_byte": 10},
            ]

    index = Index(dimension=4)
    pipeline = Pipeline(index, embed=embed, chunker=CustomChunker())

    result = pipeline.add("custom", "alpha beta")

    assert len(result["chunk_ids"]) == 2


def test_pipeline_rejects_invalid_custom_chunk_before_embedding() -> None:
    class InvalidChunker:
        def chunks(self, _text: str) -> list[TextChunk]:
            return [{"text": "hello", "start_byte": 99, "end_byte": 1}]

    index = Index(dimension=4)
    pipeline = Pipeline(index, embed=embed, chunker=InvalidChunker())

    with pytest.raises(InvalidChunkError, match="invalid UTF-8 range"):
        pipeline.add("invalid", "hello")

    assert index.active_chunk_count == 0


def test_pipeline_subdivides_chunks_to_embedding_token_limit() -> None:
    embedded_texts: list[str] = []

    def recording_embed(texts: list[str]) -> list[list[float]]:
        embedded_texts.extend(texts)
        return [[1.0, 0.0, 0.0, 0.0] for _ in texts]

    pipeline = Pipeline(
        Index(dimension=4),
        embed=recording_embed,
        count_tokens=lambda text: len(text.split()) + 2,
        max_tokens=4,
    )

    result = pipeline.add("token-aware", "one two three four five six")

    assert len(result["chunk_ids"]) > 1
    assert all(len(text.split()) + 2 <= 4 for text in embedded_texts)


def test_add_search_filter_and_delete() -> None:
    index = Index(dimension=4, metric="cosine", encoding="i8")

    index.add(
        documents=[
            {
                "id": "doc-alpha",
                "metadata": {"project": "retrievalkit"},
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
        where={"project": "retrievalkit", "archived": False},
    )

    assert len(hits) == 1
    assert hits[0]["document_id"] == "doc-alpha"
    assert hits[0]["text"] == "alpha"
    assert hits[0]["metadata"]["kind"] == "note"

    assert index.delete_document("doc-alpha") == 1
    assert (
        index.search(
            [1.0, 0.0, 0.0, 0.0],
            limit=5,
            where={"project": "retrievalkit"},
        )
        == []
    )


def test_compact_reclaims_tombstones_and_preserves_results(tmp_path) -> None:
    index = Index(dimension=4)
    first = index.add(
        documents=[
            {
                "id": "doc-1",
                "chunks": [{"text": "old", "embedding": [1.0, 0.0, 0.0, 0.0]}],
            }
        ]
    )
    replacement = index.add(
        documents=[
            {
                "id": "doc-1",
                "chunks": [{"text": "current", "embedding": [0.0, 1.0, 0.0, 0.0]}],
            }
        ]
    )
    hit_before = index.search([0.0, 1.0, 0.0, 0.0])[0]
    assert index.total_chunk_count == 2
    assert index.tombstoned_chunk_count == 1

    report = index.compact()

    assert report["chunks_before"] == 2
    assert report["chunks_after"] == 1
    assert report["chunks_removed"] == 1
    assert report["estimated_bytes_reclaimed"] > 0
    assert index.total_chunk_count == 1
    assert index.tombstoned_chunk_count == 0
    assert index.search([0.0, 1.0, 0.0, 0.0])[0] == hit_before
    assert hit_before["chunk_id"] == replacement[0]["chunk_ids"][0]
    assert hit_before["chunk_id"] != first[0]["chunk_ids"][0]

    index.save(tmp_path)
    assert Index.load(tmp_path).search([0.0, 1.0, 0.0, 0.0])[0] == hit_before


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
        alpha=0.25,
    )

    assert {hit["document_id"] for hit in hits} == {"doc-vector", "doc-keyword"}
    keyword_hit = next(hit for hit in hits if hit["document_id"] == "doc-keyword")
    assert keyword_hit["keyword_score"] is not None
    assert keyword_hit["vector_score"] is None
    assert keyword_hit["matched_terms"] == ["keyword", "rare"]
    assert keyword_hit["trace"]["keyword_rank"] == 1
    assert keyword_hit["trace"]["vector_rank"] is None
    assert keyword_hit["trace"]["alpha"] == pytest.approx(0.25)

    vector_only = index.hybrid_search(
        "missing",
        [1.0, 0.0, 0.0, 0.0],
        limit=10,
        vector_candidates=1,
        keyword_candidates=0,
    )
    assert [hit["document_id"] for hit in vector_only] == ["doc-vector"]

    alpha_one = index.hybrid_search(
        "rare keyword",
        [1.0, 0.0, 0.0, 0.0],
        vector_candidates=1,
        keyword_candidates=1,
        alpha=1,
    )
    assert [hit["document_id"] for hit in alpha_one] == ["doc-vector"]
    assert alpha_one[0]["keyword_score"] is None
    assert alpha_one[0]["trace"]["keyword_rank"] is None
    assert alpha_one[0]["trace"]["alpha"] == 1

    alpha_zero = index.hybrid_search(
        "rare keyword",
        [],
        vector_candidates=1,
        keyword_candidates=1,
        alpha=0,
    )
    assert [hit["document_id"] for hit in alpha_zero] == ["doc-keyword"]
    assert alpha_zero[0]["vector_score"] is None
    assert alpha_zero[0]["trace"]["vector_rank"] is None
    assert alpha_zero[0]["trace"]["alpha"] == 0


def test_hybrid_search_supports_alpha_and_filters() -> None:
    index = Index(dimension=4)
    index.add(
        documents=[
            {
                "id": "doc-1",
                "metadata": {"project": "retrievalkit"},
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
        where={"project": "retrievalkit"},
        alpha=0.6,
    )

    assert [hit["document_id"] for hit in hits] == ["doc-1"]
    assert hits[0]["trace"]["alpha"] == pytest.approx(0.6)


def test_python_matches_retrieval_cross_wrapper_fixture(tmp_path: Path) -> None:
    fixture_path = (
        Path(__file__).resolve().parents[3]
        / "benchmarks"
        / "retrieval-conformance"
        / "v1"
        / "fixture.json"
    )
    fixture = json.loads(fixture_path.read_text())
    assert fixture["schema_version"] == 1
    assert fixture["fixture_id"] == "retrieval-results-v1"

    index = Index(
        dimension=fixture["dimension"],
        metric=fixture["metric"],
        encoding="f32",
    )
    index.add(
        [
            {
                "id": document["id"],
                "metadata": _decode_fixture_metadata(document["metadata"]),
                "chunks": [
                    {
                        "text": chunk["text"],
                        "embedding": chunk["embedding"],
                        "metadata": _decode_fixture_metadata(chunk["metadata"]),
                    }
                    for chunk in document["chunks"]
                ],
            }
            for document in fixture["documents"]
        ]
    )
    expectations = fixture["expectations"]

    exact = index.search(expectations["exact"]["embedding"], limit=1)
    assert [hit["document_id"] for hit in exact] == expectations["exact"][
        "document_ids"
    ]
    assert exact[0]["text"] == expectations["exact"]["text"]
    assert exact[0]["metadata"] == _decode_fixture_metadata(
        expectations["exact"]["metadata"]
    )

    keyword = index.keyword_search(expectations["keyword"]["text"])
    assert [hit["document_id"] for hit in keyword] == expectations["keyword"][
        "document_ids"
    ]
    assert keyword[0]["matched_terms"] == expectations["keyword"]["matched_terms"]

    hybrid_expectation = expectations["hybrid"]
    hybrid = index.hybrid_search(
        hybrid_expectation["text"],
        hybrid_expectation["embedding"],
        vector_candidates=1,
        keyword_candidates=1,
        alpha=hybrid_expectation["alpha"],
    )
    assert [hit["document_id"] for hit in hybrid] == hybrid_expectation[
        "document_ids"
    ]
    assert all(
        hit["trace"]["alpha"] == pytest.approx(hybrid_expectation["alpha"])
        for hit in hybrid
    )

    alpha_one = index.hybrid_search(
        hybrid_expectation["text"],
        hybrid_expectation["embedding"],
        vector_candidates=1,
        keyword_candidates=1,
        alpha=1,
    )
    assert [hit["document_id"] for hit in alpha_one] == expectations["alpha_one"][
        "document_ids"
    ]
    assert alpha_one[0]["keyword_score"] is None
    assert alpha_one[0]["trace"]["keyword_rank"] is None

    alpha_zero = index.hybrid_search(
        hybrid_expectation["text"],
        [],
        vector_candidates=1,
        keyword_candidates=1,
        alpha=0,
    )
    assert [hit["document_id"] for hit in alpha_zero] == expectations["alpha_zero"][
        "document_ids"
    ]
    assert alpha_zero[0]["vector_score"] is None
    assert alpha_zero[0]["trace"]["vector_rank"] is None

    index.save(tmp_path, include_bm25=False)
    rebuilt_keyword = Index.load(tmp_path).keyword_search(
        expectations["keyword"]["text"]
    )
    assert [
        hit["document_id"] for hit in rebuilt_keyword
    ] == expectations["compact_reload_keyword"]["document_ids"]


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
                        "metadata": {"project": "retrievalkit"},
                    }
                ],
            }
        ]
    )

    report = index.save(tmp_path)
    assert report["total_bytes"] > 0

    loaded = Index.load(tmp_path)
    hits = loaded.search([1.0, 0.0, 0.0, 0.0], where={"project": "retrievalkit"})
    assert [hit["document_id"] for hit in hits] == ["doc-1"]

    index.add(
        documents=[
            {
                "id": "doc-2",
                "chunks": [{"text": "beta", "embedding": [0.0, 1.0, 0.0, 0.0]}],
            }
        ]
    )
    index.save(tmp_path)
    reloaded = Index.load(tmp_path)
    assert reloaded.active_chunk_count == 2


def test_save_error_includes_operation_cause_and_recovery_hint(tmp_path) -> None:
    blocking_file = tmp_path / "not-a-directory"
    blocking_file.write_text("file")

    with pytest.raises(PersistenceError) as caught:
        Index(dimension=4).save(blocking_file / "index")

    message = str(caught.value)
    assert "persistence create directory failed" in message
    assert str(blocking_file / "index") in message
    assert "parent directory is writable when saving" in message


def test_validate_detects_corrupt_persisted_payload(tmp_path) -> None:
    index = Index(dimension=2)
    index.add([{"id": "doc-1", "chunks": [{"text": "alpha", "embedding": [1.0, 0.0]}]}])
    index.save(tmp_path)
    assert Index.validate(tmp_path) is None

    manifest = json.loads((tmp_path / "manifest.json").read_text())
    vectors_path = tmp_path / ".snapshots" / manifest["snapshot_id"] / "vectors.vec"
    payload = bytearray(vectors_path.read_bytes())
    payload[0] ^= 0xFF
    vectors_path.write_bytes(payload)

    with pytest.raises(CorruptIndexError, match="SHA-256 checksum mismatch"):
        Index.validate(tmp_path)


def test_dimension_mismatch_is_specific_error() -> None:
    index = Index(dimension=4)
    with pytest.raises(DimensionMismatchError):
        index.search([1.0, 0.0])


def test_shared_index_searches_run_across_python_threads_and_reject_mutation() -> None:
    dimension = 128
    chunk_count = 4_000
    embedding = [1.0] + [0.0] * (dimension - 1)
    index = Index(dimension=dimension)
    index.add(
        [
            {
                "id": "parallel-doc",
                "chunks": [
                    {"text": f"parallel local search {offset}", "embedding": embedding}
                    for offset in range(chunk_count)
                ],
            }
        ]
    )

    def run_search(operation: int) -> list[dict[str, object]]:
        if operation % 3 == 0:
            return index.search(embedding, limit=3)
        if operation % 3 == 1:
            return index.keyword_search("parallel local", limit=3)
        return index.hybrid_search("parallel local", embedding, limit=3)

    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
        futures = [executor.submit(run_search, operation) for operation in range(18)]
        results = [future.result(timeout=10) for future in futures]

    assert all(len(hits) == 3 for hits in results)
    assert all(hits[0]["document_id"] == "parallel-doc" for hits in results)

    ready = threading.Event()
    begin_mutation = [False]
    mutation_outcome: list[str] = []

    def attempt_conflicting_mutation() -> None:
        ready.set()
        while not begin_mutation[0]:
            pass
        try:
            index.compact()
        except RuntimeError as error:
            mutation_outcome.append(str(error))
        else:
            mutation_outcome.append("mutation unexpectedly ran")

    worker = threading.Thread(target=attempt_conflicting_mutation)
    worker.start()
    assert ready.wait(timeout=1)

    previous_switch_interval = sys.getswitchinterval()
    sys.setswitchinterval(1_000)
    try:
        begin_mutation[0] = True
        hits = index.hybrid_search(
            "parallel local search",
            embedding,
            limit=3,
            vector_candidates=chunk_count,
            keyword_candidates=chunk_count,
        )
    finally:
        sys.setswitchinterval(previous_switch_interval)

    worker.join(timeout=2)
    assert not worker.is_alive()
    assert len(hits) == 3
    assert mutation_outcome == ["Already borrowed"]


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
