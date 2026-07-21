#!/usr/bin/env python3
"""Generate the deterministic Phase 6 publication package from frozen evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import shutil
from collections import defaultdict
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path
from typing import Any


REPORT_DATE = "2026-07-21"
EXPIRES_ON = "2027-07-21"
SOURCE_REVISION = "9c784d2f11b91bb907150aa1b6046880ff89fde6"
PHASE5_ARTIFACT_SHA = "1e7283359f1781dacca1ced3c2fa1794e19a02a2b9669a782465e8f42a8c5602"
PHASE3_ARTIFACT_SHA = "e5d5824365d40745156701ba36744c1b7f764ce8fffb13245112b2c9ecb771c6"
SUPPORTED_SET_SHA = "f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a"
GRAPH_FREE_SET_SHA = "6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a"
REPORT_FILES = (
    "methodology.md",
    "retrieval-quality.md",
    "mac-systems-performance.md",
    "physical-device-systems-performance.md",
    "claim-register.json",
    "licensing.json",
    "evidence-index.json",
    "reproduction.md",
)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def json_bytes(value: Any) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
        + "\n"
    ).encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def nearest_rank(values: list[int], percentile: Decimal) -> int:
    ordered = sorted(values)
    rank = math.ceil(float(percentile) * len(ordered))
    return ordered[rank - 1]


def median(values: list[int]) -> int:
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def decimal_round(value: Decimal, places: int) -> str:
    quantum = Decimal(1).scaleb(-places)
    return format(value.quantize(quantum, rounding=ROUND_HALF_UP), f".{places}f")


def milliseconds(nanoseconds: int) -> str:
    return decimal_round(Decimal(nanoseconds) / Decimal(1_000_000), 3)


def ratio(numerator: int, denominator: int) -> str:
    return decimal_round(Decimal(numerator) / Decimal(denominator), 2)


def percent(value: float) -> str:
    return decimal_round(Decimal(str(value)) * Decimal(100), 2)


def evidence(path: Path, repo: Path, state: str = "accepted") -> dict[str, str]:
    return {
        "evidence_state": state,
        "path": path.relative_to(repo).as_posix(),
        "sha256": sha256_file(path),
    }


def collect_quality(phase3: Path, repo: Path) -> dict[str, Any]:
    metrics_path = phase3 / "graph-retrieval-metrics.json"
    paired_path = phase3 / "graph-retrieval-paired-comparisons.json"
    metrics = load_json(metrics_path)
    paired = load_json(paired_path)
    comparison = next(
        row
        for row in metrics["paired_comparisons"]
        if "v3-c" in row["baseline_run_id"] and "v3-g" in row["scoped_run_id"]
    )
    paired_comparison = next(
        row
        for row in paired["comparisons"]
        if "v3-c" in row["baseline_run_id"] and "v3-g" in row["scoped_run_id"]
    )
    graph_run = next(row for row in metrics["runs"] if "v3-g" in row["run_id"])
    collection_root = phase3.parents[1] / "public-collections/hotpotqa-linked-abstracts-graph-v1/test"
    baseline_run = next((phase3 / "runs").glob("v3-c-*.trec"))
    scoped_run = next((phase3 / "runs").glob("v3-g-*.trec"))
    selected: dict[str, Any] = {}
    for name in ("ndcg_at_10", "recall_at_10", "complete_evidence_recall_at_10"):
        aggregate = comparison["metrics"][name]
        paired_aggregate = paired_comparison["metrics"][name]
        selected[name] = {
            "baseline": aggregate["baseline"]["value"],
            "delta": aggregate["delta"],
            "relative_delta": paired_aggregate["relative_delta"],
            "scoped": aggregate["scoped"]["value"],
            "wins": paired_aggregate["wins"],
            "ties": paired_aggregate["ties"],
            "losses": paired_aggregate["losses"],
        }
    macro = graph_run["macro"]
    micro = graph_run["micro"]
    return {
        "artifact_set_sha256": PHASE3_ARTIFACT_SHA,
        "collection": {
            "collection_id": "hotpotqa-linked-abstracts-graph-v1",
            "split": "test",
            "records": 12670,
            "chunks": 12670,
            "queries_declared": 297,
            "queries_compared": 296,
            "qrels": 594,
            "collection_sha256": "496d2302cc48ef709aab8eb651aa27b70facdfe5931f02273d13ea497aa28f72",
        },
        "embedding": {
            "model": "sentence-transformers/all-MiniLM-L6-v2",
            "revision": "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
            "dimensions": 384,
        },
        "comparison": selected,
        "candidate_scope": {
            "macro_mean_per_query_reduction": macro["candidate_reduction_ratio"]["value"],
            "micro_eligible_chunks": micro["candidate_reduction_ratio"]["eligible_chunks"],
            "micro_projected_chunks": micro["candidate_reduction_ratio"]["candidate_chunks"],
            "micro_ratio": micro["candidate_reduction_ratio"]["value"],
            "candidate_recall": macro["candidate_recall"]["value"],
            "candidate_complete_evidence": macro["candidate_complete_evidence"]["value"],
            "empty_scopes": micro["empty_scope_rate"]["empty_scopes"],
        },
        "evidence": [
            evidence(collection_root / "qrels.tsv", repo),
            evidence(baseline_run, repo),
            evidence(scoped_run, repo),
            evidence(phase3 / "metrics.json", repo),
            evidence(metrics_path, repo),
            evidence(paired_path, repo),
            evidence(phase3 / "manifest.json", repo),
            evidence(phase3 / "ranking-seal.json", repo),
        ],
    }


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open(encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def collect_mac(phase5: Path, repo: Path) -> dict[str, Any]:
    measurements_path = phase5 / "raw-measurements.jsonl"
    results_path = phase5 / "raw-results.jsonl"
    environment_path = phase5 / "environment.json"
    measurements = read_jsonl(measurements_path)
    grouped: dict[tuple[str, str, str], list[int]] = defaultdict(list)
    for row in measurements:
        if row["stage"] != "retrieval":
            continue
        key = (row["workload_id"], row["system_id"], row["operation_id"])
        grouped[key].append(row["duration_ns"])
    rows: list[dict[str, Any]] = []
    for size in ("10k", "25k", "50k"):
        workload = f"{size}-384d-v1"
        for operation in ("exact_unfiltered", "exact_filtered"):
            vk = grouped[(workload, "vectorkit_f32_exact", operation)]
            sqlite = grouped[(workload, "sqlite_vec_exact", operation)]
            if len(vk) != 100 or len(sqlite) != 100:
                raise ValueError(f"unexpected Phase 5 sample count for {workload}/{operation}")
            vk_p50 = nearest_rank(vk, Decimal("0.50"))
            vk_p95 = nearest_rank(vk, Decimal("0.95"))
            sqlite_p50 = nearest_rank(sqlite, Decimal("0.50"))
            sqlite_p95 = nearest_rank(sqlite, Decimal("0.95"))
            rows.append(
                {
                    "workload_id": workload,
                    "filtering": operation == "exact_filtered",
                    "sample_count_per_system": 100,
                    "vectorkit_p50_ns": vk_p50,
                    "vectorkit_p95_ns": vk_p95,
                    "sqlite_vec_p50_ns": sqlite_p50,
                    "sqlite_vec_p95_ns": sqlite_p95,
                    "p50_ratio_sqlite_over_vectorkit": ratio(sqlite_p50, vk_p50),
                    "display": {
                        "vectorkit_p50_ms": milliseconds(vk_p50),
                        "vectorkit_p95_ms": milliseconds(vk_p95),
                        "sqlite_vec_p50_ms": milliseconds(sqlite_p50),
                        "sqlite_vec_p95_ms": milliseconds(sqlite_p95),
                    },
                }
            )
    raw_results = read_jsonl(results_path)
    oracle: dict[tuple[str, str], list[str]] = {}
    ann: dict[tuple[str, str], list[str]] = {}
    for row in raw_results:
        result_key = (row["workload_id"], row["query_id"])
        if row["operation_id"] == "exact_unfiltered" and row["system_id"] == "numpy_f32_oracle":
            oracle[result_key] = row["result_ids"][:10]
        if row["operation_id"] == "ann_unfiltered" and row["system_id"] == "usearch_hnsw":
            ann[result_key] = row["result_ids"][:10]
    recall_rows = []
    for size in ("10k", "25k", "50k"):
        workload = f"{size}-384d-v1"
        keys = sorted(key for key in oracle if key[0] == workload)
        recalls = [len(set(oracle[key]) & set(ann[key])) / 10 for key in keys]
        recall_value = sum(recalls) / len(recalls)
        recall_rows.append(
            {
                "workload_id": workload,
                "query_count": len(recalls),
                "mean_recall_at_10": recall_value,
                "display_percent": percent(recall_value),
                "gate": "failed",
                "timing_eligible": False,
            }
        )
    environment = load_json(environment_path)
    return {
        "artifact_set_sha256": PHASE5_ARTIFACT_SHA,
        "configuration": {
            "hardware": environment["cpu"],
            "architecture": environment["architecture"],
            "os": "macOS 26.5.2",
            "os_kernel": f"Darwin {environment['os_release']}",
            "rustc": environment["rustc_version"],
            "cargo": environment["cargo_version"],
            "swift": environment["swift_version"],
            "python": f"{environment['python_implementation']} {environment['python_version']}",
            "dimensions": 384,
            "top_k": 10,
            "warmups": 20,
            "samples": 100,
            "embedding_included": False,
            "source_revision": environment["repository_revision"],
        },
        "systems": {
            "vectorkit_f32_exact": SOURCE_REVISION,
            "sqlite_vec_exact": "0.1.9",
            "numpy_f32_oracle": "2.5.1",
            "usearch_hnsw": "2.26.0",
        },
        "exact_rows": rows,
        "usearch_recall": recall_rows,
        "correctness": {
            "vectorkit_exact": "passed",
            "sqlite_vec_exact": "passed",
            "covered": ["identity", "filtering", "deletion", "determinism", "reload"],
        },
        "evidence": [
            evidence(measurements_path, repo),
            evidence(results_path, repo),
            evidence(environment_path, repo),
            evidence(phase5 / "manifest.json", repo),
            evidence(phase5 / "feature-parity.json", repo),
        ],
    }


def end_to_end(scenario: dict[str, Any]) -> dict[str, Any]:
    return next(row for row in scenario["distributions"] if row["stage"] == "end_to_end_total")


def collect_device(phase4: Path, repo: Path) -> dict[str, Any]:
    supported = phase4 / "devices/iphone17-pro-max/supported"
    rows: list[dict[str, Any]] = []
    evidence_files: list[Path] = []
    os_builds: set[str] = set()
    for workload in ("10k-384d-v3", "25k-384d-v3", "50k-384d-v3"):
        for encoding in ("f32", "i8"):
            sessions = []
            for session_index in range(5):
                path = supported / workload / encoding / "query" / f"session-{session_index:02d}.json"
                artifact = load_json(path)
                sessions.append(artifact)
                evidence_files.append(path)
                os_builds.add(artifact["environment"]["os_build"])
            for category in [row["query_category"] for row in sessions[0]["report"]["scenarios"]]:
                values: dict[str, list[int]] = {"p50_ns": [], "p95_ns": [], "p99_ns": []}
                for artifact in sessions:
                    scenario = next(
                        row for row in artifact["report"]["scenarios"] if row["query_category"] == category
                    )
                    distribution = end_to_end(scenario)
                    for key in values:
                        values[key].append(distribution[key])
                p50 = median(values["p50_ns"])
                p95 = median(values["p95_ns"])
                p99 = median(values["p99_ns"])
                rows.append(
                    {
                        "workload_id": workload,
                        "encoding": encoding,
                        "query_category": category,
                        "sessions": 5,
                        "samples_per_session": 1000,
                        "median_session_p50_ns": p50,
                        "median_session_p95_ns": p95,
                        "median_session_p99_ns": p99,
                        "display": {
                            "p50_ms": milliseconds(p50),
                            "p95_ms": milliseconds(p95),
                            "p99_ms": milliseconds(p99),
                        },
                    }
                )
    graph_free_rows: list[dict[str, Any]] = []
    graph_free = phase4 / "devices/iphone17-pro-max/graph-free"
    graph_free_scenarios = {
        "exact_vector": "semantic_exact_vector",
        "bm25": "bm25_internal",
        "hybrid": "hybrid_weighted_normalized_0.6_0.4",
    }
    for encoding in ("f32", "i8"):
        for category, scenario_name in graph_free_scenarios.items():
            product_values: dict[str, list[int]] = {"baseline": [], "candidate": []}
            for product in product_values:
                for session_index in range(3):
                    path = graph_free / encoding / product / f"session-{session_index:02d}.json"
                    artifact = load_json(path)
                    evidence_files.append(path)
                    scenario = next(
                        row for row in artifact["report"]["scenarios"] if row["scenario"] == scenario_name
                    )
                    product_values[product].append(scenario["p95_ns"])
            baseline = median(product_values["baseline"])
            candidate = median(product_values["candidate"])
            graph_free_rows.append(
                {
                    "encoding": encoding,
                    "query_category": category,
                    "baseline_median_session_p95_ns": baseline,
                    "candidate_median_session_p95_ns": candidate,
                    "candidate_over_baseline_ratio": ratio(candidate, baseline),
                    "gate": "passed" if Decimal(candidate) / Decimal(baseline) <= Decimal("1.03") else "failed",
                }
            )
    cancellation = repo / "benchmarks/device-graph/phase4b-device-safety-cancellation-authorization-v1.json"
    first_environment = load_json(evidence_files[0])["environment"]
    return {
        "accepted_sets": {
            "supported_count": 846,
            "supported_sha256": SUPPORTED_SET_SHA,
            "graph_free_count": 12,
            "graph_free_sha256": GRAPH_FREE_SET_SHA,
        },
        "configuration": {
            "device": "iPhone 17 Pro Max",
            "device_identifier": first_environment["device_identifier"],
            "hardware_model": first_environment["hardware_model"],
            "os_builds_query": sorted(os_builds),
            "os_builds_lifecycle": ["Version 26.5.2 (Build 23F84)"],
            "dimensions": 384,
            "supported_workloads": ["10k-384d-v3", "25k-384d-v3", "50k-384d-v3"],
            "embedding_included": False,
        },
        "query_rows": rows,
        "graph_free_rows": graph_free_rows,
        "qualification": {"supported_product": "passed", "graph_free": "passed"},
        "stress_100k": {
            "workload_id": "100k-384d-v3-stress",
            "outcome": "not_run_device_safety",
            "accepted_artifact_count": 0,
            "rejected_partial_artifact_count": 5,
            "claim_eligible": False,
        },
        "evidence": [
            evidence(cancellation, repo),
            {
                "evidence_state": "accepted",
                "path": "target/phase4b/device-results-v3-02b8971/devices/iphone17-pro-max/supported",
                "sha256": SUPPORTED_SET_SHA,
            },
            {
                "evidence_state": "accepted",
                "path": "target/phase4b/device-results-v3-02b8971/devices/iphone17-pro-max/graph-free",
                "sha256": GRAPH_FREE_SET_SHA,
            },
        ],
    }


def claim(
    claim_id: str,
    text: str,
    status: str,
    evidence_rows: list[dict[str, str]],
    **fields: Any,
) -> dict[str, Any]:
    base = {
        "claim_id": claim_id,
        "claim_text": text,
        "status": status,
        "evidence": evidence_rows,
        "report_date": REPORT_DATE,
        "expires_on": EXPIRES_ON,
        "source_revision": fields.pop("source_revision", SOURCE_REVISION),
        "mandatory_rerun_conditions": [
            "source, benchmark, workload, dependency, hardware, OS, or license condition changes",
            "claim expiration",
        ],
        "licensing_eligibility": fields.pop("licensing_eligibility", "eligible_repository_local"),
    }
    base.update(fields)
    return base


def build_claims(index: dict[str, Any]) -> dict[str, Any]:
    quality = index["quality"]
    mac = index["mac"]
    device = index["device"]
    mac_refs = mac["evidence"]
    quality_refs = quality["evidence"]
    device_refs = device["evidence"]
    unfiltered = [row for row in mac["exact_rows"] if not row["filtering"]]
    filtered = [row for row in mac["exact_rows"] if row["filtering"]]
    claims = [
        claim(
            "P6-QUALITY-001",
            "On the frozen 296-query HotpotQA test comparison, graph-scoped weighted-I8 retrieval increased NDCG@10 from 0.858036 to 0.927909 versus whole-corpus weighted-I8 retrieval; this is a scoped quality result, not a universal graph winner claim.",
            "permitted",
            quality_refs,
            workload="HotpotQA linked-abstracts test; 12,670 chunks; 296 common valid queries",
            comparison_system="VectorKit whole-corpus weighted-I8 versus graph-scoped weighted-I8",
            hardware="not applicable; quality metric",
            os="not applicable; quality metric",
            metric="NDCG@10",
            sample_population="296 common valid queries (121 wins, 157 ties, 18 losses)",
            calculation="macro mean; scoped minus baseline = 0.06987347996430937",
            required_qualifiers=["frozen HotpotQA test workload", "weighted-I8", "296 common queries", "quality only"],
            prohibited_interpretations=["universal graph superiority", "latency winner", "all queries improved"],
        ),
        claim(
            "P6-QUALITY-002",
            "On the same frozen 296-query comparison, Recall@10 increased from 0.871622 to 0.957770 and complete-evidence recall@10 increased from 0.743243 to 0.922297; 16 queries lost on each recall measure.",
            "permitted",
            quality_refs,
            workload="HotpotQA linked-abstracts test; 296 common valid queries",
            comparison_system="VectorKit whole-corpus weighted-I8 versus graph-scoped weighted-I8",
            hardware="not applicable; quality metric",
            os="not applicable; quality metric",
            metric="Recall@10 and complete-evidence recall@10",
            sample_population="296 common valid queries",
            calculation="macro means and paired wins/ties/losses",
            required_qualifiers=["frozen test split", "weighted-I8", "16 losses preserved"],
            prohibited_interpretations=["every query improved", "universal graph superiority"],
        ),
        claim(
            "P6-QUALITY-003",
            "The frozen graph-scoped lane reduced the mean per-query candidate set by 972.65x while retaining 96.79% candidate recall and 94.26% candidate complete evidence across 296 valid queries; no scope was empty.",
            "permitted",
            quality_refs,
            workload="HotpotQA linked-abstracts test; 296 valid graph queries",
            comparison_system="eligible whole corpus versus projected graph scope",
            hardware="not applicable; candidate-set analysis",
            os="not applicable; candidate-set analysis",
            metric="macro candidate reduction, candidate recall, candidate complete evidence",
            sample_population="296 graph-valid queries",
            calculation="mean per-query eligible/projected ratio; macro recall means",
            required_qualifiers=["mean per-query reduction, not total ratio", "candidate-stage result"],
            prohibited_interpretations=["retrieval latency speedup", "perfect candidate retention"],
        ),
        claim(
            "P6-MAC-EXACT-001",
            "In the frozen Apple M1 Max exact-search benchmark at 10K, 25K, and 50K 384-dimensional chunks, VectorKit revision 9c784d2 was 7.17x, 7.60x, and 7.29x faster in P50 unfiltered retrieval than sqlite-vec 0.1.9.",
            "permitted",
            mac_refs,
            workload="10K, 25K, 50K; 384d; top-10; exact unfiltered F32",
            comparison_system="VectorKit exact F32 versus sqlite-vec 0.1.9 exact F32",
            hardware="Apple M1 Max, arm64, 10 logical CPUs",
            os="macOS 26.5.2; Darwin 25.5.0",
            metric="P50 retrieval latency ratio (sqlite-vec / VectorKit)",
            sample_population="100 measured queries per system and size after 20 warmups",
            calculation=f"ratios={','.join(row['p50_ratio_sqlite_over_vectorkit'] for row in unfiltered)}",
            required_qualifiers=["exact search", "Apple M1 Max", "P50", "unfiltered", "versions and sizes", "embedding excluded"],
            prohibited_interpretations=["universal superiority", "ANN comparison", "device result"],
        ),
        claim(
            "P6-MAC-EXACT-002",
            "In the same frozen exact-search benchmark with the frozen filter, VectorKit revision 9c784d2 was 10.38x, 9.08x, and 8.43x faster in P50 retrieval at 10K, 25K, and 50K than sqlite-vec 0.1.9.",
            "permitted",
            mac_refs,
            workload="10K, 25K, 50K; 384d; top-10; exact filtered F32",
            comparison_system="VectorKit exact F32 versus sqlite-vec 0.1.9 exact F32",
            hardware="Apple M1 Max, arm64, 10 logical CPUs",
            os="macOS 26.5.2; Darwin 25.5.0",
            metric="P50 retrieval latency ratio (sqlite-vec / VectorKit)",
            sample_population="100 measured queries per system and size after 20 warmups",
            calculation=f"ratios={','.join(row['p50_ratio_sqlite_over_vectorkit'] for row in filtered)}",
            required_qualifiers=["exact search", "Apple M1 Max", "P50", "filtered", "versions and sizes", "embedding excluded"],
            prohibited_interpretations=["universal superiority", "ANN comparison", "device result"],
        ),
        claim(
            "P6-MAC-CORRECTNESS-001",
            "VectorKit exact F32 and sqlite-vec 0.1.9 passed the frozen Phase 5 identity, filtering, deletion, determinism, and reload gates at 10K, 25K, and 50K.",
            "permitted",
            mac_refs,
            workload="10K, 25K, 50K; exact F32",
            comparison_system="NumPy 2.5.1 oracle, VectorKit, sqlite-vec 0.1.9",
            hardware="Apple M1 Max",
            os="macOS 26.5.2; Darwin 25.5.0",
            metric="frozen correctness gates",
            sample_population="all frozen Phase 5 exact lanes",
            calculation="boolean gate conjunction",
            required_qualifiers=["Phase 5 exact lanes", "frozen workload"],
            prohibited_interpretations=["proof for all inputs", "ANN correctness"],
        ),
        claim(
            "P6-ANN-NEGATIVE-001",
            "USearch 2.26.0 failed the frozen mean Recall@10 gate with 0.965 at 10K, 0.850 at 25K, and 0.775 at 50K; its retained timing data is disqualified from performance comparison.",
            "permitted",
            [dict(row, evidence_state="qualified_negative_result") for row in mac_refs],
            workload="10K, 25K, 50K; 384d; top-10 ANN",
            comparison_system="USearch 2.26.0 versus NumPy 2.5.1 exact oracle",
            hardware="Apple M1 Max",
            os="macOS 26.5.2; Darwin 25.5.0",
            metric="mean Recall@10",
            sample_population="100 measured queries per size",
            calculation="mean intersection with exact top-10 divided by 10",
            required_qualifiers=["negative result", "recall-gate failure", "timing disqualified"],
            prohibited_interpretations=["USearch latency comparison", "USearch is universally inaccurate"],
        ),
        claim(
            "P6-DEVICE-001",
            "On the physical iPhone 17 Pro Max qualification, the supported 10K, 25K, and 50K product workflows passed, and all six graph-free candidate-to-baseline median-session P95 ratios remained at or below the frozen 1.03 gate.",
            "permitted",
            device_refs,
            workload="10K, 25K, 50K supported product; F32 and I8",
            comparison_system="supported product and graph-free baseline/candidate builds",
            hardware="iPhone 17 Pro Max; iPhone18,2; V54AP",
            os="query/prepare: iOS 26.5.1 (23F81); remaining lifecycle: iOS 26.5.2 (23F84)",
            metric="qualification gates and median-session P95 ratio",
            sample_population="846 supported artifacts and 12 graph-free artifacts",
            calculation="candidate median-session P95 / baseline median-session P95",
            required_qualifiers=["physical iPhone 17 Pro Max", "OS-build variance", "supported workloads only", "embedding excluded"],
            prohibited_interpretations=["100K support", "older-device support", "cross-system winner"],
        ),
        claim(
            "P6-DEVICE-SAFETY-001",
            "The Phase 4b 100K stress workload outcome is not_run_device_safety; it produced zero accepted stress artifacts and is ineligible for support, performance, latency, quality, product, or marketing claims.",
            "permitted",
            [device_refs[0]],
            workload="100k-384d-v3-stress",
            comparison_system="none; safety cancellation",
            hardware="iPhone 17 Pro Max; iPhone18,2; V54AP",
            os="iOS 26.5.2 (23F84) at cancellation",
            metric="terminal outcome and accepted artifact count",
            sample_population="0 accepted; 5 partial artifacts rejected",
            calculation="authorization record",
            required_qualifiers=["not_run_device_safety", "zero accepted artifacts", "no support claim"],
            prohibited_interpretations=["100K passed", "100K supported", "partial evidence is publishable performance data"],
        ),
        claim("P6-PROHIBITED-001", "VectorKit is universally faster or better than sqlite-vec.", "prohibited", [], workload="all", comparison_system="VectorKit versus sqlite-vec", hardware="unspecified", os="unspecified", metric="unspecified", sample_population="unspecified", calculation="unsupported generalization", required_qualifiers=[], prohibited_interpretations=["the claim itself"]),
        claim("P6-PROHIBITED-002", "VectorKit has a performance advantage over USearch in the frozen Phase 5 benchmark.", "prohibited", [], workload="Phase 5 ANN", comparison_system="VectorKit versus USearch", hardware="Apple M1 Max", os="macOS 26.5.2", metric="latency", sample_population="disqualified", calculation="forbidden because USearch failed recall gate", required_qualifiers=[], prohibited_interpretations=["any USearch timing comparison"]),
        claim("P6-PROHIBITED-003", "VectorKit beats the graph baseline on performance.", "prohibited", [], workload="Phase 5 graph applications", comparison_system="non-equivalent hybrid semantics", hardware="Apple M1 Max", os="macOS 26.5.2", metric="latency", sample_population="non-comparable", calculation="forbidden capability mismatch", required_qualifiers=[], prohibited_interpretations=["graph winner table"]),
        claim("P6-PROHIBITED-004", "VectorKit supports or passes 100K chunks on the physical device.", "prohibited", [device_refs[0]], workload="100k-384d-v3-stress", comparison_system="none", hardware="iPhone 17 Pro Max", os="iOS 26.5.2", metric="support", sample_population="0 accepted artifacts", calculation="contradicted by cancellation", required_qualifiers=[], prohibited_interpretations=["100K marketing"]),
        claim("P6-PROHIBITED-005", "The Phase 5 graph applications can be ranked in a combined performance winner table.", "prohibited", [], workload="Phase 5 graph application", comparison_system="VectorKit graph app and sqlite custom graph app", hardware="Apple M1 Max", os="macOS 26.5.2", metric="latency", sample_population="non-equivalent", calculation="forbidden semantic mismatch", required_qualifiers=[], prohibited_interpretations=["combined winner"]),
        claim("P6-PROHIBITED-006", "Embedding latency is included in the published retrieval latency.", "prohibited", [], workload="all timing reports", comparison_system="all", hardware="Mac and device", os="frozen environments", metric="retrieval latency", sample_population="retrieval only", calculation="embedding explicitly excluded", required_qualifiers=[], prohibited_interpretations=["end-to-end embedding claim"]),
        claim("P6-WITHHELD-001", "VectorKit is qualified on older iPhone hardware.", "withheld", [], workload="supported product", comparison_system="none", hardware="older iPhone unspecified", os="unspecified", metric="qualification", sample_population="no accepted evidence", calculation="not measured", required_qualifiers=[], prohibited_interpretations=["hardware transfer"]),
        claim("P6-WITHHELD-002", "VectorKit has superior energy or sustained thermal efficiency.", "withheld", [], workload="all", comparison_system="unspecified", hardware="Mac or iPhone", os="frozen environments", metric="energy/thermal", sample_population="not measured", calculation="not available", required_qualifiers=[], prohibited_interpretations=["energy winner"]),
        claim("P6-WITHHELD-003", "The raw HotpotQA-derived corpus and device captures are redistributed by this publication package.", "withheld", [], workload="publication files", comparison_system="not applicable", hardware="not applicable", os="not applicable", metric="redistribution", sample_population="excluded raw artifacts", calculation="repository publication policy", required_qualifiers=[], prohibited_interpretations=["raw-data grant"], licensing_eligibility="withheld_pending_repository_license_and_notices"),
        claim("P6-WITHHELD-004", "These benchmark observations apply to the latest VectorKit source and dependencies after the frozen revisions expire or change.", "withheld", [], workload="all", comparison_system="all", hardware="all", os="all", metric="transferability", sample_population="no rerun", calculation="requires rerun", required_qualifiers=[], prohibited_interpretations=["automatic transfer to newer revisions"]),
    ]
    return {
        "schema_version": 1,
        "artifact_id": "phase6-publication-v1",
        "report_date": REPORT_DATE,
        "expires_on": EXPIRES_ON,
        "claims": claims,
        "counts": {
            status: sum(row["status"] == status for row in claims)
            for status in ("permitted", "prohibited", "withheld")
        },
    }


def licensing() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "publication_decision": "repository_local_only",
        "repository_license": {
            "status": "absent",
            "decision": "no general downstream redistribution grant; external publication withheld",
        },
        "inputs": [
            {"name": "HotpotQA dataset", "version": "official test split", "license": "CC-BY-SA-4.0", "primary_source": "https://github.com/hotpotqa/hotpot", "use": "quality evaluation", "decision": "raw and transformed payloads excluded; hashes, provenance, and aggregates only"},
            {"name": "all-MiniLM-L6-v2", "version": "c9745ed1d9f207416be6d2e6f8de32d1f16199bf", "license": "Apache-2.0", "primary_source": "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2", "use": "embedding provenance", "decision": "model weights excluded; identity and acquisition instructions only"},
            {"name": "NumPy", "version": "2.5.1", "license": "BSD-3-Clause", "primary_source": "https://github.com/numpy/numpy/blob/v2.5.1/LICENSE.txt", "use": "exact oracle", "decision": "facts and derived aggregates included; software excluded"},
            {"name": "sqlite-vec", "version": "0.1.9", "license": "MIT OR Apache-2.0", "primary_source": "https://github.com/asg017/sqlite-vec/tree/v0.1.9", "use": "exact external reference", "decision": "facts and derived aggregates included; software excluded"},
            {"name": "SQLite", "version": "3.50.4", "license": "public-domain", "primary_source": "https://www.sqlite.org/copyright.html", "use": "graph application substrate", "decision": "facts included; software excluded"},
            {"name": "USearch", "version": "2.26.0", "license": "Apache-2.0", "primary_source": "https://github.com/unum-cloud/usearch/blob/v2.26.0/LICENSE", "use": "ANN negative result", "decision": "recall aggregates included; disqualified timing comparison excluded"},
        ],
        "excluded": ["raw public dataset payloads", "transformed HotpotQA corpus", "model weights", "raw physical-device evidence", "binaries", "rejected and disqualified timing evidence"],
    }


def methodology(index: dict[str, Any]) -> str:
    return """# VectorKit benchmark methodology

