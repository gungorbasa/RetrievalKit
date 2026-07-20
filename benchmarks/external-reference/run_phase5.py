#!/usr/bin/env python3
"""Run isolated Phase 5 external comparisons and emit a closed artifact root."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

from phase5_common import (
    canonical,
    canonical_file,
    canonical_jsonl,
    generate_workload,
    recall_at_k,
    reset_directory,
    runtime_environment,
    sha256_bytes,
    sha256_file,
    source_revision,
    stable_chunk_id,
)

ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_ROOT = Path(__file__).resolve().parent
CONTRACT_PATH = BENCHMARK_ROOT / "contract-v1.json"
FEATURE_PARITY_PATH = BENCHMARK_ROOT / "feature-parity-v1.json"
ARTIFACT_FILES = [
    "config.json",
    "environment.json",
    "feature-parity.json",
    "input-manifests.json",
    "raw-measurements.jsonl",
    "raw-results.jsonl",
    "failures.jsonl",
    "summary.json",
    "checksums.json",
    "manifest.json",
]
PREIMAGE_FILES = ARTIFACT_FILES[:8]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--keep-scratch", action="store_true")
    parser.add_argument("--system", action="append", dest="systems")
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError(f"'{path}' must contain a JSON object")
    return value


def validate_config(config: dict[str, Any], contract: dict[str, Any]) -> None:
    expected = {
        "config_id",
        "measurement",
        "profile",
        "retain_generated_inputs",
        "schema_version",
        "split",
        "systems",
        "workloads",
    }
    if set(config) != expected:
        raise ValueError(f"config fields must be exactly {sorted(expected)}")
    if config["schema_version"] != 1:
        raise ValueError("unsupported config schema")
    if set(config["measurement"]) != {"samples", "warmups"}:
        raise ValueError("measurement fields must be samples and warmups")
    if int(config["measurement"]["samples"]) <= 0:
        raise ValueError("samples must be positive")
    if int(config["measurement"]["warmups"]) < 0:
        raise ValueError("warmups cannot be negative")
    declared_systems = {value["system_id"] for value in contract["systems"]}
    if not set(config["systems"]).issubset(declared_systems):
        raise ValueError("config contains an undeclared system")
    for workload in config["workloads"]:
        generate_workload(workload)


def run_worker(
    python: Path,
    system_id: str,
    workload: dict[str, Any],
    measurement: dict[str, Any],
    contract: dict[str, Any],
    scratch_root: Path,
    source_commit: str,
) -> dict[str, Any]:
    worker_root = scratch_root / system_id / str(workload["workload_id"])
    worker_root.mkdir(parents=True, exist_ok=True)
    request_path = worker_root / "request.json"
    output_path = worker_root / "result.json"
    request = {
        "contract": contract,
        "measurement": measurement,
        "scratch_root": str(worker_root / "state"),
        "source_revision": source_commit,
        "system_id": system_id,
        "workload": workload,
    }
    canonical_file(request_path, request)
    completed = subprocess.run(
        [
            str(python),
            str(BENCHMARK_ROOT / "adapter_worker.py"),
            "--request",
            str(request_path),
            "--output",
            str(output_path),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if not output_path.exists():
        return {
            "artifact_type": "phase5_adapter_result",
            "build_ns": None,
            "failure": {
                "exception_type": "WorkerProcessFailure",
                "message": (
                    f"worker exited {completed.returncode} without an artifact; "
                    f"stdout={completed.stdout!r}; stderr={completed.stderr!r}"
                ),
                "stage": "worker_process",
                "traceback": None,
            },
            "input_manifest": generate_workload(workload).input_manifest,
            "load_ns": None,
            "operations": [],
            "peak_rss_bytes": None,
            "persistence": {"components": [], "total_bytes": 0},
            "save_ns": None,
            "schema_version": 1,
            "status": "failure",
            "system_id": system_id,
            "system_version": None,
            "workload_id": workload["workload_id"],
        }
    result = load_json(output_path)
    result["worker_exit_code"] = completed.returncode
    result["worker_stderr"] = completed.stderr.strip() or None
    result["worker_stdout"] = completed.stdout.strip() or None
    return result


def operation_map(adapter: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {str(value["operation_id"]): value for value in adapter["operations"]}


def results_by_query(operation: dict[str, Any]) -> dict[str, list[str]]:
    return {
        str(value["query_id"]): [str(item) for item in value["result_ids"]]
        for value in operation["results"]
    }


def summarize(config: dict[str, Any], adapters: list[dict[str, Any]]) -> dict[str, Any]:
    by_key = {
        (str(value["system_id"]), str(value["workload_id"])): value
        for value in adapters
    }
    rows = []
    all_gates = []
    for workload in config["workloads"]:
        workload_id = str(workload["workload_id"])
        oracle = by_key.get(("numpy_f32_oracle", workload_id))
        oracle_operations = operation_map(oracle) if oracle else {}
        oracle_unfiltered = (
            results_by_query(oracle_operations["exact_unfiltered"])
            if "exact_unfiltered" in oracle_operations
            else {}
        )
        oracle_filtered = (
            results_by_query(oracle_operations["exact_filtered"])
            if "exact_filtered" in oracle_operations
            else {}
        )
        for system_id in config["systems"]:
            adapter = by_key.get((str(system_id), workload_id))
            if adapter is None:
                rows.append(
                    {
                        "acceptance": "failed",
                        "failure": "missing adapter result",
                        "system_id": system_id,
                        "workload_id": workload_id,
                    }
                )
                all_gates.append(False)
                continue
            operations = operation_map(adapter)
            gates: dict[str, Any] = {}
            if adapter["status"] == "success" and system_id in {
                "vectorkit_f32_exact",
                "sqlite_vec_exact",
            }:
                for operation_id, oracle_rows in [
                    ("exact_unfiltered", oracle_unfiltered),
                    ("exact_filtered", oracle_filtered),
                ]:
                    actual = results_by_query(operations[operation_id])
                    gates[f"{operation_id}_identity_equal"] = actual == oracle_rows
            if adapter["status"] == "success" and system_id == "usearch_hnsw":
                actual = results_by_query(operations["ann_unfiltered"])
                recalls = [
                    recall_at_k(ids, oracle_unfiltered[query_id])
                    for query_id, ids in sorted(actual.items())
                ]
                mean_recall = sum(recalls) / len(recalls) if recalls else 0.0
                gates["mean_recall_at_10"] = mean_recall
                gates["recall_gate_passed"] = mean_recall >= 0.99
                gates["filtered_ann_classification"] = (
                    operations["ann_filtered"].get("unsupported_reason") is not None
                )
            if adapter["status"] == "success" and system_id in {
                "vectorkit_graph_app",
                "sqlite_custom_graph_app",
            }:
                application = operations["graph_scoped_application"]
                exact_rows = {
                    str(value["query_id"]): value["result_ids"]
                    for value in application["results"]
                    if value["operation_id"] == "graph_scoped_exact"
                }
                data = generate_workload(workload)
                expected = {
                    spec.query_id: [stable_chunk_id(spec.target_ordinal)]
                    for spec in data.query_specs
                }
                gates["graph_scoped_exact_identity_equal"] = exact_rows == expected
                replay = {
                    (str(value["operation_id"]), str(value["query_id"])): value[
                        "result_ids"
                    ]
                    for value in application["replay_results"]
                }
                original = {
                    (str(value["operation_id"]), str(value["query_id"])): value[
                        "result_ids"
                    ]
                    for value in application["results"]
                }
                gates["reload_identity_equal"] = original == replay
            gate_values = [value for value in gates.values() if isinstance(value, bool)]
            accepted = adapter["status"] == "success" and all(gate_values)
            if system_id == "numpy_f32_oracle":
                accepted = adapter["status"] == "success"
            all_gates.append(accepted)
            rows.append(
                {
                    "acceptance": "passed" if accepted else "failed",
                    "build_ns": adapter["build_ns"],
                    "failure": adapter["failure"],
                    "gates": gates,
                    "load_ns": adapter["load_ns"],
                    "operations": [
                        {
                            "distribution": value["distribution"],
                            "operation_id": value["operation_id"],
                            "timed": value["timed"],
                            "unsupported_reason": value.get("unsupported_reason"),
                        }
                        for value in adapter["operations"]
                    ],
                    "peak_rss_bytes": adapter["peak_rss_bytes"],
                    "persistence_bytes": adapter["persistence"]["total_bytes"],
                    "save_ns": adapter["save_ns"],
                    "status": adapter["status"],
                    "system_id": system_id,
                    "system_version": adapter["system_version"],
                    "workload_id": workload_id,
                }
            )
    return {
        "artifact_type": "phase5_summary",
        "config_id": config["config_id"],
        "limitations": [
            "local Mac results are not physical-device or marketing evidence",
            "ANN latency is comparable only when Recall@10 is at least 0.99",
            "USearch Python predicate filtering is unsupported",
            "SQLite FTS5 hybrid semantics are non-equivalent to VectorKit hybrid",
            "peak RSS is process-level rather than isolated component memory",
        ],
        "overall_acceptance": "passed" if all(all_gates) else "failed",
        "rows": rows,
        "schema_version": 1,
    }


def flatten_artifacts(
    adapters: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    measurements = []
    results = []
    failures = []
    for adapter in adapters:
        base = {
            "system_id": adapter["system_id"],
            "system_version": adapter["system_version"],
            "workload_id": adapter["workload_id"],
        }
        if adapter["failure"] is not None:
            failures.append(
                {
                    **base,
                    "classification": "adapter_failure",
                    **adapter["failure"],
                }
            )
        for operation in adapter["operations"]:
            if operation.get("unsupported_reason"):
                failures.append(
                    {
                        **base,
                        "classification": "unsupported_operation",
                        "exception_type": None,
                        "message": operation["unsupported_reason"],
                        "operation_id": operation["operation_id"],
                        "stage": "capability_mapping",
                        "traceback": None,
                    }
                )
            for sample in operation["samples"]:
                measurements.append({**base, **sample})
            for value in operation["results"]:
                results.append({**base, "result_kind": "measured", **value})
            for value in operation["replay_results"]:
                results.append({**base, "result_kind": "replay", **value})
    measurements.sort(
        key=lambda value: (
            value["system_id"],
            value["workload_id"],
            value["operation_id"],
            value["stage"],
            value["sample_index"],
            value["query_id"],
        )
    )
    results.sort(
        key=lambda value: (
            value["system_id"],
            value["workload_id"],
            value["operation_id"],
            value["query_id"],
            value["result_kind"],
        )
    )
    failures.sort(
        key=lambda value: (
            value["system_id"],
            value["workload_id"],
            value.get("operation_id", ""),
            value["classification"],
        )
    )
    return measurements, results, failures


def emit_artifacts(
    output: Path,
    config: dict[str, Any],
    environment: dict[str, Any],
    feature_parity: dict[str, Any],
    adapters: list[dict[str, Any]],
    contract: dict[str, Any],
) -> dict[str, Any]:
    canonical_file(output / "config.json", config)
    canonical_file(output / "environment.json", environment)
    canonical_file(output / "feature-parity.json", feature_parity)
    input_manifests = sorted(
        {value["workload_id"]: value["input_manifest"] for value in adapters}.values(),
        key=lambda value: value["workload_id"],
    )
    canonical_file(
        output / "input-manifests.json",
        {
            "artifact_type": "phase5_input_manifests",
            "manifests": input_manifests,
            "schema_version": 1,
        },
    )
    measurements, results, failures = flatten_artifacts(adapters)
    canonical_jsonl(output / "raw-measurements.jsonl", measurements)
    canonical_jsonl(output / "raw-results.jsonl", results)
    canonical_jsonl(output / "failures.jsonl", failures)
    summary = summarize(config, adapters)
    canonical_file(output / "summary.json", summary)
    checksum_entries = [
        {"path": name, "sha256": sha256_file(output / name)} for name in PREIMAGE_FILES
    ]
    checksums = {
        "algorithm": "sha256",
        "artifact_type": "phase5_checksums",
        "entries": checksum_entries,
        "preimage_sha256": sha256_bytes(canonical(checksum_entries)),
        "schema_version": 1,
    }
    canonical_file(output / "checksums.json", checksums)
    manifest_entries = [
        {"path": name, "sha256": sha256_file(output / name)}
        for name in ARTIFACT_FILES[:-1]
    ]
    manifest = {
        "artifact_set_sha256": sha256_bytes(canonical(manifest_entries)),
        "artifact_type": "phase5_manifest",
        "config_id": config["config_id"],
        "config_sha256": sha256_file(output / "config.json"),
        "contract_id": contract["contract_id"],
        "contract_sha256": sha256_file(CONTRACT_PATH),
        "entries": manifest_entries,
        "feature_parity_sha256": sha256_file(output / "feature-parity.json"),
        "file_count_excluding_manifest": len(manifest_entries),
        "physical_device_execution": False,
        "schema_version": 1,
        "source_revision": environment["repository_revision"],
        "supported_v1_capacity_changed": False,
    }
    canonical_file(output / "manifest.json", manifest)
    return manifest


def main() -> int:
    arguments = parse_arguments()
    config = load_json(arguments.config)
    contract = load_json(CONTRACT_PATH)
    feature_parity = load_json(FEATURE_PARITY_PATH)
    validate_config(config, contract)
    systems = arguments.systems or list(config["systems"])
    unknown = set(systems).difference(config["systems"])
    if unknown:
        raise ValueError(f"requested systems are not in config: {sorted(unknown)}")
    if systems != list(config["systems"]):
        config = {**config, "systems": systems}
    output = arguments.output.resolve()
    reset_directory(output)
    scratch = ROOT / "target" / "phase5-external-scratch" / config["config_id"]
    reset_directory(scratch)
    environment = runtime_environment(ROOT)
    benchmark_python = arguments.python.absolute()
    environment["benchmark_python"] = str(benchmark_python)
    environment["config_profile"] = config["profile"]
    environment["embedding_included"] = False
    environment["source_tree_dirty_before_run"] = bool(
        subprocess.run(
            ["git", "status", "--short"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )
    commit = source_revision(ROOT)
    adapters = []
    for workload in config["workloads"]:
        for system_id in systems:
            adapters.append(
                run_worker(
                    benchmark_python,
                    system_id,
                    workload,
                    config["measurement"],
                    contract,
                    scratch,
                    commit,
                )
            )
    manifest = emit_artifacts(
        output, config, environment, feature_parity, adapters, contract
    )
    if not arguments.keep_scratch:
        shutil.rmtree(scratch)
    summary = load_json(output / "summary.json")
    print(
        json.dumps(
            {
                "artifact_set_sha256": manifest["artifact_set_sha256"],
                "output": str(output),
                "overall_acceptance": summary["overall_acceptance"],
            },
            sort_keys=True,
        )
    )
    return 0 if summary["overall_acceptance"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
