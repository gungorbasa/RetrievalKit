#!/usr/bin/env python3
"""Independently validate HotpotQA Phase 3a tuning and development artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = (
    ROOT
    / "target/benchmarks/public-collections/hotpotqa-linked-abstracts-graph-v1/development"
)
DEFAULT_TUNING = ROOT / "target/benchmarks/hotpotqa-phase-3a/tuning"
DEFAULT_MATRIX = ROOT / "target/benchmarks/hotpotqa-phase-3a/development-matrix"
DEFAULT_SEARCH = (
    ROOT / "benchmarks/retrieval-quality/hotpotqa/phase-3-development-search-space.json"
)
DEFAULT_LOCK = (
    ROOT / "benchmarks/retrieval-quality/hotpotqa/phase-3-selected-configuration.json"
)
SEARCH_SHA256 = "30a93141c0b36d446617342ae846ff4174ff1f8b0f0f9cf008882ed6f3cbdeca"
LOCK_SHA256 = "ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118"
COLLECTION_SHA256 = "4ec8a04401149b04718f28b465809bd788a170c1089df5fe5e68e1ca991d633d"
ADAPTER_SHA256 = "8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6"
TOLERANCE = 1.0e-9
OBJECTIVE = (
    "complete_evidence_recall_at_10",
    "ndcg_at_10",
    "map",
    "recall_at_10",
    "mrr_at_10",
)
STANDARD_METRICS = (
    "ap",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
)


class ValidationError(RuntimeError):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: Any) -> bytes:
    return json.dumps(
        value,
        allow_nan=False,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()


def guard_development_path(path: Path) -> None:
    if "test" in path.parts:
        raise ValidationError(f"sealed test path rejected before access: {path}")


def read_json(path: Path) -> Any:
    guard_development_path(path)
    data = path.read_bytes()
    value = json.loads(data)
    if data != canonical(value) + b"\n":
        raise ValidationError(f"noncanonical JSON: {path}")
    return value


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    guard_development_path(path)
    data = path.read_bytes()
    if data and not data.endswith(b"\n"):
        raise ValidationError(f"JSONL lacks LF: {path}")
    rows = [json.loads(line) for line in data.splitlines()]
    if data != b"".join(canonical(row) + b"\n" for row in rows):
        raise ValidationError(f"noncanonical JSONL: {path}")
    return rows


def inventory(root: Path, manifest: dict[str, Any]) -> None:
    expected = {row["path"]: (row["bytes"], row["sha256"]) for row in manifest["files"]}
    actual = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValidationError(f"symlink in artifact root: {path}")
        if path.is_file() and path != root / "manifest.json":
            data = path.read_bytes()
            actual[path.relative_to(root).as_posix()] = (len(data), sha256(data))
    if actual != expected:
        raise ValidationError(f"artifact inventory mismatch: {root}")


def recursive_identity(left: Path, right: Path) -> dict[str, Any]:
    def files(root: Path) -> dict[str, tuple[int, str]]:
        guard_development_path(root)
        return {
            path.relative_to(root).as_posix(): (
                path.stat().st_size,
                sha256(path.read_bytes()),
            )
            for path in sorted(root.rglob("*"))
            if path.is_file()
        }

    left_files = files(left)
    right_files = files(right)
    if left_files != right_files:
        raise ValidationError(
            "fresh development reruns are not recursively byte-identical"
        )
    return {
        "byte_identical": True,
        "file_count": len(left_files),
        "root_sha256": sha256(canonical(left_files)),
    }


def candidate_key(row: dict[str, Any]) -> tuple[Any, ...]:
    candidate = row["candidate"]
    vector = candidate["vector_candidate_limit"]
    keyword = candidate["keyword_candidate_limit"]
    return (
        *(-float(row["aggregate"][name]) for name in OBJECTIVE),
        vector + keyword,
        max(vector, keyword),
        canonical(candidate),
    )


def validate_candidate_closure(
    registered: Iterable[Any], observed: Iterable[Any]
) -> None:
    left = {canonical(row) for row in registered}
    right = {canonical(row) for row in observed}
    if left != right:
        raise ValidationError(
            f"candidate closure mismatch: missing={len(left - right)}, extra={len(right - left)}"
        )


def mechanical_winner(rows: list[dict[str, Any]]) -> dict[str, Any]:
    if not rows:
        raise ValidationError("missing tuning candidates")
    return min(rows, key=candidate_key)


def metric_value(value: Any) -> float | None:
    if isinstance(value, dict):
        return None if value.get("status", "valid") != "valid" else value.get("value")
    return float(value)


def assert_close(expected: float, actual: float, label: str) -> float:
    difference = abs(expected - actual)
    if difference > TOLERANCE:
        raise ValidationError(
            f"{label}: metric difference {difference} exceeds {TOLERANCE}"
        )
    return difference


def parse_qrels(path: Path) -> dict[str, dict[str, int]]:
    guard_development_path(path)
    result: dict[str, dict[str, int]] = defaultdict(dict)
    previous = None
    for line in path.read_text().splitlines():
        query, zero, document, grade = line.split(" ")
        key = (query, document)
        if zero != "0" or previous is not None and key <= previous:
            raise ValidationError("qrels are malformed, duplicated, or reordered")
        previous = key
        result[query][document] = int(grade)
    return dict(result)


def parse_trec(path: Path, run_id: str) -> dict[str, list[str]]:
    guard_development_path(path)
    rows: dict[str, list[str]] = defaultdict(list)
    previous = None
    for line in path.read_text().splitlines():
        query, q0, document, rank, score, tag = line.split(" ")
        rank_value = int(rank)
        key = (query, rank_value)
        if q0 != "Q0" or tag != run_id or previous is not None and key <= previous:
            raise ValidationError(f"malformed or reordered TREC row: {path}")
        if rank_value != len(rows[query]) + 1 or int(score) <= 0:
            raise ValidationError(f"non-contiguous TREC ranking: {path}")
        previous = key
        rows[query].append(document)
    return dict(rows)


def retrieval_metrics(documents: list[str], qrels: dict[str, int]) -> dict[str, float]:
    relevant = {document for document, grade in qrels.items() if grade >= 1}

    def recall(k: int) -> float:
        return len(relevant & set(documents[:k])) / len(relevant) if relevant else 0.0

    def judged(k: int) -> float:
        denominator = min(k, len(documents))
        return (
            sum(document in qrels for document in documents[:k]) / denominator
            if denominator
            else 0.0
        )

    def ndcg(k: int) -> float:
        dcg = sum(
            ((1 << qrels.get(document, 0)) - 1) / math.log2(rank + 1)
            for rank, document in enumerate(documents[:k], 1)
        )
        ideal = sorted(qrels.values(), reverse=True)[:k]
        idcg = sum(
            ((1 << grade) - 1) / math.log2(rank + 1)
            for rank, grade in enumerate(ideal, 1)
        )
        return dcg / idcg if idcg else 0.0

    hits = 0
    ap = 0.0
    for rank, document in enumerate(documents, 1):
        if document in relevant:
            hits += 1
            ap += hits / rank
    reciprocal = next(
        (
            1 / rank
            for rank, document in enumerate(documents[:10], 1)
            if document in relevant
        ),
        0.0,
    )
    return {
        "ap": ap / len(relevant) if relevant else 0.0,
        "judged_at_10": judged(10),
        "judged_at_5": judged(5),
        "mrr_at_10": reciprocal,
        "ndcg_at_10": ndcg(10),
        "ndcg_at_5": ndcg(5),
        "precision_at_5": len(relevant & set(documents[:5])) / 5,
        "recall_at_10": recall(10),
        "recall_at_5": recall(5),
        "success_at_1": float(bool(documents and documents[0] in relevant)),
    }


def validate_metrics(
    run: dict[str, Any],
    rankings: dict[str, list[str]],
    qrels: dict[str, dict[str, int]],
) -> float:
    maximum = 0.0
    calculated = []
    query_rows = {row["query_id"]: row for row in run["queries"]}
    for query_id, documents in rankings.items():
        expected = retrieval_metrics(documents, qrels[query_id])
        actual = query_rows[query_id]
        if actual["execution_status"] != "valid":
            raise ValidationError(
                f"valid TREC query marked {actual['execution_status']}"
            )
        for name in STANDARD_METRICS:
            maximum = max(
                maximum,
                assert_close(
                    expected[name],
                    float(metric_value(actual["metrics"][name])),
                    f"{run['run_id']}/{query_id}/{name}",
                ),
            )
        calculated.append(expected)
    for name in STANDARD_METRICS:
        aggregate = sum(row[name] for row in calculated) / len(calculated)
        maximum = max(
            maximum,
            assert_close(
                aggregate,
                float(metric_value(run["macro"][name])),
                f"{run['run_id']}/macro/{name}",
            ),
        )
    return maximum


def validate_persistence(matrix: Path) -> None:
    for name in (
        "graph-persistence-validation.json",
        "graph-retrieval-persistence-validation.json",
    ):
        rows = read_json(matrix / name)["runs"]
        for row in rows:
            for field, value in row.items():
                if field.endswith("_equal") or field == "save_validate_load_equivalent":
                    if value is not True:
                        raise ValidationError(
                            f"persistence failure {row['run_id']}/{field}"
                        )


def normalized_graph_row(row: dict[str, Any], *, selection: bool) -> bytes:
    copy = dict(row)
    copy.pop("run_id", None)
    if selection:
        copy.pop("generation_fingerprint", None)
    return canonical(copy)


def validate_graph_equality(matrix: Path, run_ids: dict[str, str]) -> dict[str, Any]:
    d_selections = read_jsonl(matrix / "graph-selections" / f"{run_ids['d']}.jsonl")
    d_paths = read_jsonl(matrix / "graph-paths" / f"{run_ids['d']}.jsonl")
    by_query_selection = {
        row["query_id"]: normalized_graph_row(row, selection=True)
        for row in d_selections
    }
    normalized_d_paths = [normalized_graph_row(row, selection=False) for row in d_paths]
    counts = []
    for letter in "efg":
        selections = read_jsonl(
            matrix / "graph-selections" / f"{run_ids[letter]}.jsonl"
        )
        paths = read_jsonl(matrix / "graph-paths" / f"{run_ids[letter]}.jsonl")
        if any(
            by_query_selection[row["query_id"]]
            != normalized_graph_row(row, selection=True)
            for row in selections
        ):
            raise ValidationError(f"graph selection mismatch D/{letter.upper()}")
        if normalized_d_paths != [
            normalized_graph_row(row, selection=False) for row in paths
        ]:
            raise ValidationError(f"graph path mismatch D/{letter.upper()}")
        counts.append(
            {"letter": letter, "paths": len(paths), "selections": len(selections)}
        )
    return {"comparisons": counts, "status": "passed"}


def validate_tuning(
    search: dict[str, Any],
    tuning: Path,
    lock: dict[str, Any],
    qrels: dict[str, dict[str, int]],
) -> dict[str, Any]:
    manifest = read_json(tuning / "manifest.json")
    inventory(tuning, manifest)
    summary = read_json(tuning / "tuning-summary.json")
    rows = summary["candidates"]
    validate_candidate_closure(search["candidates"], [row["candidate"] for row in rows])
    if len(rows) != 36:
        raise ValidationError(f"expected 36 candidates, actual {len(rows)}")
    maximum = 0.0
    for row in rows:
        candidate_root = tuning / "candidates" / row["run_id"]
        candidate_manifest = read_json(candidate_root / "manifest.json")
        inventory(candidate_root, candidate_manifest)
        configuration = read_json(candidate_root / "configuration.json")
        metrics = read_json(candidate_root / "metrics.json")
        persistence = read_json(candidate_root / "persistence.json")
        if (
            configuration["candidate"] != row["candidate"]
            or metrics["aggregate"] != row["aggregate"]
        ):
            raise ValidationError(f"candidate artifact mismatch: {row['run_id']}")
        if any(
            persistence.get(field) is not True
            for field in (
                "deterministic_repeat_equal",
                "ranking_equal_after_reload",
                "save_validate_load_equivalent",
            )
        ):
            raise ValidationError(f"candidate persistence failure: {row['run_id']}")
        rankings = parse_trec(candidate_root / "run.trec", row["run_id"])
        if len(rankings) != 603:
            raise ValidationError(
                f"candidate TREC population mismatch: {row['run_id']}"
            )
        query_metrics = {
            metric_row["query_id"]: metric_row for metric_row in metrics["per_query"]
        }
        calculated = []
        for query_id, documents in rankings.items():
            expected = retrieval_metrics(documents, qrels[query_id])
            actual = query_metrics[query_id]
            if actual["execution_status"] != "valid":
                raise ValidationError(
                    f"candidate query invalid: {row['run_id']}/{query_id}"
                )
            for name in STANDARD_METRICS:
                maximum = max(
                    maximum,
                    assert_close(
                        expected[name],
                        float(actual["metrics"][name]),
                        f"{row['run_id']}/{query_id}/{name}",
                    ),
                )
            calculated.append(expected)
        for name in STANDARD_METRICS:
            expected = sum(metric[name] for metric in calculated) / len(calculated)
            actual_name = "map" if name == "ap" else name
            maximum = max(
                maximum,
                assert_close(
                    expected,
                    float(metrics["aggregate"][actual_name]),
                    f"{row['run_id']}/aggregate/{actual_name}",
                ),
            )
    winner = mechanical_winner(rows)
    selected = {
        name: lock["selected_candidate"][name]
        for name in (
            "fusion_alpha",
            "keyword_candidate_limit",
            "vector_candidate_limit",
        )
    }
    if winner["candidate"] != selected:
        raise ValidationError("mechanical winner differs from selected lock")
    return {
        "candidate_count": len(rows),
        "maximum_metric_difference": maximum,
        "winner": winner["candidate"],
    }


def slice_bucket(value: float) -> str:
    if value == 0:
        return "0"
    if value < 0.5:
        return "(0,0.5)"
    if value < 1:
        return "[0.5,1)"
    return "1"


def analyze_slices_and_errors(
    collection: Path, matrix: Path, run_ids: dict[str, str]
) -> dict[str, Any]:
    queries = {row["query_id"]: row for row in read_jsonl(collection / "queries.jsonl")}
    evidence = {
        row["query_id"]: row
        for row in read_jsonl(collection / "evidence-judgments.jsonl")
    }
    exclusions = read_jsonl(collection / "exclusions.jsonl")
    excluded_ids = {row["query_id"] for row in exclusions if row["lane"] != "global"}
    baseline = {
        row["run_id"].split("-", 2)[1]: row
        for row in read_json(matrix / "metrics.json")["runs"]
    }
    graph = {
        row["run_id"].split("-", 2)[1]: row
        for row in read_json(matrix / "graph-retrieval-metrics.json")["runs"]
    }
    graph_queries = {
        letter: {row["query_id"]: row for row in graph[letter]["queries"]}
        for letter in "efg"
    }
    selections = {
        row["query_id"]: row
        for row in read_jsonl(matrix / "graph-selections" / f"{run_ids['d']}.jsonl")
    }
    slices: dict[str, Counter[str]] = defaultdict(Counter)
    errors: dict[str, set[str]] = defaultdict(set)
    for query_id, query in queries.items():
        category = query["category"]
        question_type, difficulty = category.split(":", 1)
        slices["question_type"][question_type] += 1
        slices["difficulty"][difficulty] += 1
        slices["category"][category] += 1
        evidence_documents = {
            document
            for evidence_set in evidence[query_id]["evidence_sets"]
            for document in evidence_set
        }
        slices["evidence_document_count"][str(len(evidence_documents))] += 1
        if query_id in excluded_ids:
            slices["seed_status"]["excluded_ambiguous"] += 1
            errors["seed ambiguity exclusion"].add(query_id)
            continue
        slices["seed_status"]["resolved"] += 1
        selected = selections[query_id]
        count = selected["projected_chunks_after_filter"]
        scope = (
            "empty"
            if count == 0
            else "1-10"
            if count <= 10
            else "11-100"
            if count <= 100
            else "101-1000"
            if count <= 1000
            else "1001-10000"
            if count <= 10000
            else "above-10000"
        )
        slices["candidate_scope_size"][scope] += 1
        slices["graph_truncation_reason"][selected["truncated_reason"] or "none"] += 1
        candidate_recall = float(
            metric_value(graph_queries["e"][query_id]["metrics"]["candidate_recall"])
        )
        slices["candidate_recall_bucket"][slice_bucket(candidate_recall)] += 1
        slices["filter_selectivity"]["not_applicable"] += 1
        slices["path_accuracy"]["not_applicable"] += 1
        if count == 0:
            errors["empty graph scope"].add(query_id)
        if selected["truncated_reason"] is not None:
            errors["truncated graph selection"].add(query_id)
        if candidate_recall < 1:
            errors["supporting evidence absent from scope"].add(query_id)
        if (
            candidate_recall == 1
            and metric_value(
                graph_queries["g"][query_id]["metrics"][
                    "complete_evidence_recall_at_10"
                ]
            )
            == 0
        ):
            errors["evidence in scope but missing from top 10"].add(query_id)
    for letter, label in (
        ("a", "whole-corpus ranking failure"),
        ("e", "graph-scoped ranking failure"),
    ):
        source = baseline[letter] if letter == "a" else graph[letter]
        for row in source["queries"]:
            metric = row["metrics"].get(
                "complete_evidence_recall_at_10", row["metrics"].get("recall_at_10")
            )
            if row["execution_status"] == "valid" and metric_value(metric) == 0:
                errors[label].add(row["query_id"])
    for left, right, label in (
        ("a", "b", "I8 ranking divergence"),
        ("e", "f", "I8 ranking divergence"),
        ("b", "c", "hybrid ranking regression"),
        ("f", "g", "hybrid ranking regression"),
    ):
        source = baseline if left in baseline else graph
        lrows = {row["query_id"]: row for row in source[left]["queries"]}
        rrows = {row["query_id"]: row for row in source[right]["queries"]}
        for query_id in lrows.keys() & rrows.keys():
            lv = metric_value(lrows[query_id]["metrics"]["ndcg_at_10"])
            rv = metric_value(rrows[query_id]["metrics"]["ndcg_at_10"])
            if lv is None or rv is None:
                continue
            if (
                label.startswith("I8")
                and lv != rv
                or label.startswith("hybrid")
                and rv < lv
            ):
                errors[label].add(query_id)
    for source in list(baseline.values()) + list(graph.values()):
        for row in source["queries"]:
            if row["execution_status"] == "invalid_execution":
                errors["unexpected execution failure"].add(row["query_id"])
    for category in (
        "seed ambiguity exclusion",
        "empty graph scope",
        "truncated graph selection",
        "supporting evidence absent from scope",
        "evidence in scope but missing from top 10",
        "whole-corpus ranking failure",
        "graph-scoped ranking failure",
        "I8 ranking divergence",
        "hybrid ranking regression",
        "duplicate-collapse effect",
        "unexpected execution failure",
    ):
        errors.setdefault(category, set())

    category_metrics: dict[str, Any] = {}
    for letter, source in {**baseline, **graph}.items():
        grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for row in source["queries"]:
            if row["execution_status"] == "valid":
                grouped[queries[row["query_id"]]["category"]].append(row["metrics"])
        category_metrics[letter] = {}
        for category, rows in sorted(grouped.items()):
            values = {
                "ndcg_at_10": [metric_value(row["ndcg_at_10"]) for row in rows],
            }
            if "complete_evidence_recall_at_10" in rows[0]:
                values["complete_evidence_recall_at_10"] = [
                    metric_value(row["complete_evidence_recall_at_10"]) for row in rows
                ]
            category_metrics[letter][category] = {
                "executed_queries": len(rows),
                **{
                    name: sum(
                        float(value) for value in metric_values if value is not None
                    )
                    / sum(value is not None for value in metric_values)
                    for name, metric_values in values.items()
                },
            }
    return {
        "errors": {
            name: {"count": len(ids), "query_ids": sorted(ids)}
            for name, ids in sorted(errors.items())
        },
        "slices": {
            name: dict(sorted(counts.items()))
            for name, counts in sorted(slices.items())
        },
        "slice_metrics_by_category": category_metrics,
    }


def validate_all(
    collection: Path, tuning: Path, matrix: Path, search_path: Path, lock_path: Path
) -> dict[str, Any]:
    for path in (collection, tuning, matrix, search_path, lock_path):
        guard_development_path(path)
    if sha256((collection / "collection.json").read_bytes()) != COLLECTION_SHA256:
        raise ValidationError("development collection hash mismatch")
    if (
        sha256((collection.parent / "adapter-manifest.json").read_bytes())
        != ADAPTER_SHA256
    ):
        raise ValidationError("adapter manifest hash mismatch")
    if sha256(search_path.read_bytes()) != SEARCH_SHA256:
        raise ValidationError("search-space hash mismatch")
    if sha256(lock_path.read_bytes()) != LOCK_SHA256:
        raise ValidationError("lock hash mismatch")
    search = read_json(search_path)
    lock = read_json(lock_path)
    qrels = parse_qrels(collection / "qrels.tsv")
    tuning_report = validate_tuning(search, tuning, lock, qrels)
    manifest = read_json(matrix / "manifest.json")
    inventory(matrix, manifest)
    audit = read_json(matrix / "test-access-audit.json")
    if audit != {
        "collection_id": "hotpotqa-linked-abstracts-graph-v1-development",
        "opened_splits": ["development"],
        "schema_version": 1,
        "test_artifacts_generated": False,
        "test_collection_opened": False,
        "test_metrics_inspected": False,
    }:
        raise ValidationError("test-access audit mismatch")
    matrix_summary = read_json(matrix / "phase-3a-development-matrix.json")
    run_ids = {row["letter"]: row["run_id"] for row in matrix_summary["run_ids"]}
    if set(run_ids) != set("abcdefg") or matrix_summary["executed_counts"] != {
        "a": 603,
        "b": 603,
        "c": 603,
        "d": 599,
        "e": 599,
        "f": 599,
        "g": 599,
    }:
        raise ValidationError("matrix population mismatch")
    configurations = {
        row["configuration"]["run_letter"]: row
        for row in read_json(matrix / "run-configurations.json")["runs"]
    }
    for field in ("fusion_alpha", "candidate_limits", "bm25_policy"):
        if (
            configurations["c"]["configuration"][field]
            != configurations["g"]["configuration"][field]
        ):
            raise ValidationError(f"C/G mismatch: {field}")
    metric_runs = {
        row["run_id"]: row for row in read_json(matrix / "metrics.json")["runs"]
    }
    graph_runs = {
        row["run_id"]: row
        for row in read_json(matrix / "graph-retrieval-metrics.json")["runs"]
    }
    maximum = 0.0
    for letter in "abcefg":
        rankings = parse_trec(
            matrix / "runs" / f"{run_ids[letter]}.trec", run_ids[letter]
        )
        expected_count = 603 if letter in "abc" else 599
        if len(rankings) != expected_count:
            raise ValidationError(f"Run {letter.upper()} TREC population mismatch")
        maximum = max(
            maximum,
            validate_metrics(
                (metric_runs if letter in "abc" else graph_runs)[run_ids[letter]],
                rankings,
                qrels,
            ),
        )
    validate_persistence(matrix)
    graph_report = validate_graph_equality(matrix, run_ids)
    equality = read_json(matrix / "graph-retrieval-selection-path-equality.json")
    if equality["status"] != "valid" or any(
        not row["selection_equal"] or not row["path_equal"] for row in equality["runs"]
    ):
        raise ValidationError("published graph equality failure")
    analyses = analyze_slices_and_errors(collection, matrix, run_ids)
    if analyses["errors"].get("unexpected execution failure", {}).get("count", 0):
        raise ValidationError("invalid execution present")
    return {
        "artifact_schema": "hotpotqa-phase-3a-independent-cross-check-v1",
        "checks": {
            "artifact_inventories": "passed",
            "configuration_lock": "passed",
            "deterministic_serialization": "passed",
            "development_input_hashes": "passed",
            "graph_selection_and_paths": graph_report,
            "mechanical_selection": tuning_report,
            "metrics_and_rankings": "passed",
            "no_test_access": "passed",
            "persistence": "passed",
            "run_populations": "passed",
        },
        "maximum_metric_difference": maximum,
        "run_ids": run_ids,
        "unsupported_external_mappings": [
            "supporting-document and complete-evidence metrics",
            "candidate and graph-scope metrics",
            "path accuracy",
        ],
        **analyses,
        "status": "passed",
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--tuning", type=Path, default=DEFAULT_TUNING)
    parser.add_argument("--matrix", type=Path, default=DEFAULT_MATRIX)
    parser.add_argument("--search-space", type=Path, default=DEFAULT_SEARCH)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--rerun-roots", nargs=2, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = validate_all(
            *(
                path.resolve()
                for path in (
                    args.collection,
                    args.tuning,
                    args.matrix,
                    args.search_space,
                    args.lock,
                )
            )
        )
        if args.rerun_roots:
            report["fresh_reruns"] = recursive_identity(
                *(path.resolve() for path in args.rerun_roots)
            )
        data = canonical(report) + b"\n"
        if args.output:
            output = args.output.resolve()
            guard_development_path(output)
            if output.exists():
                raise ValidationError(f"refusing to overwrite {output}")
            output.write_bytes(data)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, KeyError, TypeError, ValueError, ValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