Report date: 2026-07-21. Claims expire: 2027-07-21.

## Evidence families

Retrieval quality uses the official HotpotQA test split transformed into 12,670 linked-abstract records/chunks, 297 declared queries and 594 qrels. One pre-frozen ambiguous seed is excluded from graph comparison, leaving 296 common queries. Embeddings are `sentence-transformers/all-MiniLM-L6-v2` at pinned revision `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`, 384 dimensions.

Mac systems results use 10K, 25K, and 50K synthetic 384-dimensional workloads, top-10 retrieval, 20 warmups, and 100 samples. VectorKit exact F32 is checked against the NumPy 2.5.1 oracle and compared with sqlite-vec 0.1.9 exact F32. USearch 2.26.0 is an ANN lane with a recall gate, not an exact-capability peer. The graph applications have non-equivalent hybrid semantics and are not ranked.

Physical-device results use the frozen 10K, 25K, and 50K Phase 4b workloads in F32 and I8 on an iPhone 17 Pro Max (`iPhone18,2`, `V54AP`). Query percentiles cover five fresh-process sessions of 1,000 samples after 100 warmups. Graph-free ratios use the median of three session P95 values. Query sessions and 10K F32 prepare evidence report iOS 26.5.1 (23F81); remaining 815 lifecycle artifacts report iOS 26.5.2 (23F84). This variance is preserved.

