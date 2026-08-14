#!/usr/bin/env python3
"""Validate raw Apple end-to-end benchmark evidence independently of Swift."""

from __future__ import annotations

import argparse
import json
import math
import re
import statistics
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


HEX_64 = re.compile(r"^[0-9a-f]{64}$")
STAGES = {
    "embedding_total": "embedding_total_ns",
    "retrieval_total": "retrieval_total_ns",
    "end_to_end_text_search": "end_to_end_text_search_ns",
}


class ValidationError(ValueError):
    pass


def nearest_rank(values: list[int], percentile: float) -> int:
    if not values:
        raise ValidationError("cannot calculate a percentile for no values")
    ordered = sorted(values)
    return ordered[max(1, math.ceil(percentile * len(ordered))) - 1]


def expected_summary(values: list[int]) -> dict[str, int]:
    return {
        "count": len(values),
        "minimum_ns": min(values),
        "maximum_ns": max(values),
        "mean_ns": round(sum(values) / len(values)),
        "p50_ns": nearest_rank(values, 0.50),
        "p95_ns": nearest_rank(values, 0.95),
        "p99_ns": nearest_rank(values, 0.99),
    }


def load_descriptors(directory: Path, version: str = "v1") -> tuple[dict[str, Any], dict[str, Any]]:
    workloads = json.loads((directory / f"workloads-{version}.json").read_text(encoding="utf-8"))
    protocol = json.loads((directory / f"protocol-{version}.json").read_text(encoding="utf-8"))
    return workloads, protocol


def validate_report(
    report: dict[str, Any],
    query_document: dict[str, Any],
    workloads_document: dict[str, Any],
    protocol: dict[str, Any],
) -> tuple[str, str, str, str, int]:
    if report.get("schema_version") != 1 or report.get("contract_version") != protocol["contract_version"]:
        raise ValidationError("unsupported result schema or contract version")
    workloads = {item["id"]: item for item in workloads_document["workloads"]}
    profiles = {item["id"]: item for item in workloads_document["model_profiles"]}
    workload_id = report.get("workload_id")
    profile_id = report.get("profile_id")
    if workload_id not in workloads or profile_id not in profiles:
        raise ValidationError("unknown workload or profile ID")
    workload = workloads[workload_id]
    profile = profiles[profile_id]
    if report.get("workload_classification") != workload["classification"]:
        raise ValidationError("workload classification mismatch")
    if report.get("marketing_eligible") != workload["marketing_eligible"]:
        raise ValidationError("marketing eligibility mismatch")
    if report.get("supported_v1_capacity_changed") is not False:
        raise ValidationError("result changed the V1 support capacity")
    if report.get("profile_classification") != profile["classification"]:
        raise ValidationError("profile classification mismatch")
    if report.get("top_k") != 10 or report.get("warmup_count") != 50:
        raise ValidationError("top K or warmup count mismatch")
    mode = report.get("search_mode")
    if mode not in protocol["search"]["modes"]:
        raise ValidationError("unknown search mode")

    samples = report.get("samples")
    if not isinstance(samples, list) or len(samples) != 750:
        raise ValidationError("each session must retain exactly 750 raw samples")
    expected_schedule = query_document["schedule"]
    if len(expected_schedule) != 750:
        raise ValidationError("source query schedule is not V1")
    process_id = report.get("environment", {}).get("process_id")
    for ordinal, (sample, expected_query_id) in enumerate(zip(samples, expected_schedule, strict=True)):
        if sample.get("ordinal") != ordinal or sample.get("query_id") != expected_query_id:
            raise ValidationError(f"sample {ordinal} does not match the frozen schedule")
        start = sample.get("start_clock_ns")
        end = sample.get("end_clock_ns")
        embedding = sample.get("embedding_total_ns")
        retrieval = sample.get("retrieval_total_ns")
        total = sample.get("end_to_end_text_search_ns")
        if not all(isinstance(value, int) and value > 0 for value in (start, end, embedding, retrieval, total)):
            raise ValidationError(f"sample {ordinal} has an invalid integer duration")
        if end <= start or total != end - start or total < embedding + retrieval:
            raise ValidationError(f"sample {ordinal} violates direct nested timing")
        if sample.get("result_count") != 10:
            raise ValidationError(f"sample {ordinal} did not return decoded top-10 results")
        if not sample.get("top_result_identity") or not HEX_64.fullmatch(sample.get("result_identity_digest", "")):
            raise ValidationError(f"sample {ordinal} lacks deterministic result evidence")
    per_query_evidence: dict[str, set[tuple[str, str]]] = defaultdict(set)
    for sample in samples:
        per_query_evidence[sample["query_id"]].add(
            (sample["top_result_identity"], sample["result_identity_digest"])
        )
    if any(len(evidence) != 1 for evidence in per_query_evidence.values()):
        raise ValidationError("repeated queries changed result identities within one session")

    summaries = report.get("summaries", {})
    if set(summaries) != set(STAGES):
        raise ValidationError("stage summaries are incomplete")
    for stage, key in STAGES.items():
        expected = expected_summary([sample[key] for sample in samples])
        if summaries[stage] != expected:
            raise ValidationError(f"{stage} summary was not derived from retained raw samples")

    environment = report.get("environment", {})
    if environment.get("debugger_attached") is not False:
        raise ValidationError("debugger-attached runs are invalid")
    if environment.get("graph_linked") is not False or environment.get("onnx_runtime_linked") is not False:
        raise ValidationError("graph or ONNX linkage is forbidden")
    platform = environment.get("platform")
    if platform not in ("mac", "iphone") or environment.get("architecture") != "arm64":
        raise ValidationError("platform or architecture mismatch")
    expected_device = protocol["devices"][platform]
    if environment.get("hardware") != expected_device["model_identifier"]:
        raise ValidationError("physical hardware identifier mismatch")
    if platform == "iphone":
        validity = report.get("iphone_validity")
        required = {
            "physical_device": True,
            "foreground_start": True,
            "foreground_end": True,
            "network_disabled": True,
            "low_power_mode": False,
            "memory_warning": False,
        }
        if not isinstance(validity, dict) or any(validity.get(key) != value for key, value in required.items()):
            raise ValidationError("iPhone validity evidence is missing or invalid")
        if not 50 <= validity.get("battery_percent", -1) <= 90:
            raise ValidationError("iPhone battery is outside the valid range")
        allowed_states = protocol["iphone_validity"].get("battery_state")
        if allowed_states is not None:
            battery_state = validity.get("battery_state")
            if battery_state not in allowed_states:
                raise ValidationError("iPhone battery state is invalid")
            if validity.get("charging") != (battery_state == "charging"):
                raise ValidationError("iPhone charging flag does not match battery state")
        elif validity.get("charging") is not False:
            raise ValidationError("iPhone must not be charging")
        if validity.get("thermal_start") != "nominal" or validity.get("thermal_end") not in ("nominal", "fair"):
            raise ValidationError("iPhone thermal state is invalid")

    if workload["classification"] == "stress" and report.get("marketing_eligible") is not False:
        raise ValidationError("100K stress evidence cannot be marketing eligible")
    session_id = report.get("session_id")
    if not isinstance(session_id, str) or not session_id:
        raise ValidationError("session ID is missing")
    return platform, workload_id, profile_id, mode, process_id


