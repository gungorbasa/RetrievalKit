#!/usr/bin/env python3
"""Independent validator for Phase 7 gate metadata and result artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_ROOT = Path(__file__).resolve().parent
STATIC_FILES = (
    "baselines-v1.json",
    "contract-v1.json",
    "fixtures/expected-observation-v1.json",
    "fixtures/graph-quality-smoke-v1.json",
    "gate-registry-v1.json",
    "result-schema-v1.json",
)
TIERS = ("pull_request", "scheduled_full", "release")
STATUSES = {"passed", "failed", "not_provisioned"}


class ValidationError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def canonical_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def load_canonical(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    value = json.loads(raw)
    require(isinstance(value, dict), f"{path}: expected one JSON object")
    require(raw == canonical_bytes(value), f"{path}: JSON is not canonical")
    return value


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"{path}: expected one JSON object")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def artifact_set(entries: list[dict[str, str]]) -> str:
    preimage = "".join(
        f"{row['path']}\t{row['sha256']}\n" for row in sorted(entries, key=lambda row: row["path"])
    )
    return hashlib.sha256(preimage.encode()).hexdigest()


def validate_static(repo: Path) -> dict[str, Any]:
    contract = load_canonical(BENCHMARK_ROOT / "contract-v1.json")
    baseline = load_canonical(BENCHMARK_ROOT / "baselines-v1.json")
    registry = load_canonical(BENCHMARK_ROOT / "gate-registry-v1.json")
    schema = load_canonical(BENCHMARK_ROOT / "result-schema-v1.json")
    fixture = load_canonical(BENCHMARK_ROOT / "fixtures/graph-quality-smoke-v1.json")
    observation = load_canonical(BENCHMARK_ROOT / "fixtures/expected-observation-v1.json")
    manifest = load_canonical(BENCHMARK_ROOT / "manifest-v1.json")

    require(contract["artifact_inventory"] == list(STATIC_FILES), "contract inventory changed")
    require(manifest["inventory"] == list(STATIC_FILES), "manifest inventory changed")
    entries = [
        {"path": name, "sha256": sha256_file(BENCHMARK_ROOT / name)}
        for name in STATIC_FILES
    ]
    require(manifest["files"] == entries, "static file hashes differ")
    require(manifest["canonical_artifact_set_sha256"] == artifact_set(entries), "static artifact-set hash differs")
    require(manifest["contract_sha256"] == sha256_file(BENCHMARK_ROOT / "contract-v1.json"), "manifest contract hash differs")
    require(manifest["registry_sha256"] == sha256_file(BENCHMARK_ROOT / "gate-registry-v1.json"), "manifest registry hash differs")
    require(manifest["baseline_sha256"] == sha256_file(BENCHMARK_ROOT / "baselines-v1.json"), "manifest baseline hash differs")
    require(manifest["fixture_sha256"] == sha256_file(BENCHMARK_ROOT / "fixtures/graph-quality-smoke-v1.json"), "manifest fixture hash differs")

    frozen = baseline["frozen_inputs"]
    frozen_paths = {
        "phase5_artifact_set_sha256": repo / "benchmarks/external-reference/artifacts/mac-comparison-v1/manifest.json",
        "phase6_contract_sha256": repo / "benchmarks/publication/contract-v1.json",
        "phase6_manifest_sha256": repo / "benchmarks/publication/artifacts/phase6-publication-v1/manifest.json",
        "phase6_validator_sha256": repo / "benchmarks/publication/validate_publication.py",
    }
    for key, path in frozen_paths.items():
        if key == "phase5_artifact_set_sha256":
            actual = load_json(path)["artifact_set_sha256"]
        else:
            actual = sha256_file(path)
        require(frozen[key] == actual, f"frozen identity changed: {key}")
    phase6_manifest = load_json(frozen_paths["phase6_manifest_sha256"])
    require(phase6_manifest["canonical_artifact_set_sha256"] == frozen["phase6_artifact_set_sha256"], "Phase 6 artifact-set identity changed")
    require(phase6_manifest["input_artifacts"]["phase4b_supported"] == frozen["phase4b_supported_artifact_set_sha256"], "Phase 4b supported identity changed")
    require(phase6_manifest["input_artifacts"]["phase4b_graph_free"] == frozen["phase4b_graph_free_artifact_set_sha256"], "Phase 4b graph-free identity changed")

    require(registry["contract_id"] == contract["contract_id"], "registry contract reference differs")
    require(registry["baseline_id"] == baseline["baseline_id"], "registry baseline reference differs")
    gate_ids = [row["gate_id"] for row in registry["gates"]]
    require(len(gate_ids) == len(set(gate_ids)), "duplicate gate ID")
    require(all(row["tier"] in TIERS for row in registry["gates"]), "unknown gate tier")
    required_gate_fields = {
        "baseline", "claim_impact", "evidence_paths", "failure_severity", "gate_id",
        "metric", "owner", "platform_requirements", "rebaseline_rules", "required_inputs",
        "threshold", "tier",
    }
    for row in registry["gates"]:
        require(set(row) == required_gate_fields, f"registry field set differs: {row.get('gate_id')}")
        require(row["threshold"]["operator"] in {"eq", "gte", "lte"}, "unknown threshold operator")
    by_id = {row["gate_id"]: row for row in registry["gates"]}
    require(by_id["P7-FULL-NDCG10"]["threshold"]["value"] == baseline["full_quality"]["ndcg_at_10_minimum"], "NDCG baseline mismatch")
    require(by_id["P7-RELEASE-GRAPH-FREE-RATIO"]["threshold"]["value"] == baseline["performance"]["graph_free_candidate_over_baseline_median_p95_ratio_maximum"], "graph-free baseline mismatch")
    require(by_id["P7-RELEASE-MEMORY"]["threshold"]["value"] == baseline["performance"]["peak_process_memory_bytes_maximum"], "memory baseline mismatch")

    encoded_fixture = json.dumps(fixture, sort_keys=True).lower()
    require(
        all(marker not in encoded_fixture for marker in ("hotpotqa", "iphone", "phase4b")),
        "smoke fixture copied excluded evidence",
    )
    require(fixture["license"]["external_source"] is False, "smoke fixture must be synthetic")
    require(len(fixture["records"]) >= 6, "smoke fixture is incomplete")
    require(set(fixture["quality"]["qrels"]["q-evidence"]) == {"beta", "gamma"}, "smoke judgments changed")
    require(observation["candidate_recall"] == observation["candidate_complete_evidence"] == 1.0, "smoke candidate metrics changed")
    require(observation["deleted_hits"] == observation["outdated_hits"] == 0, "smoke exclusions changed")
    require(observation["unexpected_empty_scope_count"] == 0, "smoke empty-scope result changed")
    require(set(schema["status_values"]) == STATUSES, "result status schema differs")
    registry_text = json.dumps(registry, sort_keys=True).lower()
    require("usearch performance winner" not in registry_text, "USearch winner policy entered the registry")

    validate_workflows(repo)
    return {
        "artifact_set_sha256": manifest["canonical_artifact_set_sha256"],
        "fixture_sha256": manifest["fixture_sha256"],
        "gate_count": len(gate_ids),
        "registry_sha256": manifest["registry_sha256"],
    }


def validate_workflows(repo: Path) -> None:
    workflow_paths = [
        repo / ".github/workflows/ci.yml",
        repo / ".github/workflows/regression-full.yml",
        repo / ".github/workflows/release-qualification.yml",
    ]
    action_pattern = re.compile(r"uses:\s+[^\s@]+@([0-9a-f]{40})(?:\s|$)")
    forbidden_device_commands = ("xcrun devicectl", "ios-deploy", "simctl install", "simctl launch")
    for path in workflow_paths:
        text = path.read_text(encoding="utf-8")
        require("permissions:\n  contents: read" in text, f"least-privilege permissions missing: {path.name}")
        require("timeout-minutes:" in text, f"job timeout missing: {path.name}")
        require("pull_request_target" not in text, f"unsafe PR trigger: {path.name}")
        for line in text.splitlines():
            if "uses:" in line:
                require(action_pattern.search(line) is not None, f"unpinned action: {path.name}: {line.strip()}")
    release_text = workflow_paths[2].read_text(encoding="utf-8").lower()
    require("100k" not in release_text, "release workflow exposes a 100K option")
    require(not any(command in release_text for command in forbidden_device_commands), "release workflow runs a device command")


def threshold_passes(actual: Any, threshold: dict[str, Any]) -> bool:
    operator = threshold["operator"]
    expected = threshold["value"]
    if operator == "eq":
        return actual == expected
    if operator == "gte":
        return float(actual) >= float(expected)
    if operator == "lte":
        return float(actual) <= float(expected)
    raise ValidationError(f"unknown operator: {operator}")


def expected_summary(result: dict[str, Any]) -> str:
    rows = [row for row in result["gates"] if row["status"] != "passed"]
    lines = [
        "# Phase 7 Regression Gate Summary", "", f"Tier: `{result['tier']}`", "",
        f"Overall status: `{result['overall_status']}`", "",
    ]
    if not rows:
        return "\n".join(lines + ["All required gates passed.", ""])
    for row in rows:
        lines.extend([
            f"## {row['gate_id']}", "", row["summary"], "",
            f"- Metric: `{row['metric']}`",
            f"- Expected: `{json.dumps(row['expected'], sort_keys=True)}`",
            f"- Actual: `{json.dumps(row['actual'], sort_keys=True)}`",
            f"- Baseline: `{row['baseline_id']}`",
            f"- Claim or guarantee: {row['claim_impact']}",
            f"- Blocking tier: `{row['blocking_tier']}`",
            f"- Reproduce: `{row['reproduction_command']}`", "",
        ])
    return "\n".join(lines)


def validate_result(root: Path) -> dict[str, Any]:
    result = load_canonical(root / "result.json")
    schema = load_canonical(BENCHMARK_ROOT / "result-schema-v1.json")
    registry = load_canonical(BENCHMARK_ROOT / "gate-registry-v1.json")
    baseline = load_canonical(BENCHMARK_ROOT / "baselines-v1.json")
    require(set(result) == set(schema["required_result_fields"]), "result field set differs")
    require(result["tier"] in TIERS, "result tier is invalid")
    require(result["overall_status"] in STATUSES, "result status is invalid")
    require(result["contract"]["sha256"] == sha256_file(BENCHMARK_ROOT / "contract-v1.json"), "result contract identity differs")
    require(result["registry"]["sha256"] == sha256_file(BENCHMARK_ROOT / "gate-registry-v1.json"), "result registry identity differs")
    require(result["baseline"]["sha256"] == sha256_file(BENCHMARK_ROOT / "baselines-v1.json"), "result baseline identity differs")
    require(result["fixture"]["sha256"] == sha256_file(BENCHMARK_ROOT / "fixtures/graph-quality-smoke-v1.json"), "result fixture identity differs")
    if result["tier"] == "release":
        for qualifier in ("device_identifier", "os", "toolchain", "source_revision", "sample_count"):
            require(bool(result["platform"].get(qualifier)), f"release platform qualifier missing: {qualifier}")
    expected_gates = [row for row in registry["gates"] if row["tier"] == result["tier"]]
    require([row["gate_id"] for row in result["gates"]] == [row["gate_id"] for row in expected_gates], "missing, extra, hidden, or reordered gate result")
    statuses = set()
    for actual, gate in zip(result["gates"], expected_gates, strict=True):
        require(set(actual) == set(schema["required_gate_fields"]), f"gate result fields differ: {gate['gate_id']}")
        require(actual["expected"] == gate["threshold"], f"threshold differs: {gate['gate_id']}")
        require(actual["baseline_id"] == baseline["baseline_id"], f"baseline differs: {gate['gate_id']}")
        require(actual["metric"] == gate["metric"], f"metric differs: {gate['gate_id']}")
        require(actual["claim_impact"] == gate["claim_impact"], f"claim impact differs: {gate['gate_id']}")
        require(actual["evidence_paths"] == gate["evidence_paths"], f"evidence paths differ: {gate['gate_id']}")
        require(actual["blocking_tier"] == gate["tier"], f"blocking tier differs: {gate['gate_id']}")
        require(actual["status"] in STATUSES, f"invalid gate status: {gate['gate_id']}")
        if actual["status"] == "passed":
            require(threshold_passes(actual["actual"], gate["threshold"]), f"false pass: {gate['gate_id']}")
        elif actual["status"] == "not_provisioned":
            require(result["tier"] != "pull_request", "PR gate cannot skip")
            require(actual["actual"] is None, "not-provisioned gate has an actual value")
        else:
            try:
                passes = threshold_passes(actual["actual"], gate["threshold"])
            except (TypeError, ValueError):
                passes = False
            require(not passes, f"passing metric mislabeled failed: {gate['gate_id']}")
        statuses.add(actual["status"])
    expected_overall = "failed" if "failed" in statuses else "not_provisioned" if "not_provisioned" in statuses else "passed"
    require(result["overall_status"] == expected_overall, "overall status differs")
    require((root / "failure-summary.md").read_text(encoding="utf-8") == expected_summary(result), "human failure summary differs")
    return {"overall_status": expected_overall, "tier": result["tier"]}


def compare_roots(first: Path, second: Path) -> None:
    first_files = {path.relative_to(first).as_posix(): path.read_bytes() for path in first.rglob("*") if path.is_file()}
    second_files = {path.relative_to(second).as_posix(): path.read_bytes() for path in second.rglob("*") if path.is_file()}
    require(first_files == second_files, "two result roots are not byte-identical")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--result-root", type=Path)
    parser.add_argument("--compare-root", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = validate_static(args.repo.resolve())
        if args.result_root:
            result["result"] = validate_result(args.result_root.resolve())
        if args.compare_root:
            require(args.result_root is not None, "--compare-root requires --result-root")
            compare_roots(args.result_root.resolve(), args.compare_root.resolve())
            result["two_root_byte_identity"] = True
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, ValidationError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"result": "PASS", **result}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
