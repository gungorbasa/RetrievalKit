#!/usr/bin/env python3
"""Independently cross-check VectorKit V3 Phase 1.2a A-C rankings.

This qualification-only validator reads the frozen collection and Rust result
artifact directly. It does not invoke VectorKit or consume Rust scoring traces
as inputs to its calculations.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import struct
import sys
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
REPORT_NAME = "independent-cross-check.json"
REPORT_SCHEMA = "phase-1.2a-independent-cross-check-v1"
F32_EPSILON = 1.1920928955078125e-07
F32_SCORE_TOLERANCE = 2.0e-07
FROZEN_COLLECTION_SHA256 = "0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65"
FROZEN_RUN_IDS = [
    "v3-a-whole-semantic-f32-na-cfg-984e4c3bf991",
    "v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53",
    "v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0",
]


class ValidationError(RuntimeError):
    """A deterministic independent cross-check failure."""


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def f32_add(left: float, right: float) -> float:
    return f32(f32(left) + f32(right))


def f32_sub(left: float, right: float) -> float:
    return f32(f32(left) - f32(right))


def f32_mul(left: float, right: float) -> float:
    return f32(f32(left) * f32(right))


def f32_div(left: float, right: float) -> float:
    return f32(f32(left) / f32(right))


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"failed to read JSON '{path}': {error}") from error


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
        return [json.loads(line) for line in lines]
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"failed to read JSONL '{path}': {error}") from error


def verify_frozen_fixture(collection: Path) -> None:
    collection_path = collection / "collection.json"
    try:
        collection_bytes = collection_path.read_bytes()
    except OSError as error:
        raise ValidationError(f"failed to read frozen collection header: {error}") from error
    actual = hashlib.sha256(collection_bytes).hexdigest()
    if actual != FROZEN_COLLECTION_SHA256:
        raise ValidationError(
            f"frozen collection hash expected {FROZEN_COLLECTION_SHA256}, actual {actual}"
        )
    header = read_json(collection_path)
    for entry in header["files"]:
        path = collection / entry["path"]
        try:
            data = path.read_bytes()
        except OSError as error:
            raise ValidationError(f"failed to read frozen fixture file '{path}': {error}") from error
        digest = hashlib.sha256(data).hexdigest()
        if len(data) != entry["bytes"] or digest != entry["sha256"]:
            raise ValidationError(
                f"frozen fixture file '{entry['path']}' failed byte/hash validation"
            )


def tagged_value(value: dict[str, Any]) -> Any:
    kind = value["type"]
    if kind == "null":
        return None
    if kind in {"string", "integer", "float", "boolean", "timestamp_millis"}:
        return value["value"]
    raise ValidationError(f"unsupported metadata value type '{kind}'")


def tokenize(text: str) -> list[str]:
    # The frozen fixture is ASCII. This is the fixture-equivalent projection of
    # unicode-segmentation's unicode_words plus Rust lowercase.
    return [token.lower() for token in re.findall(r"[^\W_]+", text, re.UNICODE)]


def normalize(vector: list[float]) -> list[float]:
    squared_norm = 0.0
    for value in vector:
        squared_norm = f32_add(squared_norm, f32_mul(value, value))
    if squared_norm == 0.0:
        return [f32(value) for value in vector]
    root = f32(math.sqrt(squared_norm))
    inverse = f32_div(1.0, root)
    return [f32_mul(value, inverse) for value in vector]


def dot(left: list[float], right: list[float]) -> float:
    total = 0.0
    for left_value, right_value in zip(left, right, strict=True):
        total = f32_add(total, f32_mul(left_value, right_value))
    return total


def round_half_away_from_zero(value: float) -> int:
    if value >= 0.0:
        return math.floor(value + 0.5)
    return math.ceil(value - 0.5)


def quantize(vector: list[float]) -> tuple[list[int], float]:
    maximum = max((abs(f32(value)) for value in vector), default=0.0)
    if maximum == 0.0:
        return [0] * len(vector), 0.0
    scale = f32_div(maximum, 127.0)
    inverse = f32_div(1.0, scale)
    values = [
        max(-128, min(127, round_half_away_from_zero(f32_mul(value, inverse))))
        for value in vector
    ]
    return values, scale


def i8_score(left: list[float], right: list[float]) -> float:
    left_values, left_scale = quantize(left)
    right_values, right_scale = quantize(right)
    accumulator = sum(a * b for a, b in zip(left_values, right_values, strict=True))
    return f32_mul(f32_mul(f32(accumulator), left_scale), right_scale)


def filter_matches(node: dict[str, Any] | None, metadata: dict[str, Any]) -> bool:
    if node is None:
        return True
    operation = node["op"]
    if operation == "all":
        return all(filter_matches(child, metadata) for child in node["children"])
    if operation == "any":
        return any(filter_matches(child, metadata) for child in node["children"])
    field = node["field"]
    if operation == "exists":
        return field in metadata
    if operation == "equals":
        return metadata.get(field) == tagged_value(node["value"])
    if operation == "not_equals":
        return field in metadata and metadata[field] != tagged_value(node["value"])
    if operation == "in":
        return metadata.get(field) in [tagged_value(value) for value in node["values"]]
    if operation == "range":
        if field not in metadata:
            return False
        value = metadata[field]
        lower = None if node["lower"] is None else tagged_value(node["lower"])
        upper = None if node["upper"] is None else tagged_value(node["upper"])
        return (lower is None or value >= lower) and (upper is None or value <= upper)
    raise ValidationError(f"unsupported metadata filter operation '{operation}'")


def load_fixture(collection: Path) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    records = read_jsonl(collection / "records.jsonl")
    corpus_embeddings = {
        (row["record_id"], row["chunk_key"]): [f32(value) for value in row["values"]]
        for row in read_jsonl(collection / "corpus-embeddings.f32.jsonl")
    }
    query_embeddings = {
        row["query_id"]: [f32(value) for value in row["values"]]
        for row in read_jsonl(collection / "query-embeddings.f32.jsonl")
    }
    chunks: list[dict[str, Any]] = []
    for record in records:
        inherited = {
            field: tagged_value(value) for field, value in record["metadata"].items()
        }
        for chunk in record["chunks"]:
            identity = (record["record_id"], chunk["chunk_key"])
            metadata = dict(inherited)
            metadata.update(
                {field: tagged_value(value) for field, value in chunk["metadata"].items()}
            )
            chunks.append(
                {
                    "chunk_key": identity[1],
                    "embedding": corpus_embeddings[identity],
                    "identity": identity,
                    "metadata": metadata,
                    "text": chunk["text"],
                    "tokens": tokenize(chunk["text"]),
                }
            )
    chunks.sort(key=lambda chunk: chunk["identity"])
    for chunk_id, chunk in enumerate(chunks):
        chunk["chunk_id"] = chunk_id

    queries = [
        {**query, "embedding": query_embeddings[query["query_id"]]}
        for query in read_jsonl(collection / "queries.jsonl")
        if "retrieval" in query["tasks"]
    ]
    queries.sort(key=lambda query: query["query_id"])
    return chunks, queries


def vector_candidates(
    chunks: list[dict[str, Any]],
    query: dict[str, Any],
    encoding: str,
    limit: int | None,
) -> list[dict[str, Any]]:
    normalized_query = normalize(query["embedding"])
    candidates: list[dict[str, Any]] = []
    for chunk in chunks:
        if not filter_matches(query["metadata_filter"], chunk["metadata"]):
            continue
        normalized_chunk = normalize(chunk["embedding"])
        score = (
            dot(normalized_query, normalized_chunk)
            if encoding == "f32"
            else i8_score(normalized_query, normalized_chunk)
        )
        candidates.append({"chunk": chunk, "score": score})
    candidates.sort(key=lambda candidate: (-candidate["score"], candidate["chunk"]["identity"]))
    return candidates if limit is None else candidates[:limit]


def inverse_document_frequency(active_count: int, document_frequency: int) -> float:
    numerator = f32_add(f32_sub(float(active_count), float(document_frequency)), 0.5)
    denominator = f32_add(float(document_frequency), 0.5)
    ratio = f32_div(numerator, denominator)
    return f32(math.log(f32_add(1.0, ratio)))


def bm25_term_score(
    term_frequency: int,
    chunk_length: int,
    average_length: float,
    inverse_frequency: float,
) -> float:
    frequency = f32(term_frequency)
    length_ratio = f32_div(float(chunk_length), average_length)
    normalization = f32_add(f32_sub(1.0, 0.75), f32_mul(0.75, length_ratio))
    denominator = f32_add(frequency, f32_mul(1.2, normalization))
    numerator = f32_mul(frequency, f32_add(1.2, 1.0))
    return f32_div(f32_mul(inverse_frequency, numerator), denominator)


def keyword_candidates(
    chunks: list[dict[str, Any]], query: dict[str, Any], limit: int
) -> list[dict[str, Any]]:
    average_length = f32_div(sum(len(chunk["tokens"]) for chunk in chunks), len(chunks))
    query_terms = sorted(set(tokenize(query["text"])))
    scores: dict[tuple[str, str], float] = {}
    matches: dict[tuple[str, str], list[str]] = {}
    for term in query_terms:
        document_frequency = sum(term in chunk["tokens"] for chunk in chunks)
        if document_frequency == 0:
            continue
        inverse_frequency = inverse_document_frequency(len(chunks), document_frequency)
        for chunk in chunks:
            if not filter_matches(query["metadata_filter"], chunk["metadata"]):
                continue
            frequency = Counter(chunk["tokens"])[term]
            if frequency == 0:
                continue
            identity = chunk["identity"]
            score = bm25_term_score(
                frequency, len(chunk["tokens"]), average_length, inverse_frequency
            )
            scores[identity] = f32_add(scores.get(identity, 0.0), score)
            matches.setdefault(identity, []).append(term)
    by_identity = {chunk["identity"]: chunk for chunk in chunks}
    candidates = [
        {"chunk": by_identity[identity], "matched_terms": matches[identity], "score": score}
        for identity, score in scores.items()
    ]
    candidates.sort(key=lambda candidate: (-candidate["score"], candidate["chunk"]["identity"]))
    return candidates[:limit]


def normalized_score(score: float, minimum: float, maximum: float) -> float:
    width = f32_sub(maximum, minimum)
    if width <= F32_EPSILON:
        return 1.0
    return f32_div(f32_sub(score, minimum), width)


def semantic_hits(
    chunks: list[dict[str, Any]], query: dict[str, Any], encoding: str
) -> list[dict[str, Any]]:
    output = []
    for rank, candidate in enumerate(vector_candidates(chunks, query, encoding, None), 1):
        record_id, chunk_key = candidate["chunk"]["identity"]
        output.append(
            {
                "bm25_normalized_score": None,
                "bm25_score": None,
                "chunk_key": chunk_key,
                "fusion_score": None,
                "keyword_rank": None,
                "matched_terms": [],
                "native_rank": rank,
                "record_id": record_id,
                "vector_normalized_score": None,
                "vector_rank": rank,
                "vector_score": candidate["score"],
            }
        )
    return output


def hybrid_hits(chunks: list[dict[str, Any]], query: dict[str, Any]) -> list[dict[str, Any]]:
    vectors = vector_candidates(chunks, query, "i8", 8)
    keywords = keyword_candidates(chunks, query, 8)
    vector_by_id = {
        candidate["chunk"]["identity"]: (rank, candidate) for rank, candidate in enumerate(vectors, 1)
    }
    keyword_by_id = {
        candidate["chunk"]["identity"]: (rank, candidate) for rank, candidate in enumerate(keywords, 1)
    }
    vector_scores = [candidate["score"] for candidate in vectors]
    keyword_scores = [candidate["score"] for candidate in keywords]
    vector_range = (min(vector_scores), max(vector_scores))
    keyword_range = (
        (min(keyword_scores), max(keyword_scores)) if keyword_scores else None
    )
    candidates = []
    for identity in sorted(set(vector_by_id) | set(keyword_by_id)):
        vector = vector_by_id.get(identity)
        keyword = keyword_by_id.get(identity)
        vector_score = None if vector is None else vector[1]["score"]
        keyword_score = None if keyword is None else keyword[1]["score"]
        vector_normalized = (
            None
            if vector_score is None
            else normalized_score(vector_score, vector_range[0], vector_range[1])
        )
        keyword_normalized = (
            None
            if keyword_score is None or keyword_range is None
            else normalized_score(keyword_score, keyword_range[0], keyword_range[1])
        )
        fusion = f32_add(
            f32_mul(0.6, 0.0 if vector_normalized is None else vector_normalized),
            f32_mul(0.4, 0.0 if keyword_normalized is None else keyword_normalized),
        )
        candidates.append(
            {
                "bm25_normalized_score": keyword_normalized,
                "bm25_score": keyword_score,
                "chunk_key": identity[1],
                "fusion_score": fusion,
                "keyword_rank": None if keyword is None else keyword[0],
                "matched_terms": [] if keyword is None else keyword[1]["matched_terms"],
                "native_rank": 0,
                "record_id": identity[0],
                "vector_normalized_score": vector_normalized,
                "vector_rank": None if vector is None else vector[0],
                "vector_score": vector_score,
                "_chunk_id": (vector or keyword)[1]["chunk"]["chunk_id"],
            }
        )
    candidates.sort(
        key=lambda candidate: (
            -candidate["fusion_score"],
            candidate["vector_rank"] if candidate["vector_rank"] is not None else math.inf,
            candidate["keyword_rank"] if candidate["keyword_rank"] is not None else math.inf,
            candidate["_chunk_id"],
        )
    )
    for rank, candidate in enumerate(candidates, 1):
        candidate["native_rank"] = rank
        del candidate["_chunk_id"]
    return candidates


def project(hits: list[dict[str, Any]], depth: int) -> tuple[list[dict[str, Any]], int]:
    seen: set[str] = set()
    documents = []
    duplicates = 0
    for hit in hits:
        if len(documents) == depth:
            break
        if hit["record_id"] in seen:
            duplicates += 1
            continue
        seen.add(hit["record_id"])
        documents.append(
            {
                "chunk_key": hit["chunk_key"],
                "document_rank": len(documents) + 1,
                "native_chunk_rank": hit["native_rank"],
                "record_id": hit["record_id"],
                "score": (
                    hit["vector_score"]
                    if hit["fusion_score"] is None
                    else hit["fusion_score"]
                ),
            }
        )
    return documents, duplicates


def load_qrels(collection: Path) -> dict[str, dict[str, int]]:
    qrels: dict[str, dict[str, int]] = {}
    try:
        rows = (collection / "qrels.tsv").read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ValidationError(f"failed to read frozen qrels: {error}") from error
    for row in rows:
        query_id, zero, record_id, relevance = row.split(" ")
        if zero != "0":
            raise ValidationError(f"invalid qrel row '{row}'")
        qrels.setdefault(query_id, {})[record_id] = int(relevance)
    return qrels


def relevant_count(
    documents: list[dict[str, Any]], qrels: dict[str, int], cutoff: int
) -> int:
    return sum(
        qrels.get(document["record_id"], 0) >= 1 for document in documents[:cutoff]
    )


def recall(documents: list[dict[str, Any]], qrels: dict[str, int], cutoff: int) -> float:
    relevant = sum(relevance >= 1 for relevance in qrels.values())
    return relevant_count(documents, qrels, cutoff) / relevant


def reciprocal_rank(
    documents: list[dict[str, Any]], qrels: dict[str, int], cutoff: int
) -> float:
    for rank, document in enumerate(documents[:cutoff], 1):
        if qrels.get(document["record_id"], 0) >= 1:
            return 1.0 / rank
    return 0.0


def average_precision(documents: list[dict[str, Any]], qrels: dict[str, int]) -> float:
    relevant = sum(relevance >= 1 for relevance in qrels.values())
    found = 0
    total = 0.0
    for rank, document in enumerate(documents, 1):
        if qrels.get(document["record_id"], 0) >= 1:
            found += 1
            total += found / rank
    return total / relevant


def judged(
    documents: list[dict[str, Any]], qrels: dict[str, int], cutoff: int
) -> float:
    denominator = min(cutoff, len(documents))
    if denominator == 0:
        return 0.0
    return (
        sum(document["record_id"] in qrels for document in documents[:cutoff])
        / denominator
    )


def ndcg(documents: list[dict[str, Any]], qrels: dict[str, int], cutoff: int) -> float:
    dcg = 0.0
    for rank, document in enumerate(documents[:cutoff], 1):
        gain = (2 ** qrels.get(document["record_id"], 0) - 1) / math.log2(rank + 1)
        dcg += gain
    ideal = sorted(qrels.items(), key=lambda item: (-item[1], item[0]))
    idcg = 0.0
    for rank, (_, relevance) in enumerate(ideal[:cutoff], 1):
        idcg += (2**relevance - 1) / math.log2(rank + 1)
    return dcg / idcg


def retrieval_metrics(
    documents: list[dict[str, Any]], qrels: dict[str, int]
) -> dict[str, float]:
    return {
        "ap": average_precision(documents, qrels),
        "judged_at_10": judged(documents, qrels, 10),
        "judged_at_5": judged(documents, qrels, 5),
        "mrr_at_10": reciprocal_rank(documents, qrels, 10),
        "ndcg_at_10": ndcg(documents, qrels, 10),
        "ndcg_at_5": ndcg(documents, qrels, 5),
        "precision_at_5": relevant_count(documents, qrels, 5) / 5,
        "recall_at_10": recall(documents, qrels, 10),
        "recall_at_5": recall(documents, qrels, 5),
        "success_at_1": float(relevant_count(documents, qrels, 1) > 0),
    }


def macro_metrics(rows: list[dict[str, float]]) -> dict[str, float]:
    totals = {key: 0.0 for key in rows[0]}
    for row in rows:
        for key in totals:
            totals[key] += row[key]
    return {key: value / len(rows) for key, value in totals.items()}


def assert_equal(
    expected: Any,
    actual: Any,
    path: str,
    differences: dict[str, float] | None = None,
) -> None:
    if (
        isinstance(expected, (int, float))
        and not isinstance(expected, bool)
        and isinstance(actual, (int, float))
        and not isinstance(actual, bool)
    ):
        equal = (
            math.isclose(
                float(expected),
                float(actual),
                rel_tol=F32_SCORE_TOLERANCE,
                abs_tol=F32_SCORE_TOLERANCE,
            )
            if isinstance(expected, float) or isinstance(actual, float)
            else expected == actual
        )
        if differences is not None and (
            isinstance(expected, float) or isinstance(actual, float)
        ):
            category = "metrics" if ".metrics" in path or ".macro" in path else "scores"
            differences[category] = max(
                differences[category], abs(float(expected) - float(actual))
            )
        if not equal:
            raise ValidationError(f"{path}: expected {expected!r}, actual {actual!r}")
        return
    if type(expected) is not type(actual):
        raise ValidationError(
            f"{path}: type mismatch expected {type(expected).__name__}, actual {type(actual).__name__}"
        )
    if isinstance(expected, dict):
        if expected.keys() != actual.keys():
            raise ValidationError(
                f"{path}: key mismatch expected {sorted(expected)}, actual {sorted(actual)}"
            )
        for key in expected:
            assert_equal(expected[key], actual[key], f"{path}.{key}", differences)
        return
    if isinstance(expected, list):
        if len(expected) != len(actual):
            raise ValidationError(
                f"{path}: length mismatch expected {len(expected)}, actual {len(actual)}"
            )
        for offset, (left, right) in enumerate(zip(expected, actual, strict=True)):
            assert_equal(left, right, f"{path}[{offset}]", differences)
        return
    if expected != actual:
        raise ValidationError(f"{path}: expected {expected!r}, actual {actual!r}")


def validate(collection: Path, artifacts: Path) -> dict[str, Any]:
    verify_frozen_fixture(collection)
    marker = read_json(artifacts / "qualification.json")
    if marker.get("partial") is not True or marker.get("publication_ready") is not False:
        raise ValidationError("qualification marker does not identify a partial, non-publication artifact")
    results = read_json(artifacts / "rust-results.json")
    rust_metrics = read_json(artifacts / "metrics.json")
    chunks, queries = load_fixture(collection)
    qrels = load_qrels(collection)
    query_by_id = {query["query_id"]: query for query in queries}
    expected_letters = ["a", "b", "c"]
    actual_letters = [run["run_id"].split("-", 2)[1] for run in results["runs"]]
    if actual_letters != expected_letters:
        raise ValidationError(
            f"Rust result run letters expected {expected_letters}, actual {actual_letters}"
        )
    actual_run_ids = [run["run_id"] for run in results["runs"]]
    if actual_run_ids != FROZEN_RUN_IDS:
        raise ValidationError(
            f"Rust result run IDs expected {FROZEN_RUN_IDS}, actual {actual_run_ids}"
        )

    checked = 0
    differences = {"metrics": 0.0, "scores": 0.0}
    calculated_metrics: dict[str, dict[str, dict[str, float]]] = {}
    for run in results["runs"]:
        letter = run["run_id"].split("-", 2)[1]
        if run["status"] != "valid":
            raise ValidationError(f"run '{run['run_id']}' is not valid")
        expected_query_ids = sorted(query_by_id)
        actual_query_ids = [query["query_id"] for query in run["queries"]]
        if actual_query_ids != expected_query_ids:
            raise ValidationError(
                f"run '{run['run_id']}' query order expected {expected_query_ids}, actual {actual_query_ids}"
            )
        trec_lines: list[str] = []
        calculated_metrics[run["run_id"]] = {}
        for actual in run["queries"]:
            query = query_by_id[actual["query_id"]]
            expected_limits = (
                {"keyword": 8, "vector": 8}
                if letter == "c"
                else {"keyword": None, "vector": None}
            )
            assert_equal(
                expected_limits,
                actual["candidate_limits"],
                f"{run['run_id']}.{query['query_id']}.candidate_limits",
                differences,
            )
            assert_equal(
                query["metadata_filter"],
                actual["filter"],
                f"{run['run_id']}.{query['query_id']}.filter",
                differences,
            )
            hits = (
                semantic_hits(chunks, query, "f32" if letter == "a" else "i8")
                if letter in {"a", "b"}
                else hybrid_hits(chunks, query)
            )
            documents, duplicates = project(hits, 10)
            assert_equal(
                hits,
                actual["chunk_hits"],
                f"{run['run_id']}.{query['query_id']}.chunk_hits",
                differences,
            )
            assert_equal(
                documents,
                actual["projected_documents"],
                f"{run['run_id']}.{query['query_id']}.projected_documents",
                differences,
            )
            assert_equal(
                duplicates,
                actual["duplicate_collapse_count"],
                f"{run['run_id']}.{query['query_id']}.duplicate_collapse_count",
                differences,
            )
            calculated_metrics[run["run_id"]][query["query_id"]] = retrieval_metrics(
                documents, qrels[query["query_id"]]
            )
            for document in documents:
                trec_lines.append(
                    f"{query['query_id']} Q0 {document['record_id']} "
                    f"{document['document_rank']} {11 - document['document_rank']} {run['run_id']}\n"
                )
            checked += 1
        trec_path = artifacts / "runs" / f"{run['run_id']}.trec"
        try:
            actual_trec = trec_path.read_text(encoding="utf-8")
        except OSError as error:
            raise ValidationError(f"failed to read TREC run '{trec_path}': {error}") from error
        expected_trec = "".join(trec_lines)
        if actual_trec != expected_trec:
            raise ValidationError(f"TREC projection mismatch for run '{run['run_id']}'")

    metric_run_ids = [run["run_id"] for run in rust_metrics["runs"]]
    if metric_run_ids != FROZEN_RUN_IDS:
        raise ValidationError(
            f"Rust metric run IDs expected {FROZEN_RUN_IDS}, actual {metric_run_ids}"
        )
    for run in rust_metrics["runs"]:
        run_id = run["run_id"]
        query_rows = run["queries"]
        expected_query_metrics = calculated_metrics[run_id]
        if [row["query_id"] for row in query_rows] != sorted(expected_query_metrics):
            raise ValidationError(f"Rust metric query order mismatch for run '{run_id}'")
        ordered_metrics = []
        for row in query_rows:
            expected = expected_query_metrics[row["query_id"]]
            assert_equal(
                expected,
                row["metrics"],
                f"{run_id}.{row['query_id']}.metrics",
                differences,
            )
            ordered_metrics.append(expected)
        assert_equal(
            macro_metrics(ordered_metrics),
            run["macro"],
            f"{run_id}.macro",
            differences,
        )
    return {
        "artifact_schema": REPORT_SCHEMA,
        "checked_query_runs": checked,
        "included_run_letters": expected_letters,
        "partial": True,
        "publication_ready": False,
        "maximum_absolute_differences": differences,
        "score_comparison": {
            "absolute_tolerance": F32_SCORE_TOLERANCE,
            "relative_tolerance": F32_SCORE_TOLERANCE,
        },
        "status": "passed",
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite independent cross-check report '{path}'")
    data = json.dumps(
        report, allow_nan=False, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    )
    path.write_text(data + "\n", encoding="utf-8", newline="\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument(
        "--check-only", action="store_true", help="validate without writing the deterministic report"
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = validate(args.collection.resolve(), args.artifacts.resolve())
        if not args.check_only:
            write_report(args.artifacts.resolve() / REPORT_NAME, report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except ValidationError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