def validate_collection(
    reports: list[dict[str, Any]],
    queries: dict[str, Any],
    workloads: dict[str, Any],
    protocol: dict[str, Any],
    require_complete_sessions: bool,
) -> dict[str, Any]:
    groups: dict[tuple[str, str, str, str], list[dict[str, Any]]] = defaultdict(list)
    seen_sessions: set[str] = set()
    process_modes: dict[tuple[str, int], set[str]] = defaultdict(set)
    for report in reports:
        key = validate_report(report, queries, workloads, protocol)
        group_key = key[:4]
        process_modes[(key[0], key[4])].add(key[3])
        session_id = report["session_id"]
        if session_id in seen_sessions:
            raise ValidationError(f"duplicate session ID: {session_id}")
        seen_sessions.add(session_id)
        groups[group_key].append(report)
    if any(len(modes) != 1 for modes in process_modes.values()):
        raise ValidationError("one process executed more than one search mode")

    aggregate: dict[str, Any] = {}
    for key, items in sorted(groups.items()):
        if require_complete_sessions and len(items) < 3:
            raise ValidationError(f"configuration {key} has fewer than three valid sessions")
        p95_values = [item["summaries"]["end_to_end_text_search"]["p95_ns"] for item in items]
        cross_session_evidence: dict[str, set[tuple[str, str]]] = defaultdict(set)
        for item in items:
            for sample in item["samples"]:
                cross_session_evidence[sample["query_id"]].add(
                    (sample["top_result_identity"], sample["result_identity_digest"])
                )
        if any(len(evidence) != 1 for evidence in cross_session_evidence.values()):
            raise ValidationError(f"configuration {key} changed result identities across sessions")
        aggregate["/".join(key)] = {
            "median_session_p95_ns": int(statistics.median(p95_values)),
            "session_count": len(items),
            "session_p95_ns": sorted(p95_values),
        }
    return aggregate


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--queries", type=Path, required=True)
    parser.add_argument("--descriptor-directory", type=Path, default=Path(__file__).parent)
    parser.add_argument("--descriptor-version", choices=("v1", "v2"), default="v1")
    parser.add_argument("--require-complete-sessions", action="store_true")
    parser.add_argument("--q8-quality", type=Path)
    parser.add_argument("reports", nargs="+", type=Path)
    args = parser.parse_args()
    queries = json.loads(args.queries.read_text(encoding="utf-8"))
    workloads, protocol = load_descriptors(args.descriptor_directory, args.descriptor_version)
    reports = [json.loads(path.read_text(encoding="utf-8")) for path in args.reports]
    if any(report.get("profile_id") == "coreml-weight-only-q8-experimental-v1" for report in reports):
        if args.q8_quality is None:
            raise ValidationError("Q8 reports require --q8-quality provider evidence")
        quality = json.loads(args.q8_quality.read_text(encoding="utf-8"))
        required = protocol["q8_prerequisite"]
        if (
            quality.get("query_count") != 42
            or quality.get("passed") is not True
            or quality.get("median_cosine", 0) < required["median_cosine_minimum"]
            or quality.get("mean_top10_overlap", 0) < required["mean_top10_overlap_minimum"]
            or quality.get("minimum_top10_overlap", 0) < required["minimum_query_top10_overlap"]
        ):
            raise ValidationError("Q8 provider-quality prerequisite did not pass")
    aggregate = validate_collection(
        reports, queries, workloads, protocol, args.require_complete_sessions
    )
    print(json.dumps({"configurations": aggregate, "valid": True}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValidationError as error:
        raise SystemExit(f"invalid benchmark evidence: {error}") from error