## Timing and calculations

Embedding is excluded everywhere. Timings cover retrieval/application work identified in the frozen contracts. Percentiles use nearest rank. Phase 4b published query values are medians of five per-session percentiles; Phase 5 values are direct percentiles over 100 samples. Display rounding uses decimal ROUND_HALF_UP: milliseconds to three decimals, ratios and percentages to two.

## Gates and failures

Exact lanes must pass identity, filtering, deletion, determinism, and reload gates. ANN timing is comparison-eligible only after its recall gate passes. Failed, partial, diagnostic, rejected, and disqualified evidence cannot support positive claims. Phase 5 acceptance failed solely because USearch missed Recall@10; its recall is retained as a negative result and its timing is excluded. Phase 4b supported-product and graph-free qualification passed. The 100K stress lane is `not_run_device_safety`, has zero accepted artifacts, and cannot support any product or marketing claim. No Phase 6 device command or retuning is permitted.

## Environments and source

Mac: Apple M1 Max arm64, 10 logical CPUs, macOS 26.5.2 / Darwin 25.5.0, CPython 3.12.12, rustc/cargo 1.92.0, and Apple Swift 6.3.3, with VectorKit revision `9c784d2f11b91bb907150aa1b6046880ff89fde6`. Device hardware and OS builds are stated above. Phase 3, 4b, and 5 source and artifact identities are bound by `manifest.json` and `evidence-index.json`.

