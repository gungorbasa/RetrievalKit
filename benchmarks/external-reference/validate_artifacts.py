#!/usr/bin/env python3
"""Independent Phase 5 artifact validator.

This module intentionally does not import the runner, workers, or shared helper.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_ROOT = Path(__file__).resolve().parent
CONTRACT_PATH = BENCHMARK_ROOT / "contract-v1.json"
FEATURE_PARITY_PATH = BENCHMARK_ROOT / "feature-parity-v1.json"
FILES = [
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
PREIMAGE_FILES = FILES[:8]
GENERATOR_ID = "phase5-generator-v1"
CHUNKS_PER_RECORD = 4
TOP_K = 10


class ValidationError(RuntimeError):
    pass


def _trim_fraction(value: str) -> str:
    if "." not in value:
        return value
    trimmed = value.rstrip("0").rstrip(".")
    return "0" if trimmed == "-0" else trimmed


def _scientific_to_plain(mantissa: str, exponent: int) -> str:
    negative = mantissa.startswith("-")
    unsigned = mantissa.removeprefix("-")
    digits = unsigned.replace(".", "")
    decimal = 1 + exponent
    if decimal <= 0:
        plain = f"0.{('0' * -decimal)}{digits}"
    elif decimal >= len(digits):
        plain = f"{digits}{'0' * (decimal - len(digits))}"
    else:
        plain = f"{digits[:decimal]}.{digits[decimal:]}"
    plain = _trim_fraction(plain)
    return f"-{plain}" if negative else plain


def _number(value: int | float) -> str:
    if isinstance(value, int):
        return str(value)
    if not math.isfinite(value):
        raise ValidationError("non-finite JSON number")
    if value == 0:
        return "0"
    encoded = repr(value).replace("E", "e").replace("e+", "e")
    if "e" not in encoded:
        return _trim_fraction(encoded)
    mantissa, exponent_text = encoded.split("e", 1)
    exponent = int(exponent_text)
    if -6 <= exponent <= 20:
        return _scientific_to_plain(mantissa, exponent)
    sign = "-" if exponent < 0 else ""
    return f"{_trim_fraction(mantissa)}e{sign}{abs(exponent)}"


def _encode(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, (int, float)):
        return _number(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    if isinstance(value, list):
        return f"[{','.join(_encode(item) for item in value)}]"
    if isinstance(value, dict):
        entries = sorted(value.items(), key=lambda item: item[0].encode())
        return (
            "{"
            + ",".join(f"{_encode(key)}:{_encode(item)}" for key, item in entries)
            + "}"
        )
    raise ValidationError(f"unsupported JSON type {type(value).__name__}")


def canonical(value: Any) -> bytes:
    return _encode(value).encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stable_chunk_id(value: int) -> str:
    return f"chunk-{value:08d}"


def stable_record_id(value: int) -> str:
    return f"record-{value:08d}"


def load_canonical_json(path: Path) -> Any:
    value = json.loads(path.read_text())
    if path.read_bytes() != canonical(value) + b"\n":
        raise ValidationError(f"non-canonical JSON: {path.name}")
    return value


def load_canonical_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line_number, line in enumerate(path.read_bytes().splitlines(keepends=True), 1):
        if not line.endswith(b"\n"):
            raise ValidationError(f"missing LF in {path.name}:{line_number}")
        value = json.loads(line)
        if line != canonical(value) + b"\n":
            raise ValidationError(f"non-canonical JSONL in {path.name}:{line_number}")
        if not isinstance(value, dict):
            raise ValidationError(f"non-object JSONL row in {path.name}:{line_number}")
        rows.append(value)
    return rows


def require_fields(value: dict[str, Any], expected: set[str], label: str) -> None:
    if set(value) != expected:
        raise ValidationError(
            f"{label} fields differ: expected {sorted(expected)}, got {sorted(value)}"
        )


def regenerate_manifest(
    spec: dict[str, Any],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    active = int(spec["active_chunks"])
    deleted = int(spec["deleted_chunks"])
    dimension = int(spec["dimension"])
    total = active + deleted
    rng = np.random.Generator(np.random.PCG64(int(spec["seed"])))
    vectors = rng.standard_normal((total, dimension), dtype=np.float32)
    norms = np.linalg.norm(vectors, axis=1, keepdims=True).astype(np.float32)
    vectors /= norms
    targets: list[int] = []
    cursor = int(spec["seed"]) % active
    for _ in range(int(spec["query_count"])):
        while cursor in targets:
            cursor = (cursor + 7919) % active
        targets.append(cursor)
        cursor = (cursor + 7919) % active
    queries = np.ascontiguousarray(vectors[targets], dtype=np.float32)
    query_specs = [
        {
            "query_id": f"q-{index:04d}",
            "query_text": f"topic{target % 32} exact{target}",
            "seed_record_id": stable_record_id(
                ((target // CHUNKS_PER_RECORD) - 1) % (active // CHUNKS_PER_RECORD)
            ),
            "target_chunk_id": stable_chunk_id(target),
            "tenant": f"tenant-{target % 10}",
        }
        for index, target in enumerate(targets)
    ]
    descriptors = {
        "active_chunks": active,
        "chunks_per_record": CHUNKS_PER_RECORD,
        "deleted_chunk_ids": [stable_chunk_id(value) for value in range(active, total)],
        "deleted_chunks": deleted,
        "dimension": dimension,
        "generator_id": GENERATOR_ID,
        "graph_policy": {
            "relationships": ["next", "linked"],
            "next_offset_records": 1,
            "linked_offset_records": 7,
        },
        "metadata_policy": {"category_modulus": 7, "tenant_modulus": 10},
        "query_specs": query_specs,
        "seed": int(spec["seed"]),
        "text_policy": "topic/exact/category/tenant/local-retrieval-graph-evidence-v1",
        "workload_id": spec["workload_id"],
    }
    core = {
        "schema_version": 1,
        "workload_id": spec["workload_id"],
        "generator_id": GENERATOR_ID,
        "active_chunks": active,
        "deleted_chunks": deleted,
        "dimension": dimension,
        "query_count": len(query_specs),
        "vectors_sha256": sha256_bytes(vectors.astype("<f4", copy=False).tobytes()),
        "queries_sha256": sha256_bytes(queries.astype("<f4", copy=False).tobytes()),
        "descriptors_sha256": sha256_bytes(canonical(descriptors)),
        "query_specs_sha256": sha256_bytes(canonical(query_specs)),
    }
    return {**core, "input_identity_sha256": sha256_bytes(canonical(core))}, query_specs


def nearest_rank_distribution(values: list[int]) -> dict[str, Any]:
    if not values:
        raise ValidationError("empty distribution")
    ordered = sorted(values)

    def percentile(value: float) -> int:
        return ordered[max(1, math.ceil(value * len(ordered))) - 1]

    return {
        "mean_ns": sum(ordered) // len(ordered),
        "min_ns": ordered[0],
        "max_ns": ordered[-1],
        "p50_ns": percentile(0.50),
        "p95_ns": percentile(0.95),
        "p99_ns": percentile(0.99),
        "percentile_method": "nearest_rank",
        "sample_count": len(ordered),
    }


def validate_inventory(root: Path) -> None:
    actual = []
    for value in root.iterdir():
        if value.is_symlink():
            raise ValidationError(f"symlink forbidden: {value.name}")
        if not value.is_file():
            raise ValidationError(f"non-file artifact entry: {value.name}")
        actual.append(value.name)
    if sorted(actual) != sorted(FILES):
        raise ValidationError(f"closed inventory differs: {sorted(actual)}")


def validate_checksums(root: Path, manifest: dict[str, Any]) -> None:
    checksums = load_canonical_json(root / "checksums.json")
    require_fields(
        checksums,
        {"algorithm", "artifact_type", "entries", "preimage_sha256", "schema_version"},
        "checksums",
    )
    expected = [
        {"path": name, "sha256": sha256_file(root / name)} for name in PREIMAGE_FILES
    ]
    if checksums["entries"] != expected:
        raise ValidationError("checksums entries differ")
    if checksums["preimage_sha256"] != sha256_bytes(canonical(expected)):
        raise ValidationError("checksums preimage identity differs")
    manifest_entries = [
        {"path": name, "sha256": sha256_file(root / name)} for name in FILES[:-1]
    ]
    if manifest["entries"] != manifest_entries:
        raise ValidationError("manifest entries differ")
    if manifest["artifact_set_sha256"] != sha256_bytes(canonical(manifest_entries)):
        raise ValidationError("artifact-set identity differs")


def result_key(value: dict[str, Any]) -> tuple[str, str, str, str, str]:
    return (
        str(value["system_id"]),
        str(value["workload_id"]),
        str(value["operation_id"]),
        str(value["query_id"]),
        str(value["result_kind"]),
    )


def validate_results(
    config: dict[str, Any], results: list[dict[str, Any]], summary: dict[str, Any]
) -> None:
    expected_order = sorted(results, key=result_key)
    if results != expected_order:
        raise ValidationError("raw results are not in canonical identity order")
    for value in results:
        required = {
            "operation_id",
            "query_id",
            "result_identity_sha256",
            "result_ids",
            "result_kind",
            "system_id",
            "system_version",
            "workload_id",
        }
        if not required.issubset(value):
            raise ValidationError("raw result missing required fields")
        if value["result_identity_sha256"] != sha256_bytes(
            canonical(value["result_ids"])
        ):
            raise ValidationError("raw result identity differs")
        if len(value["result_ids"]) != len(set(value["result_ids"])):
            raise ValidationError("duplicate result identity")
    measured = {
        result_key(value)[:-1]: value
        for value in results
        if value["result_kind"] == "measured"
    }
    replay = {
        result_key(value)[:-1]: value
        for value in results
        if value["result_kind"] == "replay"
    }
    for key, value in replay.items():
        if key not in measured or measured[key]["result_ids"] != value["result_ids"]:
            raise ValidationError(f"reload result differs for {key}")

    query_specs_by_workload: dict[str, list[dict[str, Any]]] = {}
    for workload in config["workloads"]:
        _manifest, query_specs = regenerate_manifest(workload)
        query_specs_by_workload[str(workload["workload_id"])] = query_specs
    for workload_id, query_specs in query_specs_by_workload.items():
        for system_id in ["vectorkit_graph_app", "sqlite_custom_graph_app"]:
            if system_id not in config["systems"]:
                continue
            for query in query_specs:
                key = (
                    system_id,
                    workload_id,
                    "graph_scoped_exact",
                    query["query_id"],
                )
                if key in measured and measured[key]["result_ids"] != [
                    query["target_chunk_id"]
                ]:
                    raise ValidationError(
                        f"graph-scoped exact identity differs for {key}"
                    )

    for row in summary["rows"]:
        gates = row["gates"]
        if row["system_id"] == "usearch_hnsw" and row["status"] == "success":
            workload_id = row["workload_id"]
            recalls = []
            for query in query_specs_by_workload[workload_id]:
                actual = measured[
                    ("usearch_hnsw", workload_id, "ann_unfiltered", query["query_id"])
                ]["result_ids"]
                expected = measured[
                    (
                        "numpy_f32_oracle",
                        workload_id,
                        "exact_unfiltered",
                        query["query_id"],
                    )
                ]["result_ids"]
                recalls.append(len(set(actual).intersection(expected)) / len(expected))
            mean = sum(recalls) / len(recalls)
            if abs(float(gates["mean_recall_at_10"]) - mean) > 1e-15:
                raise ValidationError("ANN mean recall calculation differs")
            if gates["recall_gate_passed"] != (mean >= 0.99):
                raise ValidationError("ANN recall gate differs")
        if (
            row["system_id"] in {"vectorkit_f32_exact", "sqlite_vec_exact"}
            and row["status"] == "success"
        ):
            for operation in ["exact_unfiltered", "exact_filtered"]:
                for query in query_specs_by_workload[row["workload_id"]]:
                    actual = measured[
                        (
                            row["system_id"],
                            row["workload_id"],
                            operation,
                            query["query_id"],
                        )
                    ]["result_ids"]
                    expected = measured[
                        (
                            "numpy_f32_oracle",
                            row["workload_id"],
                            operation,
                            query["query_id"],
                        )
                    ]["result_ids"]
                    if actual != expected:
                        raise ValidationError("exact result differs from oracle")

    row_acceptance = []
    for row in summary["rows"]:
        gate_values = [
            value for value in row["gates"].values() if isinstance(value, bool)
        ]
        accepted = row["status"] == "success" and all(gate_values)
        if row["system_id"] == "numpy_f32_oracle":
            accepted = row["status"] == "success"
        expected = "passed" if accepted else "failed"
        if row["acceptance"] != expected:
            raise ValidationError("row acceptance differs from recomputed gates")
        row_acceptance.append(accepted)
    expected_overall = "passed" if all(row_acceptance) else "failed"
    if summary["overall_acceptance"] != expected_overall:
        raise ValidationError("overall acceptance differs from recomputed rows")


def validate_measurements(
    config: dict[str, Any], measurements: list[dict[str, Any]], summary: dict[str, Any]
) -> None:
    expected_order = sorted(
        measurements,
        key=lambda value: (
            value["system_id"],
            value["workload_id"],
            value["operation_id"],
            value["stage"],
            value["sample_index"],
            value["query_id"],
        ),
    )
    if measurements != expected_order:
        raise ValidationError("raw measurements are not in canonical identity order")
    groups: dict[tuple[str, str, str, str], list[int]] = defaultdict(list)
    for value in measurements:
        if not isinstance(value["duration_ns"], int) or value["duration_ns"] < 0:
            raise ValidationError("duration must be a non-negative integer")
        groups[
            (
                str(value["system_id"]),
                str(value["workload_id"]),
                str(value["operation_id"]),
                str(value["stage"]),
            )
        ].append(int(value["duration_ns"]))
    sample_count = int(config["measurement"]["samples"])
    for key, values in groups.items():
        if len(values) != sample_count:
            raise ValidationError(f"sample count differs for {key}")
    rows = {
        (value["system_id"], value["workload_id"]): value for value in summary["rows"]
    }
    for (system_id, workload_id, operation_id, stage), values in groups.items():
        operations = {
            value["operation_id"]: value
            for value in rows[(system_id, workload_id)]["operations"]
        }
        declared = operations[operation_id]["distribution"]
        actual = nearest_rank_distribution(values)
        if isinstance(declared, dict) and "sample_count" in declared:
            expected = declared
        else:
            expected = declared[stage]
        if actual != expected:
            raise ValidationError(
                f"distribution differs for {(system_id, workload_id, operation_id, stage)}"
            )


def validate_feature_parity(
    feature_parity: dict[str, Any], failures: list[dict[str, Any]]
) -> None:
    checked = json.loads(FEATURE_PARITY_PATH.read_text())
    if feature_parity != checked:
        raise ValidationError("feature parity differs from checked-in matrix")
    if feature_parity["cells"]["usearch_hnsw"]["ann_equality_filter"] != "unsupported":
        raise ValidationError("USearch filtered ANN must remain unsupported")
    if (
        feature_parity["cells"]["sqlite_custom_graph_app"]["graph_scoped_hybrid"]
        != "non_equivalent"
    ):
        raise ValidationError("custom application hybrid must remain non-equivalent")
    unsupported = {
        (value["system_id"], value.get("operation_id"))
        for value in failures
        if value["classification"] == "unsupported_operation"
    }
    if ("usearch_hnsw", "ann_filtered") not in unsupported:
        raise ValidationError("filtered ANN unsupported artifact missing")


def validate(root: Path) -> dict[str, Any]:
    root = root.resolve()
    validate_inventory(root)
    config = load_canonical_json(root / "config.json")
    environment = load_canonical_json(root / "environment.json")
    feature_parity = load_canonical_json(root / "feature-parity.json")
    input_manifests = load_canonical_json(root / "input-manifests.json")
    measurements = load_canonical_jsonl(root / "raw-measurements.jsonl")
    results = load_canonical_jsonl(root / "raw-results.jsonl")
    failures = load_canonical_jsonl(root / "failures.jsonl")
    summary = load_canonical_json(root / "summary.json")
    manifest = load_canonical_json(root / "manifest.json")
    require_fields(
        manifest,
        {
            "artifact_set_sha256",
            "artifact_type",
            "config_id",
            "config_sha256",
            "contract_id",
            "contract_sha256",
            "entries",
            "feature_parity_sha256",
            "file_count_excluding_manifest",
            "physical_device_execution",
            "schema_version",
            "source_revision",
            "supported_v1_capacity_changed",
        },
        "manifest",
    )
    if manifest["physical_device_execution"] is not False:
        raise ValidationError("physical-device execution is forbidden")
    if manifest["supported_v1_capacity_changed"] is not False:
        raise ValidationError("V1 capacity change is forbidden")
    if environment["physical_device_execution"] is not False:
        raise ValidationError("environment claims physical-device execution")
    if environment["embedding_included"] is not False:
        raise ValidationError("embedding latency must remain excluded")
    if manifest["contract_sha256"] != sha256_file(CONTRACT_PATH):
        raise ValidationError("contract checksum differs")
    if manifest["config_sha256"] != sha256_file(root / "config.json"):
        raise ValidationError("config checksum differs")
    validate_checksums(root, manifest)
    expected_manifests = []
    for workload in config["workloads"]:
        regenerated, _queries = regenerate_manifest(workload)
        expected_manifests.append(regenerated)
    expected_manifests.sort(key=lambda value: value["workload_id"])
    if input_manifests != {
        "artifact_type": "phase5_input_manifests",
        "manifests": expected_manifests,
        "schema_version": 1,
    }:
        raise ValidationError("independent input manifest replay differs")
    validate_feature_parity(feature_parity, failures)
    validate_measurements(config, measurements, summary)
    validate_results(config, results, summary)
    return {
        "artifact_set_sha256": manifest["artifact_set_sha256"],
        "artifact_type": "phase5_independent_validation",
        "benchmark_acceptance": summary["overall_acceptance"],
        "failure_count": sum(
            value["classification"] == "adapter_failure" for value in failures
        ),
        "result": "PASS",
        "schema_version": 1,
        "unsupported_operation_count": sum(
            value["classification"] == "unsupported_operation" for value in failures
        ),
        "validated_file_count": len(FILES),
        "validated_measurement_count": len(measurements),
        "validated_result_count": len(results),
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        report = validate(arguments.root)
    except (OSError, ValueError, KeyError, TypeError, ValidationError) as error:
        report = {
            "artifact_type": "phase5_independent_validation",
            "error": str(error),
            "result": "FAIL",
            "schema_version": 1,
        }
        if arguments.output:
            arguments.output.write_bytes(canonical(report) + b"\n")
        print(json.dumps(report, sort_keys=True))
        return 1
    if arguments.output:
        arguments.output.write_bytes(canonical(report) + b"\n")
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
