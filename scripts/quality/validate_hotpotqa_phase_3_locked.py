#!/usr/bin/env python3
"""Independently validate sealed HotpotQA Phase 3b artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

from hotpotqa_phase_3_canonical import canonical

LOCK_SHA256 = "ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118"
COLLECTION_SHA256 = "496d21d1c686e2ef3bc36d9820d0cda058f4ca6b82bb029889ed62b48b084f72"
ADAPTER_SHA256 = "8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6"
TEST_POPULATION_SHA256 = "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010"
DERIVED_POPULATION_SHA256 = "93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8"
TOLERANCE = 1.0e-9
FORBIDDEN_STAGE_A = {
    "qrels.tsv",
    "evidence-judgments.jsonl",
    "expected-paths.jsonl",
}
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


class ValidationError(ValueError):
    pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json(path: Path) -> Any:
    data = path.read_bytes()
    value = json.loads(data)
    if data != canonical(value) + b"\n":
        raise ValidationError(f"non-canonical JSON: {path}")
    return value


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    data = path.read_bytes()
    rows = [json.loads(line) for line in data.splitlines()]
    if data != b"".join(canonical(row) + b"\n" for row in rows):
        raise ValidationError(f"non-canonical JSONL: {path}")
    return rows


def population_hash(query_ids: set[str]) -> str:
    return sha256(b"".join(query.encode() + b"\n" for query in sorted(query_ids)))


def validate_stage_a_audit(audit: dict[str, Any]) -> None:
    opened = set(audit.get("opened_collection_files", []))
    if opened & FORBIDDEN_STAGE_A or audit.get("forbidden_files_opened") != []:
        raise ValidationError("Stage A opened qrels, evidence, or expected paths")
    if audit.get("previous_result_inputs") != [] or audit.get("status") != "passed":
        raise ValidationError("Stage A used a stale prior result or has invalid status")


def validate_stage_b_audit(audit: dict[str, Any]) -> None:
    if audit.get("retrieval_invoked") is not False:
        raise ValidationError("retrieval invoked during Stage B")
    if set(audit.get("opened_label_files", [])) != FORBIDDEN_STAGE_A:
        raise ValidationError("Stage B label access inventory mismatch")


def require_fresh_output(path: Path) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite {path}")


def validate_attempt(audit: dict[str, Any], expected_attempt: int) -> None:
    if audit.get("attempt") != expected_attempt or audit.get("status") != "passed":
        raise ValidationError("second unauthorized reporting attempt or failed attempt")


def validate_rerun_equality(summary: dict[str, Any]) -> None:
    if (
        summary.get("mandatory_ranking_rerun_equal") is not True
        or summary.get("mandatory_scoring_rerun_equal") is not True
    ):
        raise ValidationError("nondeterministic ranking or scored artifact root")


def validate_execution_rows(runs: list[dict[str, Any]]) -> None:
    if any(
        query["execution_status"] == "invalid_execution"
        for run in runs
        for query in run["queries"]
    ):
        raise ValidationError("invalid run publication")


def validate_selected_lock(lock: dict[str, Any], digest: str) -> None:
    selected = lock.get("selected_candidate", {})
    if digest != LOCK_SHA256:
        raise ValidationError("selected lock mismatch")
    if selected != {
        "fusion_alpha": 0.2,
        "fusion_alpha_f32_bits": "3e4ccccd",
        "keyword_candidate_limit": 100,
        "vector_candidate_limit": 100,
    }:
        raise ValidationError("alpha override, candidate-limit override, or extra configuration")


def validate_inventory(root: Path, manifest: dict[str, Any]) -> None:
    actual = []
    for path in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = path.relative_to(root).as_posix()
        if relative == "manifest.json":
            continue
        data = path.read_bytes()
        actual.append({"bytes": len(data), "path": relative, "sha256": sha256(data)})
    if actual != manifest.get("files"):
        raise ValidationError("artifact inventory mismatch or partial artifact publication")
    if sha256(canonical(actual)) != manifest.get("artifact_root_sha256"):
        raise ValidationError("artifact root hash mismatch")


def validate_ranking_seal(root: Path, seal: dict[str, Any]) -> None:
    files = []
    scoring_files = {
        "graph-metrics.json",
        "graph-retrieval-metrics.json",
        "graph-retrieval-paired-comparisons.json",
        "locked-analysis.json",
        "locked-reporting-summary.json",
        "manifest.json",
        "metrics.json",
        "stage-b-file-access-audit.json",
    }
    for path in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = path.relative_to(root).as_posix()
        if relative == "ranking-seal.json" or relative in scoring_files:
            continue
        data = path.read_bytes()
        files.append({"bytes": len(data), "path": relative, "sha256": sha256(data)})
    preimage = seal.get("preimage")
    if not isinstance(preimage, dict) or preimage.get("files") != files:
        raise ValidationError("ranking modification after seal")
    if sha256(canonical(preimage)) != seal.get("ranking_seal_sha256"):
        raise ValidationError("ranking seal construction mismatch")


def parse_qrels(path: Path) -> dict[str, dict[str, int]]:
    result: dict[str, dict[str, int]] = defaultdict(dict)
    previous = None
    for line in path.read_text().splitlines():
        query, zero, document, grade = line.split(" ")
        key = (query, document)
        if zero != "0" or previous is not None and key <= previous:
            raise ValidationError("qrels malformed or reordered")
        previous = key
        result[query][document] = int(grade)
    return dict(result)


def parse_trec(path: Path, run_id: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = defaultdict(list)
    previous = None
    for line in path.read_text().splitlines():
        query, q0, document, rank, score, tag = line.split(" ")
        key = (query, int(rank))
        if q0 != "Q0" or tag != run_id or previous is not None and key <= previous:
            raise ValidationError(f"malformed TREC run: {path}")
        if int(rank) != len(result[query]) + 1 or int(score) <= 0:
            raise ValidationError(f"noncanonical TREC projection: {path}")
        result[query].append(document)
        previous = key
    return dict(result)


def calculate_metrics(documents: list[str], qrels: dict[str, int]) -> dict[str, float]:
    relevant = {document for document, grade in qrels.items() if grade >= 1}

    def relevant_count(cutoff: int) -> int:
        return sum(document in relevant for document in documents[:cutoff])

    def judged(cutoff: int) -> float:
        denominator = min(cutoff, len(documents))
        return (
            sum(document in qrels for document in documents[:cutoff]) / denominator
            if denominator
            else 0.0
        )

    def ndcg(cutoff: int) -> float:
        dcg = 0.0
        for rank, document in enumerate(documents[:cutoff], 1):
            dcg += ((1 << qrels.get(document, 0)) - 1) / math.log2(rank + 1)
        ideal = sorted(qrels.items(), key=lambda row: (-row[1], row[0]))[:cutoff]
        idcg = 0.0
        for rank, (_, grade) in enumerate(ideal, 1):
            idcg += ((1 << grade) - 1) / math.log2(rank + 1)
        return dcg / idcg

    hits = 0
    ap = 0.0
    for rank, document in enumerate(documents, 1):
        if document in relevant:
            hits += 1
            ap += hits / rank
    reciprocal = next(
        (
            1.0 / rank
            for rank, document in enumerate(documents[:10], 1)
            if document in relevant
        ),
        0.0,
    )
    return {
        "ap": ap / len(relevant),
        "judged_at_10": judged(10),
        "judged_at_5": judged(5),
        "mrr_at_10": reciprocal,
        "ndcg_at_10": ndcg(10),
        "ndcg_at_5": ndcg(5),
        "precision_at_5": relevant_count(5) / 5,
        "recall_at_10": relevant_count(10) / len(relevant),
        "recall_at_5": relevant_count(5) / len(relevant),
        "success_at_1": float(relevant_count(1) > 0),
    }


def metric_value(value: Any) -> float | None:
    if isinstance(value, dict):
        if "status" in value and value["status"] != "valid":
            return None
        nested = value.get("value")
        return float(nested) if nested is not None else None
    return float(value)


def compare_metrics(
    published: dict[str, Any],
    rankings: dict[str, list[str]],
    qrels: dict[str, dict[str, int]],
) -> float:
    rows = {row["query_id"]: row for row in published["queries"]}
    calculated = {}
    maximum = 0.0
    for query_id, documents in rankings.items():
        if rows[query_id]["execution_status"] != "valid":
            raise ValidationError("invalid run publication")
        calculated[query_id] = calculate_metrics(documents, qrels[query_id])
        for name in STANDARD_METRICS:
            difference = abs(
                calculated[query_id][name]
                - float(metric_value(rows[query_id]["metrics"][name]))
            )
            maximum = max(maximum, difference)
            if difference > TOLERANCE:
                raise ValidationError(f"independent per-query metric mismatch: {name}")
    for name in STANDARD_METRICS:
        expected = sum(calculated[query][name] for query in sorted(calculated)) / len(
            calculated
        )
        difference = abs(expected - float(metric_value(published["macro"][name])))
        maximum = max(maximum, difference)
        if difference > TOLERANCE:
            raise ValidationError(f"independent aggregate metric mismatch: {name}")
    return maximum


def normalize_graph_row(row: dict[str, Any], *, selection: bool) -> bytes:
    copied = dict(row)
    copied.pop("run_id", None)
    if selection:
        copied.pop("generation_fingerprint", None)
    return canonical(copied)


def validate_graph_equality(root: Path, run_ids: dict[str, str]) -> None:
    selected = {
        row["query_id"]: normalize_graph_row(row, selection=True)
        for row in read_jsonl(root / "graph-selections" / f"{run_ids['d']}.jsonl")
    }
    paths = [
        normalize_graph_row(row, selection=False)
        for row in read_jsonl(root / "graph-paths" / f"{run_ids['d']}.jsonl")
    ]
    for letter in "efg":
        actual_selection = read_jsonl(
            root / "graph-selections" / f"{run_ids[letter]}.jsonl"
        )
        actual_paths = read_jsonl(root / "graph-paths" / f"{run_ids[letter]}.jsonl")
        if any(
            selected[row["query_id"]] != normalize_graph_row(row, selection=True)
            for row in actual_selection
        ) or paths != [normalize_graph_row(row, selection=False) for row in actual_paths]:
            raise ValidationError("graph selection/path equality mismatch")


def validate_persistence(root: Path) -> None:
    for name in (
        "retrieval-persistence-validation.json",
        "graph-persistence-validation.json",
        "graph-retrieval-persistence-validation.json",
    ):
        report = read_json(root / name)
        if report.get("status") != "valid":
            raise ValidationError("failed persistence/reload")
        for row in report["runs"]:
            if any(
                value is not True
                for field, value in row.items()
                if field.endswith("_equal")
                or field in {"save_validate_load_equivalent", "ranking_equal_after_reload"}
            ):
                raise ValidationError("failed persistence/reload")


def validate(
    collection: Path,
    artifacts: Path,
    lock_path: Path,
    authorization_path: Path,
    attempt_audit_path: Path,
) -> dict[str, Any]:
    if sha256((collection / "collection.json").read_bytes()) != COLLECTION_SHA256:
        raise ValidationError("test collection identity mismatch")
    if sha256((collection.parent / "adapter-manifest.json").read_bytes()) != ADAPTER_SHA256:
        raise ValidationError("adapter identity mismatch")
    lock_bytes = lock_path.read_bytes()
    validate_selected_lock(read_json(lock_path), sha256(lock_bytes))
    authorization = read_json(authorization_path)
    authorization_digest = sha256(authorization_path.read_bytes())
    if authorization["authorization_schema"] != "hotpotqa-phase-3b-execution-authorization-v1":
        raise ValidationError("authorization identity mismatch")
    if authorization["selected_configuration"]["lock_sha256"] != LOCK_SHA256:
        raise ValidationError("authorization selected-lock mismatch")
    attempt = read_json(attempt_audit_path)
    validate_attempt(attempt, int(authorization["attempt_sequence"]))
    manifest = read_json(artifacts / "manifest.json")
    validate_inventory(artifacts, manifest)
    if manifest["authorization_sha256"] != authorization_digest:
        raise ValidationError("artifact authorization mismatch")
    seal = read_json(artifacts / "ranking-seal.json")
    validate_ranking_seal(artifacts, seal)
    validate_stage_a_audit(read_json(artifacts / "stage-a-file-access-audit.json"))
    validate_stage_b_audit(read_json(artifacts / "stage-b-file-access-audit.json"))
    validate_rerun_equality(read_json(artifacts / "locked-reporting-summary.json"))
    forbidden_names = {"tuning-summary.json", "selected-configuration-provisional.json"}
    if forbidden_names & {path.name for path in artifacts.rglob("*")}:
        raise ValidationError("tuning/search-space behavior present in locked artifacts")

    queries = {row["query_id"] for row in read_jsonl(collection / "queries.jsonl")}
    exclusions = {
        row["query_id"]
        for row in read_jsonl(collection / "exclusions.jsonl")
        if row["lane"] == "hotpotqa-exact-title-v1"
    }
    if len(queries) != 297 or population_hash(queries) != TEST_POPULATION_SHA256:
        raise ValidationError("test query removal or population mutation")
    derived = queries - exclusions
    if len(exclusions) != 1 or population_hash(derived) != DERIVED_POPULATION_SHA256:
        raise ValidationError("exclusion mutation or derived population mutation")

    configurations = read_json(artifacts / "run-configurations.json")["runs"]
    run_ids = {
        row["configuration"]["run_letter"]: row["run_id"] for row in configurations
    }
    if set(run_ids) != set("abcdefg"):
        raise ValidationError("extra candidate configuration or missing locked run")
    c = next(row for row in configurations if row["configuration"]["run_letter"] == "c")
    g = next(row for row in configurations if row["configuration"]["run_letter"] == "g")
    for field in ("fusion_alpha", "candidate_limits", "bm25_policy"):
        if c["configuration"][field] != g["configuration"][field]:
            raise ValidationError("C/G configuration mismatch")
    qrels = parse_qrels(collection / "qrels.tsv")
    metric_runs = {row["run_id"]: row for row in read_json(artifacts / "metrics.json")["runs"]}
    graph_runs = {
        row["run_id"]: row
        for row in read_json(artifacts / "graph-retrieval-metrics.json")["runs"]
    }
    maximum = 0.0
    for letter in "abcefg":
        rankings = parse_trec(artifacts / "runs" / f"{run_ids[letter]}.trec", run_ids[letter])
        expected = queries if letter in "abc" else derived
        if set(rankings) != expected:
            raise ValidationError("run population and TREC population disagree")
        maximum = max(
            maximum,
            compare_metrics(
                (metric_runs if letter in "abc" else graph_runs)[run_ids[letter]],
                rankings,
                qrels,
            ),
        )
    validate_graph_equality(artifacts, run_ids)
    validate_persistence(artifacts)
    equality = read_json(artifacts / "graph-retrieval-selection-path-equality.json")
    if equality.get("status") != "valid":
        raise ValidationError("published graph equality status is invalid")
    raw = read_json(artifacts / "rust-results.json")["runs"] + read_json(
        artifacts / "graph-retrieval-rust-results.json"
    )["runs"]
    validate_execution_rows(raw)
    analysis = read_json(artifacts / "locked-analysis.json")
    if analysis.get("status") != "valid" or analysis["errors"][
        "unexpected execution failure"
    ]["count"]:
        raise ValidationError("closed error analysis is invalid")
    return {
        "artifact_schema": "hotpotqa-phase-3b-independent-validation-v1",
        "authorization_sha256": authorization_digest,
        "checks": {
            "artifact_inventory": "passed",
            "authorization": "passed",
            "canonical_serialization": "passed",
            "configuration_and_no_tuning": "passed",
            "graph_selection_and_paths": "passed",
            "label_isolation": "passed",
            "persistence_reload": "passed",
            "ranking_seal": "passed",
            "run_populations": "passed",
        },
        "maximum_metric_difference": maximum,
        "run_ids": {letter.upper(): run_id for letter, run_id in run_ids.items()},
        "status": "passed",
        "unsupported_external_mappings": [
            "Judged metrics in official trec_eval",
            "supporting-document and complete-evidence metrics",
            "candidate and graph-scope metrics",
            "path accuracy",
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--lock", type=Path, required=True)
    parser.add_argument("--authorization", type=Path, required=True)
    parser.add_argument("--attempt-audit", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        output = args.output.resolve()
        require_fresh_output(output)
        report = validate(
            args.collection.resolve(),
            args.artifacts.resolve(),
            args.lock.resolve(),
            args.authorization.resolve(),
            args.attempt_audit.resolve(),
        )
        output.write_bytes(canonical(report) + b"\n")
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, KeyError, TypeError, ValueError, ValidationError) as error:
        print(f"error: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