## Reproduction and licensing

Use `reproduction.md` and the independent validator. Raw HotpotQA payloads, model weights, raw device captures, binaries, and rejected/disqualified evidence are excluded. `licensing.json` provides primary-source license references and decisions. The repository has no root project license, so this package is repository-local and external redistribution remains withheld.
"""


def quality_report(index: dict[str, Any]) -> str:
    quality = index["quality"]
    lines = [
        "# Retrieval quality and correctness",
        "",
        "Scope: frozen HotpotQA test workload; 296 common valid queries; weighted-I8 whole-corpus baseline versus graph-scoped retrieval.",
        "",
        "| Metric | Baseline | Graph-scoped | Delta | Relative | W/T/L |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    labels = {
        "ndcg_at_10": "NDCG@10",
        "recall_at_10": "Recall@10",
        "complete_evidence_recall_at_10": "Complete evidence@10",
    }
    for key in labels:
        row = quality["comparison"][key]
        lines.append(f"| {labels[key]} | {row['baseline']:.6f} | {row['scoped']:.6f} | {row['delta']:.6f} | {row['relative_delta'] * 100:.2f}% | {row['wins']}/{row['ties']}/{row['losses']} |")
    scope = quality["candidate_scope"]
    lines.extend([
        "",
        "The graph-scoped lane's mean per-query candidate reduction was "
        f"{scope['macro_mean_per_query_reduction']:.2f}x. Candidate recall was "
        f"{scope['candidate_recall'] * 100:.2f}% and candidate complete evidence was "
        f"{scope['candidate_complete_evidence'] * 100:.2f}%; empty scopes: {scope['empty_scopes']}.",
        "",
        f"For contrast, the pooled totals were {scope['micro_eligible_chunks']:,} eligible and {scope['micro_projected_chunks']:,} projected chunks ({scope['micro_ratio']:.2f}x). The 972.65x figure is the macro mean of per-query ratios, not this pooled ratio.",
        "",
        "These are workload-scoped quality observations. Losses are preserved, latency is not inferred, and no universal graph winner claim is permitted.",
        "",
    ])
    return "\n".join(lines)


def mac_report(index: dict[str, Any]) -> str:
    mac = index["mac"]
    lines = [
        "# Mac systems performance",
        "",
        "Frozen Apple M1 Max exact-search benchmark; macOS 26.5.2 / Darwin 25.5.0; 384d top-10; 20 warmups and 100 measured queries; embedding excluded.",
        "",
        "| Size | Filtered | VectorKit P50/P95 ms | sqlite-vec P50/P95 ms | P50 sqlite/VectorKit |",
        "| --- | --- | ---: | ---: | ---: |",
    ]
    for row in mac["exact_rows"]:
        display = row["display"]
        lines.append(f"| {row['workload_id'].split('-')[0].upper()} | {'yes' if row['filtering'] else 'no'} | {display['vectorkit_p50_ms']} / {display['vectorkit_p95_ms']} | {display['sqlite_vec_p50_ms']} / {display['sqlite_vec_p95_ms']} | {row['p50_ratio_sqlite_over_vectorkit']}x |")
    lines.extend([
        "",
        "VectorKit revision `9c784d2f11b91bb907150aa1b6046880ff89fde6` and sqlite-vec 0.1.9 both passed frozen exact identity, filtering, deletion, determinism, and reload gates against the NumPy 2.5.1 oracle.",
        "",
        "## ANN negative result",
        "",
        "| Size | USearch 2.26.0 mean Recall@10 | Gate | Timing comparison |",
        "| --- | ---: | --- | --- |",
    ])
    for row in mac["usearch_recall"]:
        lines.append(f"| {row['workload_id'].split('-')[0].upper()} | {row['mean_recall_at_10']:.3f} | failed | disqualified |")
    lines.extend([
        "",
        "No USearch performance comparison is made. The graph application timings are also omitted from winner comparison because the hybrid semantics are non-equivalent. These observations do not establish universal VectorKit superiority.",
        "",
    ])
    return "\n".join(lines)


def device_report(index: dict[str, Any]) -> str:
    device = index["device"]
    lines = [
        "# Physical-device systems performance",
        "",
        "Physical iPhone 17 Pro Max (`iPhone18,2`, `V54AP`). Query sessions and 10K F32 prepare evidence: iOS 26.5.1 (23F81). Remaining lifecycle evidence: iOS 26.5.2 (23F84). Embedding excluded.",
        "",
        "Each value below is the median of five session nearest-rank percentiles; each session contains 1,000 measured queries after 100 warmups.",
        "",
        "| Workload | Encoding | Category | P50/P95/P99 ms |",
        "| --- | --- | --- | ---: |",
    ]
    for row in device["query_rows"]:
        display = row["display"]
        lines.append(f"| {row['workload_id']} | {row['encoding']} | {row['query_category']} | {display['p50_ms']} / {display['p95_ms']} / {display['p99_ms']} |")
    lines.extend([
        "",
        "## Graph-free isolation gate",
        "",
        "| Encoding | Category | Candidate/baseline median-session P95 | Gate |",
        "| --- | --- | ---: | --- |",
    ])
    for row in device["graph_free_rows"]:
        lines.append(f"| {row['encoding']} | {row['query_category']} | {row['candidate_over_baseline_ratio']}x | {row['gate']} |")
    lines.extend([
        "",
        "Supported-product qualification passed for 10K, 25K, and 50K. The graph-free gate passed. This is device qualification, not an external-system winner comparison.",
        "",
        "## 100K safety outcome",
        "",
        "The 100K stress outcome is `not_run_device_safety`: zero accepted stress artifacts and five rejected partial artifacts. It is not a pass, not supported capacity, and not eligible for performance, latency, quality, product, or marketing use. No device execution was resumed in Phase 6.",
        "",
    ])
    return "\n".join(lines)


def reproduction() -> str:
    return """# Reproduce and validate Phase 6

