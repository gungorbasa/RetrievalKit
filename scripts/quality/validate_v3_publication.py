#!/usr/bin/env python3
"""Independently validate a complete canonical V3 public artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

if __package__:
    from . import bootstrap_v3_trec_eval as trec_bootstrap
    from . import validate_v3_conformance as foundation
    from .validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from .validate_v3_phase_1_2b import canonical_bytes
else:
    import bootstrap_v3_trec_eval as trec_bootstrap
    import validate_v3_conformance as foundation
    from validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from validate_v3_phase_1_2b import canonical_bytes


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
MANIFEST_KEYS = {
    "collection_id",
    "collection_version",
    "determinism_context",
    "determinism_environment",
    "deterministic_files",
    "files",
    "generation_fingerprints",
    "implementation_revision",
    "metric_definition_version",
    "population_hashes",
    "profile",
    "publication_status",
    "run_configurations",
    "schema_version",
}
RESULT_KEYS = {"collection_id", "collection_version", "runs", "schema_version", "seed_resolutions"}
RESULT_RUN_KEYS = {"queries", "run_id", "status"}
RESULT_QUERY_KEYS = {
    "candidate_limits",
    "chunk_hits",
    "duplicate_collapse_count",
    "execution_status",
    "filter",
    "projected_documents",
    "query_id",
    "selection_run_id",
    "status_reason",
}
CHUNK_HIT_KEYS = {
    "bm25_normalized_score",
    "bm25_score",
    "chunk_key",
    "fusion_score",
    "keyword_rank",
    "matched_terms",
    "native_rank",
    "record_id",
    "vector_normalized_score",
    "vector_rank",
    "vector_score",
}
DOCUMENT_HIT_KEYS = {"chunk_key", "document_rank", "native_chunk_rank", "record_id", "score"}
METRICS_KEYS = {
    "collection_id",
    "collection_version",
    "exclusions",
    "metric_definition_version",
    "paired_comparisons",
    "publication_status",
    "runs",
    "schema_version",
    "seed_resolution_coverage",
}
METRIC_RUN_KEYS = {
    "counts",
    "declared_population_sha256",
    "execution_population_sha256",
    "macro",
    "micro",
    "queries",
    "run_id",
    "status",
}
METRIC_NAMES = {
    "ap",
    "candidate_complete_evidence",
    "candidate_recall",
    "candidate_reduction_ratio",
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "empty_scope",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "path_accuracy",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
    "truncated",
    "truncated_max_hops",
    "truncated_max_results",
    "truncated_max_visited",
    "truncated_max_working_bytes",
}
MICRO_KEYS = {
    "candidate_recall",
    "candidate_reduction_ratio",
    "empty_scope_rate",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
    "truncation_rate",
    "truncation_rate_max_hops",
    "truncation_rate_max_results",
    "truncation_rate_max_visited",
    "truncation_rate_max_working_bytes",
}
SELECTION_KEYS = {
    "active_corpus_chunks_before_filter",
    "corpus_id",
    "eligible_corpus_chunks_after_filter",
    "generation_fingerprint",
    "matched_nodes",
    "projected_chunks_after_filter",
    "projected_chunks_before_filter",
    "projected_documents_after_filter",
    "query_id",
    "resolved_seed",
    "run_id",
    "seed_lane",
    "seed_provenance",
    "seed_status",
    "stale",
    "trace",
    "truncated_reason",
}
PATH_KEYS = {"depth", "edges", "matched_node", "path_ordinal", "query_id", "run_id"}
RUN_LINE = re.compile(
    rb"([A-Za-z0-9][A-Za-z0-9._:-]{0,127}) Q0 "
    rb"([A-Za-z0-9][A-Za-z0-9._:-]{0,127}) ([1-9][0-9]*) ([1-9][0-9]*) "
    rb"([a-z0-9][a-z0-9-]{0,95})\n"
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def require_keys(value: Any, keys: set[str], label: str) -> None:
    require(isinstance(value, dict), f"{label} expected object")
    require(set(value) == keys, f"{label} closed schema mismatch")


def collection_files(collection: Path) -> dict[str, bytes]:
    header = read_json(collection / "collection.json")
    files = {row["path"]: (collection / row["path"]).read_bytes() for row in header["files"]}
    files["collection.json"] = (collection / "collection.json").read_bytes()
    return files


def canonical_json_file(path: Path) -> Any:
    data = path.read_bytes()
    value = json.loads(data)
    require(data == canonical_bytes(value) + b"\n", f"noncanonical serialization in '{path}'")
    return value


def canonical_jsonl_file(path: Path) -> list[Any]:
    data = path.read_bytes()
    if not data:
        return []
    require(data.endswith(b"\n"), f"JSONL file '{path}' does not end in LF")
    rows = []
    for line in data.splitlines(keepends=True):
        value = json.loads(line)
        require(line == canonical_bytes(value) + b"\n", f"noncanonical serialization in '{path}'")
        rows.append(value)
    return rows


def expected_paths(runs: list[dict[str, Any]]) -> set[str]:
    ranked = [row["run_id"] for row in runs if row["configuration"]["run_letter"] in "abcefg"]
    graph = [row["run_id"] for row in runs if row["configuration"]["run_letter"] in "defg"]
    return {
        "qrels.tsv",
        "evidence-judgments.jsonl",
        "expected-paths.jsonl",
        "exclusions.jsonl",
        "rust-results.json",
        "metrics.json",
        "timing-samples.jsonl",
        "manifest.json",
        *{f"runs/{run_id}.trec" for run_id in ranked},
        *{f"graph-selections/{run_id}.jsonl" for run_id in graph},
        *{f"graph-paths/{run_id}.jsonl" for run_id in graph},
    }


def validate_inventory(root: Path, expected: set[str]) -> None:
    require(root.is_dir() and not root.is_symlink(), f"artifact root is not a regular directory: '{root}'")
    symlinks = [path for path in root.rglob("*") if path.is_symlink()]
    require(not symlinks, "public artifact contains symlinks")
    actual_files = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
    actual_directories = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_dir()}
    missing = sorted(expected - actual_files)
    extra = sorted(actual_files - expected)
    require(not missing, f"public artifact is missing files {missing}")
    require(not extra, f"public artifact has extra files {extra}")
    require(
        actual_directories == {"graph-paths", "graph-selections", "runs"},
        f"public artifact has unexpected directories {sorted(actual_directories)}",
    )
    require(len(actual_files) == 44, f"public artifact expected 44 files, actual {len(actual_files)}")


def validate_manifest_file_index(root: Path, manifest: dict[str, Any]) -> None:
    indexed = []
    for path in sorted(path for path in root.rglob("*") if path.is_file() and path.name != "manifest.json"):
        data = path.read_bytes()
        indexed.append(
            {"bytes": len(data), "path": path.relative_to(root).as_posix(), "sha256": sha256(data)}
        )
    require(manifest["files"] == indexed, "manifest file digest or byte-count index mismatch")
    paths = [row["path"] for row in indexed]
    require(manifest["deterministic_files"] == paths, "manifest deterministic file set mismatch")
    require(len(indexed) == 43, "manifest must index exactly 43 files")


def validate_revision(
    manifest: dict[str, Any], executable: Path, repository: Path
) -> None:
    revision = manifest["implementation_revision"]
    require_keys(revision, {"binary_sha256", "git_commit", "source_sha256"}, "implementation revision")
    require(revision["source_sha256"] is None, "clean publication source_sha256 must be null")
    require(revision["binary_sha256"] == sha256(executable.read_bytes()), "incorrect binary digest")
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=repository, capture_output=True, check=False, text=True
    )
    require(head.returncode == 0, "failed to inspect repository HEAD")
    require(revision["git_commit"] == head.stdout.strip(), "wrong implementation revision")
    require(bool(re.fullmatch(r"[0-9a-f]{40}", revision["git_commit"])), "invalid Git revision")


def validate_environment(manifest: dict[str, Any]) -> None:
    environment = manifest["determinism_environment"]
    require_keys(
        environment,
        {
            "cpu_architecture",
            "cpu_features",
            "execution_threads",
            "floating_point_mode",
            "locale",
            "os_build",
            "runtime_flags",
        },
        "determinism environment",
    )
    require(environment["execution_threads"] > 0, "invalid execution-thread count")
    require(environment["locale"] == "C", "quality locale must be C")
    require(
        environment["floating_point_mode"] == "round_to_nearest_ties_to_even",
        "invalid floating-point mode",
    )
    require(environment["cpu_features"] == sorted(set(environment["cpu_features"])), "CPU features are not canonical")
    require(environment["runtime_flags"] == sorted(set(environment["runtime_flags"])), "runtime flags are not canonical")
    context = manifest["determinism_context"]
    require_keys(
        context,
        {"binary_sha256", "environment_sha256", "runtime_id", "runtime_version", "target_triple"},
        "determinism context",
    )
    require(
        context["environment_sha256"] == sha256(canonical_bytes(environment)),
        "incorrect environment digest",
    )
    require(
        context["binary_sha256"] == manifest["implementation_revision"]["binary_sha256"],
        "context and implementation binary digests differ",
    )


def validate_run_configurations(
    manifest: dict[str, Any], files: dict[str, bytes]
) -> list[dict[str, Any]]:
    expected = foundation.derive_runs(files, manifest["implementation_revision"])
    expected_rows = [
        {
            "configuration": row["configuration"],
            "declared_population_sha256": row["declared_population_sha256"],
            "execution_population_sha256": row["execution_population_sha256"],
            "generation_fingerprint": None,
            "logical_run_sha256": row["logical_run_sha256"],
            "run_id": row["run_id"],
        }
        for row in expected
    ]
    fingerprints = foundation.derive_generation_fingerprints(files, expected)
    by_run = {row["run_id"]: row["fingerprint"] for row in fingerprints["bindings"]}
    for row in expected_rows:
        row["generation_fingerprint"] = by_run.get(row["run_id"])
    require(manifest["run_configurations"] == expected_rows, "run-ID or logical-run mismatch")
    populations = [
        {
            "declared": row["declared_population_sha256"],
            "execution": row["execution_population_sha256"],
            "run_id": row["run_id"],
        }
        for row in expected
    ]
    require(manifest["population_hashes"] == populations, "population mismatch")
    require(manifest["generation_fingerprints"] == fingerprints["preimages"], "generation-fingerprint mismatch")
    require(len({row["logical_run_sha256"] for row in expected}) == 15, "logical runs are not unique")
    return expected


def validate_results(root: Path, expected_runs: list[dict[str, Any]]) -> dict[str, Any]:
    results = canonical_json_file(root / "rust-results.json")
    require_keys(results, RESULT_KEYS, "rust-results.json")
    require(results["schema_version"] == 3, "wrong Rust-results schema version")
    by_id = {row["run_id"]: row for row in results["runs"]}
    require(list(by_id) == sorted(by_id), "Rust result runs are reordered")
    require(set(by_id) == {row["run_id"] for row in expected_runs}, "Rust result run-ID mismatch")
    for expected in expected_runs:
        run = by_id[expected["run_id"]]
        require_keys(run, RESULT_RUN_KEYS, f"Rust run {run['run_id']}")
        require(run["status"] == "valid", f"invalid run status for {run['run_id']}")
        require(
            [row["query_id"] for row in run["queries"]] == expected["declared_population"],
            f"Rust query population mismatch for {run['run_id']}",
        )
        for query in run["queries"]:
            require_keys(query, RESULT_QUERY_KEYS, f"Rust query {run['run_id']}/{query['query_id']}")
            require_keys(query["candidate_limits"], {"keyword", "vector"}, "candidate limits")
            require(
                query["execution_status"] in {"valid", "excluded_pre_freeze"},
                "invalid execution in public Rust results",
            )
            require(query["execution_status"] != "valid" or query["status_reason"] is None, "valid query has reason")
            for hit in query["chunk_hits"]:
                require_keys(hit, CHUNK_HIT_KEYS, "chunk hit")
            for hit in query["projected_documents"]:
                require_keys(hit, DOCUMENT_HIT_KEYS, "projected document")
    return results


def validate_valid_run_statuses(
    metric_runs: list[dict[str, Any]], result_runs: list[dict[str, Any]]
) -> None:
    metric_status = {row["run_id"]: row["status"] for row in metric_runs}
    result_status = {row["run_id"]: row["status"] for row in result_runs}
    require(metric_status == result_status, "metrics/result status mismatch")
    require(all(status == "valid" for status in metric_status.values()), "invalid run status")


def validate_metrics(root: Path, expected_runs: list[dict[str, Any]], results: dict[str, Any]) -> dict[str, Any]:
    metrics = canonical_json_file(root / "metrics.json")
    require_keys(metrics, METRICS_KEYS, "metrics.json")
    require(metrics["schema_version"] == 3, "wrong metrics schema version")
    require(metrics["metric_definition_version"] == "graph-retrieval-v3-r2", "wrong metric version")
    require(metrics["publication_status"] == "valid", "publication status is not valid")
    by_result = {row["run_id"]: row for row in results["runs"]}
    by_metric = {row["run_id"]: row for row in metrics["runs"]}
    validate_valid_run_statuses(metrics["runs"], results["runs"])
    require(list(by_metric) == sorted(by_metric), "metric runs are reordered")
    require(set(by_metric) == {row["run_id"] for row in expected_runs}, "metric run-ID mismatch")
    for expected in expected_runs:
        run = by_metric[expected["run_id"]]
        require_keys(run, METRIC_RUN_KEYS, f"metric run {run['run_id']}")
        require(run["status"] == by_result[run["run_id"]]["status"] == "valid", "metrics/result status mismatch")
        require(run["declared_population_sha256"] == expected["declared_population_sha256"], "declared population mismatch")
        require(run["execution_population_sha256"] == expected["execution_population_sha256"], "execution population mismatch")
        require(set(run["macro"]) == METRIC_NAMES, "macro metric schema mismatch")
        require(set(run["micro"]) == MICRO_KEYS, "micro metric schema mismatch")
        require([row["query_id"] for row in run["queries"]] == expected["declared_population"], "metric query population mismatch")
        for query in run["queries"]:
            require_keys(query, {"candidate_counts", "execution_status", "metrics", "query_id"}, "metric query")
            require(set(query["metrics"]) == METRIC_NAMES, "per-query metric schema mismatch")
            require(query["execution_status"] != "invalid_execution", "invalid metric execution")
            for value in query["metrics"].values():
                require_keys(value, {"status", "value"}, "per-query metric value")
                require(value["status"] != "invalid_execution", "invalid metric status")
        for value in run["macro"].values():
            require_keys(value, {"denominator", "numerator", "status_counts", "value"}, "macro metric")
            require_keys(
                value["status_counts"],
                {"excluded_pre_freeze", "invalid_execution", "not_applicable", "undefined", "valid"},
                "macro status counts",
            )
            require(value["status_counts"]["invalid_execution"] == 0, "invalid macro execution count")
    require(len(metrics["paired_comparisons"]) == 9, "expected nine paired comparisons")
    require(all(row["status"] == "valid" for row in metrics["paired_comparisons"]), "invalid paired comparison")
    return metrics


def parse_run(path: Path, run_id: str) -> list[tuple[str, str, int, int]]:
    data = path.read_bytes()
    offset = 0
    rows = []
    previous: tuple[str, int] | None = None
    seen = set()
    for match in RUN_LINE.finditer(data):
        require(match.start() == offset, f"malformed TREC row in '{path}'")
        offset = match.end()
        query, document, rank, score, tag = match.groups()
        query_id = query.decode("ascii")
        record_id = document.decode("ascii")
        integer_rank = int(rank)
        require(tag.decode("ascii") == run_id, f"TREC run tag mismatch in '{path}'")
        require(previous is None or (query_id, integer_rank) > previous, f"reordered TREC run '{path}'")
        require((query_id, record_id) not in seen, f"duplicate TREC row in '{path}'")
        require(integer_rank == (1 if previous is None or previous[0] != query_id else previous[1] + 1), f"nonconsecutive rank in '{path}'")
        seen.add((query_id, record_id))
        rows.append((query_id, record_id, integer_rank, int(score)))
        previous = (query_id, integer_rank)
    require(offset == len(data), f"malformed TREC bytes in '{path}'")
    return rows


def validate_ranked_runs(root: Path, expected_runs: list[dict[str, Any]], results: dict[str, Any], depth: int) -> None:
    result_by_id = {row["run_id"]: row for row in results["runs"]}
    for expected in expected_runs:
        if expected["configuration"]["run_letter"] not in "abcefg":
            continue
        run_id = expected["run_id"]
        wanted = []
        for query in result_by_id[run_id]["queries"]:
            for document in query["projected_documents"]:
                rank = document["document_rank"]
                wanted.append((query["query_id"], document["record_id"], rank, depth - rank + 1))
        require(parse_run(root / "runs" / f"{run_id}.trec", run_id) == wanted, f"TREC/Rust ranking mismatch for {run_id}")


def validate_graph_files(root: Path, expected_runs: list[dict[str, Any]]) -> None:
    for run in expected_runs:
        if run["configuration"]["run_letter"] not in "defg":
            continue
        run_id = run["run_id"]
        selections = canonical_jsonl_file(root / "graph-selections" / f"{run_id}.jsonl")
        paths = canonical_jsonl_file(root / "graph-paths" / f"{run_id}.jsonl")
        for row in selections:
            require_keys(row, SELECTION_KEYS, "graph selection")
            require(row["run_id"] == run_id and row["stale"] is False, "invalid graph selection identity")
        for row in paths:
            require_keys(row, PATH_KEYS, "graph path")
            require(row["run_id"] == run_id and row["depth"] == len(row["edges"]), "invalid graph path identity")


def validate_gate_reports(
    trec_report_path: Path,
    ir_report_path: Path,
    ranked_run_ids: set[str],
    metrics: dict[str, Any],
) -> None:
    trec = canonical_json_file(trec_report_path)
    require_gate_status(trec, "trec_eval")
    require(trec.get("tolerance") == 1.0e-9, "wrong trec_eval tolerance")
    require(
        trec.get("maximum_absolute_differences", {}).get("per_query", math.inf) <= 1.0e-9
        and trec.get("maximum_absolute_differences", {}).get("aggregate", math.inf) <= 1.0e-9,
        "trec_eval difference exceeds tolerance",
    )
    dependency = trec.get("dependency", {})
    require(dependency.get("upstream_commit") == trec_bootstrap.UPSTREAM_COMMIT, "trec_eval commit mismatch")
    require(dependency.get("archive_sha256") == trec_bootstrap.ARCHIVE_SHA256, "trec_eval source checksum mismatch")
    require(dependency.get("source_tree_sha256") == trec_bootstrap.SOURCE_TREE_SHA256, "trec_eval tree checksum mismatch")
    require({row["run_id"] for row in trec.get("runs", [])} == ranked_run_ids, "trec_eval run identity mismatch")
    metric_by_id = {row["run_id"]: row for row in metrics["runs"]}
    for run in trec["runs"]:
        query_by_id = {row["query_id"]: row for row in metric_by_id[run["run_id"]]["queries"]}
        for check in run["query_checks"]:
            require(check["status"] == "passed", "failed trec_eval query check")
            value = query_by_id[check["query_id"]]["metrics"][check["metric"]]["value"]
            require(value == check["rust_value"], "mismatched trec_eval report metric identity")
        for check in run["aggregate_checks"]:
            require(check["status"] == "passed", "failed trec_eval aggregate check")
            value = metric_by_id[run["run_id"]]["macro"][check["metric"]]["value"]
            require(value == check["rust_value"], "mismatched trec_eval aggregate identity")
    ir = canonical_json_file(ir_report_path)
    require_gate_status(ir, "ir_measures")
    require(ir.get("dependency", {}).get("ir_measures") == "0.4.3", "ir_measures identity mismatch")
    require({row["run_id"] for row in ir.get("checked_runs", [])} == ranked_run_ids, "ir_measures run identity mismatch")


def require_gate_status(report: dict[str, Any], label: str) -> None:
    require(report.get("status") == "passed", f"failed or mismatched {label} report")


def validate(
    collection: Path,
    root: Path,
    executable: Path,
    trec_report: Path,
    ir_report: Path,
    repository: Path = ROOT,
) -> dict[str, Any]:
    verify_frozen_fixture(collection)
    manifest = canonical_json_file(root / "manifest.json")
    require_keys(manifest, MANIFEST_KEYS, "manifest.json")
    require(manifest["schema_version"] == 3, "wrong manifest schema version")
    require(manifest["profile"] == "deterministic_quality", "wrong publication profile")
    require(manifest["publication_status"] == "valid", "manifest publication status is not valid")
    files = collection_files(collection)
    expected_runs = validate_run_configurations(manifest, files)
    validate_inventory(root, expected_paths(expected_runs))
    validate_manifest_file_index(root, manifest)
    validate_revision(manifest, executable, repository)
    validate_environment(manifest)
    header = read_json(collection / "collection.json")
    require(manifest["collection_id"] == header["collection_id"], "collection ID mismatch")
    require(manifest["collection_version"] == header["collection_version"], "collection version mismatch")
    for name in ("qrels.tsv", "evidence-judgments.jsonl", "expected-paths.jsonl", "exclusions.jsonl"):
        require((root / name).read_bytes() == (collection / name).read_bytes(), f"copied input '{name}' differs")
    canonical_jsonl_file(root / "evidence-judgments.jsonl")
    canonical_jsonl_file(root / "expected-paths.jsonl")
    canonical_jsonl_file(root / "exclusions.jsonl")
    results = validate_results(root, expected_runs)
    metrics = validate_metrics(root, expected_runs, results)
    require(manifest["metric_definition_version"] == metrics["metric_definition_version"], "manifest metric version mismatch")
    validate_ranked_runs(root, expected_runs, results, header["evaluation_depth"])
    validate_graph_files(root, expected_runs)
    require(
        (root / "timing-samples.jsonl").read_bytes()
        == b'{"profile":"deterministic_quality","status":"not_measured"}\n',
        "timing samples are not the exact deterministic-quality row",
    )
    ranked = {row["run_id"] for row in expected_runs if row["configuration"]["run_letter"] in "abcefg"}
    validate_gate_reports(trec_report, ir_report, ranked, metrics)
    indexed = []
    for path in sorted(path for path in root.rglob("*") if path.is_file()):
        data = path.read_bytes()
        indexed.append({"bytes": len(data), "path": path.relative_to(root).as_posix(), "sha256": sha256(data)})
    return {
        "artifact_set_sha256": sha256(canonical_bytes(indexed)),
        "file_count": len(indexed),
        "logical_run_sha256": sorted(row["logical_run_sha256"] for row in expected_runs),
        "manifest_file_count": len(manifest["files"]),
        "run_ids": [row["run_id"] for row in expected_runs],
        "status": "passed",
    }


def remap_ids(value: Any, mapping: dict[str, str]) -> Any:
    if isinstance(value, list):
        return [remap_ids(item, mapping) for item in value]
    if isinstance(value, dict):
        return {
            key: (mapping.get(item, item) if key in {"run_id", "selection_run_id", "baseline_run_id", "scoped_run_id"} and item is not None else remap_ids(item, mapping))
            for key, item in value.items()
        }
    return value


def portability_view(root: Path) -> dict[str, Any]:
    manifest = read_json(root / "manifest.json")
    mapping = {row["run_id"]: row["logical_run_sha256"] for row in manifest["run_configurations"]}
    require(len(mapping) == len(set(mapping.values())), "logical-run mapping is not bijective")
    view: dict[str, Any] = {}
    logical_paths = []
    for path in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = path.relative_to(root).as_posix()
        parts = relative.split("/")
        if len(parts) == 2 and parts[0] in {"runs", "graph-selections", "graph-paths"}:
            stem, suffix = os.path.splitext(parts[1])
            require(stem in mapping, f"unmapped run filename '{relative}'")
            relative = f"{parts[0]}/{mapping[stem]}{suffix}"
        logical_paths.append(relative)
        if path.suffix == ".json":
            value = remap_ids(read_json(path), mapping)
            if path.name == "manifest.json":
                for key in (
                    "determinism_context",
                    "determinism_environment",
                    "implementation_revision",
                    "files",
                    "deterministic_files",
                ):
                    value.pop(key)
                for row in value["run_configurations"]:
                    row["configuration"].pop("implementation_revision")
                value["logical_paths"] = None
            view[relative] = value
        elif path.suffix == ".jsonl":
            view[relative] = [remap_ids(row, mapping) for row in canonical_jsonl_file(path)]
        elif path.suffix == ".trec":
            rows = []
            for line in path.read_text(encoding="utf-8").splitlines():
                fields = line.split(" ")
                require(fields[5] in mapping, "unmapped TREC run tag")
                fields[5] = mapping[fields[5]]
                rows.append(fields)
            view[relative] = rows
        else:
            view[relative] = path.read_bytes()
    view["manifest.json"]["logical_paths"] = sorted(logical_paths)
    return view


def compare_values(left: Any, right: Any, path: str = "") -> None:
    if isinstance(left, float) or isinstance(right, float):
        require(isinstance(left, (int, float)) and isinstance(right, (int, float)), f"portability type mismatch at {path}")
        native = path.rsplit("/", 1)[-1] in {
            "bm25_normalized_score",
            "bm25_score",
            "fusion_score",
            "score",
            "vector_normalized_score",
            "vector_score",
        }
        tolerance = 1.0e-6 if native else 1.0e-12
        require(abs(float(left) - float(right)) <= tolerance, f"portability numeric mismatch at {path}")
        return
    require(type(left) is type(right), f"portability type mismatch at {path}")
    if isinstance(left, dict):
        require(left.keys() == right.keys(), f"portability object mismatch at {path}")
        for key in left:
            compare_values(left[key], right[key], f"{path}/{key}")
    elif isinstance(left, list):
        require(len(left) == len(right), f"portability array mismatch at {path}")
        for index, (left_item, right_item) in enumerate(zip(left, right, strict=True)):
            compare_values(left_item, right_item, f"{path}/{index}")
    else:
        require(left == right, f"portability mismatch at {path}")


def compare_portability(left: Path, right: Path) -> dict[str, Any]:
    left_view = portability_view(left)
    right_view = portability_view(right)
    compare_values(left_view, right_view)
    return {"logical_file_count": len(left_view), "status": "passed"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--trec-eval-report", type=Path, required=True)
    parser.add_argument("--ir-measures-report", type=Path, required=True)
    parser.add_argument("--compare-portability", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = validate(
            args.collection.resolve(),
            args.artifacts.resolve(),
            args.executable.resolve(),
            args.trec_eval_report.resolve(),
            args.ir_measures_report.resolve(),
        )
        if args.compare_portability:
            report["portability"] = compare_portability(
                args.artifacts.resolve(), args.compare_portability.resolve()
            )
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
