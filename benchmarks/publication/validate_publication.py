#!/usr/bin/env python3
"""Independently validate the Phase 6 benchmark publication package."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from collections import defaultdict
from datetime import date
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path
from typing import Any


REPORT_DATE = "2026-07-21"
EXPIRES_ON = "2027-07-21"
SOURCE_REVISION = "9c784d2f11b91bb907150aa1b6046880ff89fde6"
PHASE3_ARTIFACT_SHA = "e5d5824365d40745156701ba36744c1b7f764ce8fffb13245112b2c9ecb771c6"
PHASE5_ARTIFACT_SHA = "1e7283359f1781dacca1ced3c2fa1794e19a02a2b9669a782465e8f42a8c5602"
SUPPORTED_SET_SHA = "f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a"
GRAPH_FREE_SET_SHA = "6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a"
EXPECTED_INVENTORY = {
    "methodology.md",
    "retrieval-quality.md",
    "mac-systems-performance.md",
    "physical-device-systems-performance.md",
    "claim-register.json",
    "licensing.json",
    "evidence-index.json",
    "reproduction.md",
    "checksums.json",
    "manifest.json",
}
EXPECTED_IDS = {
    "permitted": {
        "P6-QUALITY-001",
        "P6-QUALITY-002",
        "P6-QUALITY-003",
        "P6-MAC-EXACT-001",
        "P6-MAC-EXACT-002",
        "P6-MAC-CORRECTNESS-001",
        "P6-ANN-NEGATIVE-001",
        "P6-DEVICE-001",
        "P6-DEVICE-SAFETY-001",
    },
    "prohibited": {
        "P6-PROHIBITED-001",
        "P6-PROHIBITED-002",
        "P6-PROHIBITED-003",
        "P6-PROHIBITED-004",
        "P6-PROHIBITED-005",
        "P6-PROHIBITED-006",
    },
    "withheld": {
        "P6-WITHHELD-001",
        "P6-WITHHELD-002",
        "P6-WITHHELD-003",
        "P6-WITHHELD-004",
    },
}
# Frozen after the generator and validator are reviewed. These hashes bind exact
# human-readable wording, claim membership/rounding, and licensing decisions.
EXPECTED_CONTENT_HASHES: dict[str, str] = {
    "methodology.md": "5758012387497b754773dda94c16467c2002c3db5bc0b7ecf11eac05961266c4",
    "retrieval-quality.md": "ce02dd9ce9d2839f251bddb629ef3b7377c4bc72aa597b94ae7a0ab0e07cde4a",
    "mac-systems-performance.md": "e0d7a4a49730936ead3c2bbe7fc4bc29dd81eddcff1f1f89600583320de09e6a",
    "physical-device-systems-performance.md": "2a56f867d7fc59f01deca8f1e7cef485b5e932a45d4192dad4c7e03dda36fa0e",
    "claim-register.json": "f0fc222ace0caacff5fc5f6c857bdbc6f76b86010cf193df8930c3ffcab97aaa",
    "licensing.json": "347ae5760abb620842b816725fb2c1a61c5d0d32ddb7da96b6e0f2a678572da6",
    "reproduction.md": "ab867c4ffb7732c39289ea5baeb41d51daf3b4d1e1d585446bade72f7f9d8fc4",
}


class ValidationError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open(encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def nearest_rank(values: list[int], percentile: Decimal) -> int:
    require(bool(values), "cannot calculate a percentile over no samples")
    ordered = sorted(values)
    return ordered[math.ceil(float(percentile) * len(ordered)) - 1]


def median(values: list[int]) -> int:
    require(len(values) % 2 == 1, "median population must be odd")
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def rounded(value: Decimal, places: int) -> str:
    quantum = Decimal(1).scaleb(-places)
    return format(value.quantize(quantum, rounding=ROUND_HALF_UP), f".{places}f")


def milliseconds(value: int) -> str:
    return rounded(Decimal(value) / Decimal(1_000_000), 3)


def ratio(numerator: int, denominator: int) -> str:
    return rounded(Decimal(numerator) / Decimal(denominator), 2)


def percent(value: float) -> str:
    return rounded(Decimal(str(value)) * Decimal(100), 2)


def validate_inventory(root: Path) -> None:
    require(root.is_dir(), f"publication root is missing: {root}")
    paths: set[str] = set()
    for path in root.rglob("*"):
        require(not path.is_symlink(), f"symlink forbidden: {path}")
        require(path.is_file(), f"nested directories forbidden: {path}")
        paths.add(path.relative_to(root).as_posix())
    require(paths == EXPECTED_INVENTORY, f"inventory mismatch: {sorted(paths ^ EXPECTED_INVENTORY)}")


def validate_hashes(root: Path, repo: Path) -> dict[str, Any]:
    checksums = load_json(root / "checksums.json")
    expected_payloads = EXPECTED_INVENTORY - {"checksums.json", "manifest.json"}
    require(set(checksums["files"]) == expected_payloads, "checksums inventory mismatch")
    for name, expected in checksums["files"].items():
        require(sha256_file(root / name) == expected, f"incorrect checksum: {name}")
    manifest: dict[str, Any] = load_json(root / "manifest.json")
    require(set(manifest["inventory"]) == EXPECTED_INVENTORY, "manifest inventory mismatch")
    expected_preimage_files = EXPECTED_INVENTORY - {"manifest.json"}
    require(set(manifest["files"]) == expected_preimage_files, "manifest file map mismatch")
    for name, expected in manifest["files"].items():
        require(sha256_file(root / name) == expected, f"manifest hash mismatch: {name}")
    preimage = "".join(f"{name}\t{manifest['files'][name]}\n" for name in sorted(manifest["files"])).encode()
    require(sha256_bytes(preimage) == manifest["canonical_artifact_set_sha256"], "canonical artifact hash mismatch")
    require(manifest["input_artifacts"] == {
        "phase3_locked_reporting": PHASE3_ARTIFACT_SHA,
        "phase4b_supported": SUPPORTED_SET_SHA,
        "phase4b_graph_free": GRAPH_FREE_SET_SHA,
        "phase5_mac_comparison": PHASE5_ARTIFACT_SHA,
    }, "input artifact identities changed")
    contract = repo / manifest["contract"]["path"]
    validator = repo / manifest["validator"]["path"]
    require(sha256_file(contract) == manifest["contract"]["sha256"], "contract identity mismatch")
    require(sha256_file(validator) == manifest["validator"]["sha256"], "validator identity mismatch")
    require(manifest["validator"]["result"] == "PASS", "manifest validator result is not PASS")
    for name, expected in EXPECTED_CONTENT_HASHES.items():
        require(sha256_file(root / name) == expected, f"frozen publication content changed: {name}")
    return manifest


def close_float(actual: float, expected: float, label: str) -> None:
    require(math.isclose(actual, expected, rel_tol=0.0, abs_tol=5e-12), f"{label} changed: {actual} != {expected}")


def validate_quality(index: dict[str, Any], phase3: Path) -> None:
    data = index["quality"]
    require(data["artifact_set_sha256"] == PHASE3_ARTIFACT_SHA, "Phase 3 artifact identity changed")
    metrics = load_json(phase3 / "graph-retrieval-metrics.json")
    scoped = next(row for row in metrics["runs"] if "v3-g" in row["run_id"])
    scoped_queries = {row["query_id"]: row for row in scoped["queries"] if row["execution_status"] == "valid"}
    qrels_path = phase3.parents[1] / "public-collections/hotpotqa-linked-abstracts-graph-v1/test/qrels.tsv"
    qrels: dict[str, set[str]] = defaultdict(set)
    for line in qrels_path.read_text(encoding="utf-8").splitlines():
        query_id, _, document_id, relevance = line.split()
        if int(relevance) > 0:
            qrels[query_id].add(document_id)

    def read_run(path: Path) -> dict[str, list[str]]:
        result: dict[str, list[str]] = defaultdict(list)
        for line in path.read_text(encoding="utf-8").splitlines():
            query_id, _, document_id, rank, _, _ = line.split()
            require(int(rank) == len(result[query_id]) + 1, f"non-contiguous run rank: {path}")
            result[query_id].append(document_id)
        return result

    baseline_run = read_run(next((phase3 / "runs").glob("v3-c-*.trec")))
    scoped_run = read_run(next((phase3 / "runs").glob("v3-g-*.trec")))
    common = sorted(set(baseline_run) & set(scoped_run) & set(scoped_queries))
    require(len(common) == 296, "Phase 3 common population is not 296")

    def raw_metric(run: dict[str, list[str]], query_id: str, metric: str) -> float:
        relevant = qrels[query_id]
        top_ten = run[query_id][:10]
        hits = [1 if document_id in relevant else 0 for document_id in top_ten]
        if metric == "recall_at_10":
            return sum(hits) / len(relevant)
        if metric == "complete_evidence_recall_at_10":
            return float(sum(hits) == len(relevant))
        gains = math.fsum(hit / math.log2(rank + 1) for rank, hit in enumerate(hits, start=1))
        ideal = math.fsum(1 / math.log2(rank + 1) for rank in range(1, min(len(relevant), 10) + 1))
        return gains / ideal

    for metric in ("ndcg_at_10", "recall_at_10", "complete_evidence_recall_at_10"):
        baseline_values = [raw_metric(baseline_run, query_id, metric) for query_id in common]
        scoped_values = [raw_metric(scoped_run, query_id, metric) for query_id in common]
        baseline_mean = math.fsum(baseline_values) / len(common)
        scoped_mean = math.fsum(scoped_values) / len(common)
        wins = sum(candidate > control for control, candidate in zip(baseline_values, scoped_values, strict=True))
        ties = sum(candidate == control for control, candidate in zip(baseline_values, scoped_values, strict=True))
        losses = len(common) - wins - ties
        published = data["comparison"][metric]
        close_float(published["baseline"], baseline_mean, f"{metric} baseline")
        close_float(published["scoped"], scoped_mean, f"{metric} scoped")
        close_float(published["delta"], scoped_mean - baseline_mean, f"{metric} delta")
        close_float(published["relative_delta"], (scoped_mean - baseline_mean) / baseline_mean, f"{metric} relative")
        require((published["wins"], published["ties"], published["losses"]) == (wins, ties, losses), f"{metric} W/T/L changed")
    candidate_rows = [scoped_queries[qid] for qid in common]
    eligible = sum(row["candidate_counts"]["eligible_chunks"] for row in candidate_rows)
    projected = sum(row["candidate_counts"]["projected_chunks"] for row in candidate_rows)
    reduction = math.fsum(row["candidate_counts"]["eligible_chunks"] / row["candidate_counts"]["projected_chunks"] for row in candidate_rows) / len(candidate_rows)
    candidate_recall = math.fsum(row["metrics"]["candidate_recall"]["value"] for row in candidate_rows) / len(candidate_rows)
    complete = math.fsum(row["metrics"]["candidate_complete_evidence"]["value"] for row in candidate_rows) / len(candidate_rows)
    empty = sum(row["metrics"]["empty_scope"]["value"] for row in candidate_rows)
    published_scope = data["candidate_scope"]
    close_float(published_scope["macro_mean_per_query_reduction"], reduction, "candidate reduction")
    require(published_scope["micro_eligible_chunks"] == eligible, "eligible candidate total changed")
    require(published_scope["micro_projected_chunks"] == projected, "projected candidate total changed")
    close_float(published_scope["micro_ratio"], eligible / projected, "pooled candidate ratio")
    close_float(published_scope["candidate_recall"], candidate_recall, "candidate recall")
    close_float(published_scope["candidate_complete_evidence"], complete, "candidate complete evidence")
    require(published_scope["empty_scopes"] == empty == 0, "empty-scope count changed")


def validate_mac(index: dict[str, Any], phase5: Path) -> None:
    data = index["mac"]
    require(data["artifact_set_sha256"] == PHASE5_ARTIFACT_SHA, "Phase 5 artifact identity changed")
    environment = load_json(phase5 / "environment.json")
    require(environment["cpu"] == "Apple M1 Max", "Mac hardware changed")
    require(environment["os_release"] == "25.5.0", "Mac OS changed")
    require(environment["repository_revision"] == SOURCE_REVISION, "Mac source revision changed")
    measurements = read_jsonl(phase5 / "raw-measurements.jsonl")
    grouped: dict[tuple[str, str, str], list[int]] = defaultdict(list)
    for row in measurements:
        if row["stage"] == "retrieval":
            grouped[(row["workload_id"], row["system_id"], row["operation_id"])].append(row["duration_ns"])
    published_rows = {(row["workload_id"], row["filtering"]): row for row in data["exact_rows"]}
    for size in ("10k", "25k", "50k"):
        workload = f"{size}-384d-v1"
        for operation in ("exact_unfiltered", "exact_filtered"):
            filtering = operation == "exact_filtered"
            row = published_rows[(workload, filtering)]
            vk = grouped[(workload, "vectorkit_f32_exact", operation)]
            sqlite = grouped[(workload, "sqlite_vec_exact", operation)]
            require(len(vk) == len(sqlite) == 100, f"Mac sample count changed: {workload}/{operation}")
            expected = {
                "vectorkit_p50_ns": nearest_rank(vk, Decimal("0.50")),
                "vectorkit_p95_ns": nearest_rank(vk, Decimal("0.95")),
                "sqlite_vec_p50_ns": nearest_rank(sqlite, Decimal("0.50")),
                "sqlite_vec_p95_ns": nearest_rank(sqlite, Decimal("0.95")),
            }
            for key, value in expected.items():
                require(row[key] == value, f"Mac raw percentile changed: {workload}/{operation}/{key}")
            require(row["p50_ratio_sqlite_over_vectorkit"] == ratio(expected["sqlite_vec_p50_ns"], expected["vectorkit_p50_ns"]), "Mac ratio or rounding changed")
            require(row["display"] == {
                "vectorkit_p50_ms": milliseconds(expected["vectorkit_p50_ns"]),
                "vectorkit_p95_ms": milliseconds(expected["vectorkit_p95_ns"]),
                "sqlite_vec_p50_ms": milliseconds(expected["sqlite_vec_p50_ns"]),
                "sqlite_vec_p95_ms": milliseconds(expected["sqlite_vec_p95_ns"]),
            }, "Mac display rounding changed")
    results = read_jsonl(phase5 / "raw-results.jsonl")
    oracle: dict[tuple[str, str], set[str]] = {}
    ann: dict[tuple[str, str], set[str]] = {}
    for row in results:
        result_key = (row["workload_id"], row["query_id"])
        if row["operation_id"] == "exact_unfiltered" and row["system_id"] == "numpy_f32_oracle":
            oracle[result_key] = set(row["result_ids"][:10])
        if row["operation_id"] == "ann_unfiltered" and row["system_id"] == "usearch_hnsw":
            ann[result_key] = set(row["result_ids"][:10])
    recall_rows = {row["workload_id"]: row for row in data["usearch_recall"]}
    for workload, row in recall_rows.items():
        keys = sorted(key for key in oracle if key[0] == workload)
        recalls = [len(oracle[key] & ann[key]) / 10 for key in keys]
        actual = math.fsum(recalls) / len(recalls)
        close_float(row["mean_recall_at_10"], actual, f"{workload} USearch recall")
        require(row["display_percent"] == percent(actual), "USearch recall rounding changed")
        require(row["gate"] == "failed" and not row["timing_eligible"], "USearch disqualification missing")


def validate_device(index: dict[str, Any], phase4: Path) -> None:
    data = index["device"]
    require(data["accepted_sets"] == {
        "supported_count": 846,
        "supported_sha256": SUPPORTED_SET_SHA,
        "graph_free_count": 12,
        "graph_free_sha256": GRAPH_FREE_SET_SHA,
    }, "Phase 4b accepted sets changed")
    supported = phase4 / "devices/iphone17-pro-max/supported"
    published = {(row["workload_id"], row["encoding"], row["query_category"]): row for row in data["query_rows"]}
    for workload in ("10k-384d-v3", "25k-384d-v3", "50k-384d-v3"):
        for encoding in ("f32", "i8"):
            session_artifacts = [load_json(supported / workload / encoding / "query" / f"session-{number:02d}.json") for number in range(5)]
            require(all(row["environment"]["device_identifier"] == "iPhone18,2" for row in session_artifacts), "device identity changed")
            for category in [row["query_category"] for row in session_artifacts[0]["report"]["scenarios"]]:
                per_percentile: dict[str, list[int]] = {"p50_ns": [], "p95_ns": [], "p99_ns": []}
                for artifact in session_artifacts:
                    scenario = next(row for row in artifact["report"]["scenarios"] if row["query_category"] == category)
                    distribution = next(row for row in scenario["distributions"] if row["stage"] == "end_to_end_total")
                    raw = [next(stage["duration_ns"] for stage in sample["stages"] if stage["stage"] == "end_to_end_total") for sample in scenario["samples"]]
                    require(len(raw) == 1000, "device query sample count changed")
                    for key, percentile_value in (("p50_ns", "0.50"), ("p95_ns", "0.95"), ("p99_ns", "0.99")):
                        recomputed = nearest_rank(raw, Decimal(percentile_value))
                        require(distribution[key] == recomputed, f"device session percentile mismatch: {workload}/{encoding}/{category}/{key}")
                        per_percentile[key].append(recomputed)
                row = published[(workload, encoding, category)]
                expected = {key: median(values) for key, values in per_percentile.items()}
                require(row["median_session_p50_ns"] == expected["p50_ns"], "device P50 changed")
                require(row["median_session_p95_ns"] == expected["p95_ns"], "device P95 changed")
                require(row["median_session_p99_ns"] == expected["p99_ns"], "device P99 changed")
                require(row["display"] == {"p50_ms": milliseconds(expected["p50_ns"]), "p95_ms": milliseconds(expected["p95_ns"]), "p99_ms": milliseconds(expected["p99_ns"])}, "device rounding changed")
    graph_free = phase4 / "devices/iphone17-pro-max/graph-free"
    names = {"exact_vector": "semantic_exact_vector", "bm25": "bm25_internal", "hybrid": "hybrid_weighted_normalized_0.6_0.4"}
    published_graph_free = {(row["encoding"], row["query_category"]): row for row in data["graph_free_rows"]}
    for encoding in ("f32", "i8"):
        for category, scenario_name in names.items():
            medians: dict[str, int] = {}
            for product in ("baseline", "candidate"):
                p95s = []
                for number in range(3):
                    artifact = load_json(graph_free / encoding / product / f"session-{number:02d}.json")
                    scenario = next(row for row in artifact["report"]["scenarios"] if row["scenario"] == scenario_name)
                    raw = scenario["raw_duration_ns"]
                    recomputed = nearest_rank(raw, Decimal("0.95"))
                    require(scenario["p95_ns"] == recomputed, "graph-free raw percentile mismatch")
                    p95s.append(recomputed)
                medians[product] = median(p95s)
            row = published_graph_free[(encoding, category)]
            expected_ratio = ratio(medians["candidate"], medians["baseline"])
            require(row["candidate_over_baseline_ratio"] == expected_ratio, "graph-free ratio or rounding changed")
            require(Decimal(medians["candidate"]) / Decimal(medians["baseline"]) <= Decimal("1.03"), "graph-free gate failed")
            require(row["gate"] == "passed", "graph-free gate publication changed")
    stress = data["stress_100k"]
    require(stress["outcome"] == "not_run_device_safety", "100K outcome changed")
    require(stress["accepted_artifact_count"] == 0 and not stress["claim_eligible"], "100K became claim eligible")


def validate_evidence_references(index: dict[str, Any], claims: dict[str, Any], repo: Path) -> None:
    known: set[tuple[str, str]] = set()
    for family in ("quality", "mac", "device"):
        for row in index[family]["evidence"]:
            known.add((row["path"], row["sha256"]))
            path = repo / row["path"]
            if path.is_file():
                require(sha256_file(path) == row["sha256"], f"evidence hash mismatch: {row['path']}")
            else:
                require(row["sha256"] in {SUPPORTED_SET_SHA, GRAPH_FREE_SET_SHA}, f"unverifiable evidence path: {row['path']}")
    for claim_row in claims["claims"]:
        for row in claim_row["evidence"]:
            require((row["path"], row["sha256"]) in known, f"unknown claim evidence: {claim_row['claim_id']}")
            if claim_row["status"] == "permitted":
                require(row["evidence_state"] in {"accepted", "qualified_negative_result"}, "permitted claim references ineligible evidence")
                lowered = row["path"].lower()
                require("rejected" not in lowered and "disqualified" not in lowered, "permitted claim references rejected evidence")


def validate_claims(claims: dict[str, Any], as_of: date) -> None:
    require(claims["report_date"] == REPORT_DATE and claims["expires_on"] == EXPIRES_ON, "claim register dates changed")
    require(as_of <= date.fromisoformat(claims["expires_on"]), "claims are expired")
    ids: set[str] = set()
    observed: dict[str, set[str]] = defaultdict(set)
    required_fields = {
        "claim_id", "claim_text", "status", "evidence", "workload", "comparison_system",
        "hardware", "os", "metric", "sample_population", "calculation", "required_qualifiers",
        "prohibited_interpretations", "source_revision", "report_date", "expires_on",
        "mandatory_rerun_conditions", "licensing_eligibility",
    }
    for row in claims["claims"]:
        require(required_fields <= set(row), f"claim fields missing: {row.get('claim_id')}")
        require(row["claim_id"] not in ids, f"duplicate claim ID: {row['claim_id']}")
        ids.add(row["claim_id"])
        observed[row["status"]].add(row["claim_id"])
        require(row["source_revision"] == SOURCE_REVISION, f"source qualifier missing: {row['claim_id']}")
        require(row["report_date"] == REPORT_DATE and row["expires_on"] == EXPIRES_ON, f"claim dates changed: {row['claim_id']}")
        require(row["hardware"] and row["os"] and row["comparison_system"], f"hardware/version qualifier missing: {row['claim_id']}")
    require(dict(observed) == EXPECTED_IDS, "claim membership or status changed")
    require(claims["counts"] == {status: len(ids) for status, ids in EXPECTED_IDS.items()}, "claim counts changed")
    by_id = {row["claim_id"]: row for row in claims["claims"]}
    for claim_id in ("P6-MAC-EXACT-001", "P6-MAC-EXACT-002"):
        row = by_id[claim_id]
        require("Apple M1 Max" in row["hardware"] and "macOS 26.5.2" in row["os"], f"Mac qualifier missing: {claim_id}")
        require("sqlite-vec 0.1.9" in row["comparison_system"] and "P50" in row["metric"], f"Mac metric/version missing: {claim_id}")
    require("USearch 2.26.0" in by_id["P6-ANN-NEGATIVE-001"]["comparison_system"], "USearch version missing")
    require("timing disqualified" in by_id["P6-ANN-NEGATIVE-001"]["required_qualifiers"], "USearch timing disqualification missing")
    ann_text = by_id["P6-ANN-NEGATIVE-001"]["claim_text"].lower()
    require("faster" not in ann_text and "latency advantage" not in ann_text, "USearch performance claim")
    require("not_run_device_safety" in by_id["P6-DEVICE-SAFETY-001"]["claim_text"], "100K safety wording missing")
    for row in claims["claims"]:
        if row["status"] != "permitted":
            continue
        text = row["claim_text"].lower()
        require("universally faster" not in text and "universally better" not in text, "unsupported universal winner claim")
        if row["claim_id"] != "P6-ANN-NEGATIVE-001":
            require(not ("usearch" in text and ("faster" in text or "latency" in text)), "USearch performance claim")
        require(not ("beats the graph" in text or "graph performance winner" in text), "graph winner claim")
        if "100k" in text:
            require("not_run_device_safety" in text and "ineligible" in text, "100K support or marketing claim")


def validate_licensing(root: Path) -> None:
    licensing = load_json(root / "licensing.json")
    require(licensing["publication_decision"] == "repository_local_only", "publication eligibility broadened")
    require(licensing["repository_license"]["status"] == "absent", "repository license status changed")
    by_name = {row["name"]: row for row in licensing["inputs"]}
    require(by_name["HotpotQA dataset"]["license"] == "CC-BY-SA-4.0", "HotpotQA license changed")
    require("raw and transformed payloads excluded" in by_name["HotpotQA dataset"]["decision"], "HotpotQA raw exclusion missing")
    require(by_name["sqlite-vec"]["version"] == "0.1.9", "sqlite-vec version changed")
    require(by_name["USearch"]["version"] == "2.26.0", "USearch version changed")
    require("disqualified timing comparison excluded" in by_name["USearch"]["decision"], "USearch licensing/publication exclusion missing")
    for path in root.iterdir():
        require(path.suffix in {".md", ".json"}, f"unlicensed public artifact type: {path.name}")


def validate_package(args: argparse.Namespace) -> dict[str, Any]:
    repo = args.repo.resolve()
    root = args.root.resolve()
    phase3 = (repo / args.phase3_root).resolve() if not args.phase3_root.is_absolute() else args.phase3_root
    phase4 = (repo / args.phase4_root).resolve() if not args.phase4_root.is_absolute() else args.phase4_root
    phase5 = (repo / args.phase5_root).resolve() if not args.phase5_root.is_absolute() else args.phase5_root
    validate_inventory(root)
    manifest = validate_hashes(root, repo)
    index = load_json(root / "evidence-index.json")
    claims = load_json(root / "claim-register.json")
    validate_claims(claims, args.as_of_date)
    validate_licensing(root)
    validate_evidence_references(index, claims, repo)
    validate_quality(index, phase3)
    validate_mac(index, phase5)
    validate_device(index, phase4)
    return {
        "schema_version": 1,
        "artifact_id": manifest["artifact_id"],
        "canonical_artifact_set_sha256": manifest["canonical_artifact_set_sha256"],
        "validator_sha256": sha256_file(repo / "benchmarks/publication/validate_publication.py"),
        "result": "PASS",
        "validated_on": args.as_of_date.isoformat(),
        "gates": [
            "inventory", "hashes", "quality_recomputation", "mac_recomputation",
            "device_recomputation", "claims", "evidence", "licensing", "currency",
        ],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--phase3-root", type=Path, required=True)
    parser.add_argument("--phase4-root", type=Path, required=True)
    parser.add_argument("--phase5-root", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--as-of-date", type=date.fromisoformat, default=date.today())
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = validate_package(args)
    except (OSError, KeyError, TypeError, ValueError, ValidationError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    encoded = (json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if args.output:
        args.output.write_bytes(encoded)
    else:
        sys.stdout.buffer.write(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