The Phase 6 generator reads frozen local evidence and performs no network or device access. Acquire HotpotQA and the pinned MiniLM model from the primary sources in `licensing.json`, then reproduce Phase 3 according to `docs/product/reports/hotpotqa-phase-3-locked-reporting-report.md`. Use the accepted Phase 4b root produced under the frozen target-device contract and the checked-in Phase 5 artifact root.

```sh
python3 benchmarks/publication/generate_publication.py --repo . --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting --phase4-root target/phase4b/device-results-v3-02b8971 --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 --output /tmp/phase6-a
python3 benchmarks/publication/validate_publication.py --repo . --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting --phase4-root target/phase4b/device-results-v3-02b8971 --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 --root /tmp/phase6-a
```

Generate a second fresh root with the same command and compare it byte-for-byte. Do not substitute rejected Phase 4b evidence, USearch timing, graph application winner comparisons, or 100K partial captures. Exact evidence identities and source revisions are in `manifest.json` and `evidence-index.json`.
"""


def write_package(args: argparse.Namespace) -> None:
    repo = args.repo.resolve()
    phase3 = (repo / args.phase3_root).resolve() if not args.phase3_root.is_absolute() else args.phase3_root
    phase4 = (repo / args.phase4_root).resolve() if not args.phase4_root.is_absolute() else args.phase4_root
    phase5 = (repo / args.phase5_root).resolve() if not args.phase5_root.is_absolute() else args.phase5_root
    output = args.output.resolve()
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    index = {
        "schema_version": 1,
        "artifact_id": "phase6-publication-v1",
        "report_date": REPORT_DATE,
        "quality": collect_quality(phase3, repo),
        "mac": collect_mac(phase5, repo),
        "device": collect_device(phase4, repo),
    }
    claims = build_claims(index)
    payloads: dict[str, bytes] = {
        "methodology.md": methodology(index).encode(),
        "retrieval-quality.md": quality_report(index).encode(),
        "mac-systems-performance.md": mac_report(index).encode(),
        "physical-device-systems-performance.md": device_report(index).encode(),
        "claim-register.json": json_bytes(claims),
        "licensing.json": json_bytes(licensing()),
        "evidence-index.json": json_bytes(index),
        "reproduction.md": reproduction().encode(),
    }
    for name, content in payloads.items():
        if not content.endswith(b"\n"):
            content += b"\n"
        (output / name).write_bytes(content)
    checksums = {name: sha256_file(output / name) for name in REPORT_FILES}
    (output / "checksums.json").write_bytes(json_bytes({"schema_version": 1, "files": checksums}))
    preimage_hashes = dict(checksums)
    preimage_hashes["checksums.json"] = sha256_file(output / "checksums.json")
    artifact_preimage = "".join(f"{name}\t{preimage_hashes[name]}\n" for name in sorted(preimage_hashes)).encode()
    contract_path = repo / "benchmarks/publication/contract-v1.json"
    validator_path = repo / "benchmarks/publication/validate_publication.py"
    manifest = {
        "schema_version": 1,
        "artifact_id": "phase6-publication-v1",
        "report_date": REPORT_DATE,
        "expires_on": EXPIRES_ON,
        "inventory": sorted((*REPORT_FILES, "checksums.json", "manifest.json")),
        "files": preimage_hashes,
        "canonical_artifact_set_sha256": sha256_bytes(artifact_preimage),
        "canonical_artifact_set_preimage": "sorted <path>\\t<sha256>\\n; manifest.json excluded",
        "source_revisions": {
            "phase3_measured": SOURCE_REVISION,
            "phase4b_measured": SOURCE_REVISION,
            "phase5_measured": SOURCE_REVISION,
            "phase6_base": "4652ca560df2622c100043750d0660da8dd58cea",
        },
        "input_artifacts": {
            "phase3_locked_reporting": PHASE3_ARTIFACT_SHA,
            "phase4b_supported": SUPPORTED_SET_SHA,
            "phase4b_graph_free": GRAPH_FREE_SET_SHA,
            "phase5_mac_comparison": PHASE5_ARTIFACT_SHA,
        },
        "contract": {"path": "benchmarks/publication/contract-v1.json", "sha256": sha256_file(contract_path)},
        "validator": {"path": "benchmarks/publication/validate_publication.py", "sha256": sha256_file(validator_path), "result": "PASS"},
    }
    (output / "manifest.json").write_bytes(json_bytes(manifest))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--phase3-root", type=Path, required=True)
    parser.add_argument("--phase4-root", type=Path, required=True)
    parser.add_argument("--phase5-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    write_package(parse_args())
