#!/usr/bin/env python3
"""Run deterministic Phase 7 regression gates and emit closed result artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any, cast

ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_ROOT = Path(__file__).resolve().parent
CONTRACT_PATH = BENCHMARK_ROOT / "contract-v1.json"
REGISTRY_PATH = BENCHMARK_ROOT / "gate-registry-v1.json"
BASELINE_PATH = BENCHMARK_ROOT / "baselines-v1.json"
FIXTURE_PATH = BENCHMARK_ROOT / "fixtures/graph-quality-smoke-v1.json"
PR_OBSERVATION_PATH = BENCHMARK_ROOT / "fixtures/expected-observation-v1.json"


class GateError(RuntimeError):
    pass


def canonical_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise GateError(f"cannot read JSON '{path}': {error}") from error
    if not isinstance(value, dict):
        raise GateError(f"'{path}' must contain one JSON object")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def git_revision(repo: Path) -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def pr_metric(metric: str, observed: dict[str, Any]) -> Any:
    if metric == "exclusion_violation_count":
        return (
            int(observed["deleted_hits"])
            + int(observed["outdated_hits"])
            + int(not observed["dimension_mismatch_rejected"])
        )
    if metric == "graph_scope_violation_count":
        return (
            int(observed["graph_selection_mismatches"])
            + int(observed["unexpected_empty_scope_count"])
            + int(observed["invalid_scope_rejections"] != 1)
        )
    if metric == "quality_smoke_minimum":
        return min(
            float(observed[name])
            for name in (
                "ndcg_at_3",
                "recall_at_3",
                "complete_evidence_recall_at_3",
                "candidate_recall",
                "candidate_complete_evidence",
            )
        )
    if metric == "graph_free_activity_count":
        return sum(
            int(observed[name])
            for name in (
                "graph_queries",
                "graph_nodes_visited",
                "graph_edges_traversed",
                "graph_candidates_projected",
            )
        )
    if metric == "artifact_integrity_violation_count":
        return int(not observed["artifact_inventory_valid"]) + int(
            not observed["schema_valid"]
        )
    return observed[metric]


def provisioned(gate: dict[str, Any], inputs: dict[str, Any]) -> bool:
    declared = inputs.get("provisioned", [])
    return isinstance(declared, list) and all(
        required in declared for required in gate["required_inputs"]
    )


def controlled_metric(
    gate: dict[str, Any],
    observed: dict[str, Any],
    observation: dict[str, Any],
    baseline: dict[str, Any],
) -> Any:
    metric = gate["metric"]
    if metric == "phase6_claim_validation":
        hashes = observation.get("inputs", {}).get("frozen_inputs", {})
        frozen_match = all(
            hashes.get(key) == value
            for key, value in baseline["frozen_inputs"].items()
        )
        return bool(observed[metric]) and frozen_match
    if metric == "claim_policy_violation_count":
        platform = observation.get("platform", {})
        missing = sum(
            int(not platform.get(name))
            for name in (
                "device_identifier",
                "os",
                "toolchain",
                "source_revision",
                "sample_count",
            )
        )
        encoded = json.dumps(observation, sort_keys=True).lower()
        prohibited = int("usearch performance winner" in encoded) + int(
            "graph performance winner" in encoded
        )
        return int(observed[metric]) + missing + prohibited
    if metric == "physical_device_100k_violation_count":
        encoded = json.dumps(observation, sort_keys=True).lower()
        return int(observed[metric]) + int("100k" in encoded)
    return observed[metric]


def compare(actual: Any, threshold: dict[str, Any]) -> bool:
    operator = threshold["operator"]
    expected = threshold["value"]
    if operator == "eq":
        return cast(bool, actual == expected)
    if operator == "gte":
        return float(actual) >= float(expected)
    if operator == "lte":
        return float(actual) <= float(expected)
    raise GateError(f"unknown threshold operator '{operator}'")


def gate_result(
    gate: dict[str, Any], actual: Any, status: str, baseline_id: str
) -> dict[str, Any]:
    expected = gate["threshold"]
    if status == "passed":
        summary = f"{gate['gate_id']} passed: {gate['metric']}={actual!r}."
    elif status == "not_provisioned":
        summary = (
            f"{gate['gate_id']} not provisioned: required inputs "
            f"{', '.join(gate['required_inputs'])}."
        )
    else:
        summary = (
            f"{gate['gate_id']} regressed: expected {expected['operator']} "
            f"{expected['value']!r}, actual {actual!r}."
        )
    return {
        "actual": actual,
        "baseline_id": baseline_id,
        "blocking_tier": gate["tier"],
        "claim_impact": gate["claim_impact"],
        "evidence_paths": gate["evidence_paths"],
        "expected": expected,
        "gate_id": gate["gate_id"],
        "metric": gate["metric"],
        "reproduction_command": reproduction_command(gate["tier"]),
        "status": status,
        "summary": summary,
    }


def reproduction_command(tier: str) -> str:
    if tier == "pull_request":
        return "scripts/benchmarks/run-phase7-pr.sh"
    if tier == "scheduled_full":
        return (
            "python3 benchmarks/regression/run_gates.py --tier scheduled_full "
            "--observation <provisioned-observation.json> --output <fresh-root>"
        )
    return (
        "python3 benchmarks/regression/run_gates.py --tier release "
        "--observation <authorized-release-observation.json> --output <fresh-root>"
    )


def failure_summary(result: dict[str, Any]) -> str:
    rows = [gate for gate in result["gates"] if gate["status"] != "passed"]
    lines = [
        "# Phase 7 Regression Gate Summary",
        "",
        f"Tier: `{result['tier']}`",
        "",
        f"Overall status: `{result['overall_status']}`",
        "",
    ]
    if not rows:
        lines.extend(["All required gates passed.", ""])
        return "\n".join(lines)
    for row in rows:
        lines.extend(
            [
                f"## {row['gate_id']}",
                "",
                row["summary"],
                "",
                f"- Metric: `{row['metric']}`",
                f"- Expected: `{json.dumps(row['expected'], sort_keys=True)}`",
                f"- Actual: `{json.dumps(row['actual'], sort_keys=True)}`",
                f"- Baseline: `{row['baseline_id']}`",
                f"- Claim or guarantee: {row['claim_impact']}",
                f"- Blocking tier: `{row['blocking_tier']}`",
                f"- Reproduce: `{row['reproduction_command']}`",
                "",
            ]
        )
    return "\n".join(lines)


def evaluate(args: argparse.Namespace) -> dict[str, Any]:
    contract = load_json(CONTRACT_PATH)
    registry = load_json(REGISTRY_PATH)
    baseline = load_json(BASELINE_PATH)
    fixture = load_json(FIXTURE_PATH)
    tier_gates = [gate for gate in registry["gates"] if gate["tier"] == args.tier]
    if not tier_gates:
        raise GateError(f"registry has no gates for tier '{args.tier}'")

    observation_path = args.observation
    if args.tier == "pull_request" and observation_path is None:
        observation_path = PR_OBSERVATION_PATH
    observation = load_json(observation_path) if observation_path else None
    platform = observation.get("platform", {}) if observation else {}
    inputs = observation.get("inputs", {}) if observation else {}
    observed_metrics = observation.get("metrics", observation) if observation else {}

    gate_rows = []
    if observation is None:
        gate_rows = [
            gate_result(gate, None, "not_provisioned", baseline["baseline_id"])
            for gate in tier_gates
        ]
    else:
        for gate in tier_gates:
            try:
                if args.tier != "pull_request" and not provisioned(gate, inputs):
                    raise KeyError("required inputs are not fully provisioned")
                actual = (
                    pr_metric(gate["metric"], observed_metrics)
                    if args.tier == "pull_request"
                    else controlled_metric(gate, observed_metrics, observation, baseline)
                )
            except (KeyError, TypeError, ValueError) as error:
                actual = f"missing_or_invalid_metric:{error}"
                status = "failed"
            else:
                status = "passed" if compare(actual, gate["threshold"]) else "failed"
            gate_rows.append(
                gate_result(gate, actual, status, baseline["baseline_id"])
            )

    statuses = {row["status"] for row in gate_rows}
    overall = (
        "failed"
        if "failed" in statuses
        else "not_provisioned"
        if "not_provisioned" in statuses
        else "passed"
    )
    revision = args.source_revision or git_revision(args.repo)
    return {
        "artifact_type": "phase7_regression_result",
        "baseline": {
            "id": baseline["baseline_id"],
            "sha256": sha256_file(BASELINE_PATH),
        },
        "contract": {
            "id": contract["contract_id"],
            "sha256": sha256_file(CONTRACT_PATH),
        },
        "fixture": {
            "id": fixture["fixture_id"],
            "sha256": sha256_file(FIXTURE_PATH),
        },
        "gates": gate_rows,
        "inputs": inputs,
        "overall_status": overall,
        "platform": platform,
        "registry": {
            "id": registry["registry_id"],
            "sha256": sha256_file(REGISTRY_PATH),
        },
        "schema_version": 1,
        "source_revision": revision,
        "tier": args.tier,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--tier", choices=("pull_request", "scheduled_full", "release"), required=True
    )
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--observation", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = evaluate(args)
        args.output.mkdir(parents=True, exist_ok=False)
        (args.output / "result.json").write_bytes(canonical_bytes(result))
        (args.output / "failure-summary.md").write_text(
            failure_summary(result), encoding="utf-8", newline="\n"
        )
    except (GateError, OSError, subprocess.CalledProcessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(canonical_bytes(result).decode(), end="")
    return {"passed": 0, "failed": 1, "not_provisioned": 2}[result["overall_status"]]


if __name__ == "__main__":
    raise SystemExit(main())
