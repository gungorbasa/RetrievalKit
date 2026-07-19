#!/usr/bin/env python3
"""Independent validator for frozen Phase 4 target-device graph artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import mmap
import shutil
import statistics
import struct
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SCRIPT_DIRECTORY = Path(__file__).resolve().parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))

from authorization_lineage import (  # noqa: E402
    LineageError,
    is_preserved_v3_path,
    validate_lineage,
)

WORKLOAD_IDS = (
    "10k-384d-v3",
    "25k-384d-v3",
    "50k-384d-v3",
    "100k-384d-v3-stress",
)
SUPPORTED_WORKLOAD_IDS = WORKLOAD_IDS[:3]
STRESS_WORKLOAD_ID = WORKLOAD_IDS[3]
DEVICE_MATRIX = (
    ("iphone17-pro-max", "iPhone 17 Pro Max", "iPhone18,2", True),
)
STAGES = (
    "seed_resolution",
    "traversal",
    "projection",
    "filter_intersection",
    "ranking",
    "hydration",
    "end_to_end_total",
)
CORRECTNESS_STAGES = (
    "build_corpus_and_retrieval",
    "build_graph",
    "correctness_queries",
    "save",
    "read_only_validation",
    "cold_load",
    "cold_load_replay",
    "warm_load",
    "warm_load_replay",
)
CHECKS = (
    "semantic:passed",
    "exact_name:passed",
    "hybrid:passed",
    "metadata_filter:passed",
    "graph_1hop:passed",
    "graph_2hop:passed",
    "graph_3hop:passed",
    "graph_filter:passed",
    "active_deleted_counts:passed",
    "stable_identities:passed",
    "persistence_reload_policy:passed",
)
MAGIC = b"VECTORKIT-PHASE4-V1\0"


class ValidationError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"cannot read JSON '{path}': {error}") from error
    require(isinstance(value, dict), f"'{path}' must contain one JSON object")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as stream:
            while block := stream.read(1024 * 1024):
                digest.update(block)
    except OSError as error:
        raise ValidationError(f"cannot hash '{path}': {error}") from error
    return digest.hexdigest()


def nearest_rank(values: list[int], percentile: int) -> int:
    require(bool(values), "percentile distribution cannot be empty")
    ordered = sorted(values)
    rank = max(1, (len(ordered) * percentile + 99) // 100)
    return ordered[rank - 1]


def reject_100k_claim(value: dict[str, Any], label: str) -> None:
    if value.get("workload_id") != "100k-384d-v3-stress":
        return
    require(value.get("classification") == "stress", f"{label}: 100K must be stress")
    require(value.get("marketing_eligible", False) is False, f"{label}: 100K cannot market")
    require(
        value.get("supported_v1_capacity_changed", False) is False,
        f"{label}: 100K cannot change supported capacity",
    )
    for key in ("support_classification", "product_classification", "claim_classification"):
        require(key not in value, f"{label}: 100K cannot contain {key}")


@dataclass(frozen=True)
class Workload:
    workload_id: str
    classification: str
    active_records: int
    deleted_records: int
    active_chunks: int
    deleted_chunks: int
    graph_nodes: int
    graph_edges: int


@dataclass(frozen=True)
class EvidenceAuthorization:
    authorization_sha256: str
    hashes: dict[str, Any]


@dataclass(frozen=True)
class AuthorizationResolver:
    artifact_root: Path
    current: EvidenceAuthorization
    prior: EvidenceAuthorization | None = None

    def context_for(self, path: Path) -> EvidenceAuthorization:
        relative_path = path.relative_to(self.artifact_root).as_posix()
        if self.prior is not None and is_preserved_v3_path(relative_path):
            return self.prior
        return self.current


class BinaryReader:
    def __init__(self, data: mmap.mmap) -> None:
        self.data = data
        self.offset = 0

    def take(self, count: int) -> bytes:
        end = self.offset + count
        require(end <= len(self.data), "fixture ended unexpectedly")
        value = self.data[self.offset : end]
        self.offset = end
        return value

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def u64(self) -> int:
        return struct.unpack("<Q", self.take(8))[0]

    def string(self) -> str:
        size = self.u32()
        require(size <= 1024 * 1024, "fixture string exceeds independent bound")
        try:
            return self.take(size).decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValidationError("fixture string is not UTF-8") from error


def validate_fixture_binary(path: Path, workload: Workload, policy_sha256: str) -> None:
    try:
        with path.open("rb") as stream, mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ) as data:
            reader = BinaryReader(data)
            require(reader.take(len(MAGIC)) == MAGIC, "fixture magic mismatch")
            require(reader.string() == workload.workload_id, "fixture workload mismatch")
            require(reader.string() == workload.classification, "fixture classification mismatch")
            require(reader.string() == policy_sha256, "fixture policy mismatch")
            expected_header = (
                workload.active_records,
                workload.deleted_records,
                workload.active_chunks,
                workload.deleted_chunks,
                workload.graph_nodes,
                workload.graph_edges,
            )
            require(tuple(reader.u64() for _ in range(6)) == expected_header, "fixture counts mismatch")
            require(reader.u32() == 384, "fixture dimension mismatch")
            categories = [reader.string() for _ in range(reader.u32())]
            require(tuple(categories) == STAGES[:0] + (
                "semantic", "exact_name", "hybrid", "metadata_filter", "graph_1hop",
                "graph_2hop", "graph_3hop", "graph_filter",
            ), "fixture query categories mismatch")
            total_records = workload.active_records + workload.deleted_records
            for ordinal in range(total_records):
                deleted = ordinal >= workload.active_records
                record_ordinal = ordinal - workload.active_records if deleted else ordinal
                expected_id = (
                    f"deleted-{record_ordinal:08}" if deleted else f"record-{record_ordinal:08}"
                )
                require(reader.string() == expected_id, "fixture record identity mismatch")
                require(reader.take(1) == bytes([int(deleted)]), "fixture deleted state mismatch")
                require(reader.u64() == record_ordinal, "fixture record ordinal mismatch")
                reference_count = reader.u32()
                expected_references = 4 + int(record_ordinal % 5 != 0)
                require(reference_count == expected_references, "fixture reference count mismatch")
                for _ in range(reference_count):
                    target = reader.string()
                    require(target.startswith("record-") and len(target) == 15, "bad reference identity")
                require(reader.u32() == 4, "fixture chunks-per-record mismatch")
                for chunk in range(4):
                    require(reader.string() == f"chunk-{chunk:02}", "fixture chunk key mismatch")
                    text = reader.string()
                    require(("deleted" if deleted else "active") in text, "fixture text state mismatch")
                    require(reader.u32() == record_ordinal % 4, "fixture tenant bucket mismatch")
                    require(
                        reader.u32() == (record_ordinal * 4 + chunk) % 8,
                        "fixture category bucket mismatch",
                    )
                    reader.take(384 * 4)
            require(reader.offset == len(data), "fixture contains trailing bytes")
    except OSError as error:
        raise ValidationError(f"cannot parse fixture '{path}': {error}") from error


def workload_map(repo: Path) -> dict[str, Workload]:
    config = load_json(repo / "benchmarks/device-graph/workloads-v1.json")
    values: dict[str, Workload] = {}
    for row in config.get("workloads", []):
        item = Workload(
            workload_id=row["id"],
            classification=row["classification"],
            active_records=row["active_records"],
            deleted_records=row["deleted_records"],
            active_chunks=row["active_chunks"],
            deleted_chunks=row["deleted_chunks"],
            graph_nodes=row["graph_nodes"],
            graph_edges=row["graph_edges"],
        )
        values[item.workload_id] = item
    require(tuple(values) == WORKLOAD_IDS, "workload configuration order/IDs changed")
    return values


def validate_generations(repo: Path, root: Path, workloads: dict[str, Workload]) -> list[dict[str, Any]]:
    registry = load_json(repo / "benchmarks/device-graph/fixture-identities-v1.json")
    identities = {row["workload_id"]: row for row in registry["artifacts"]}
    policy = registry["generator_policy_sha256"]
    results: list[dict[str, Any]] = []
    for workload_id in WORKLOAD_IDS:
        workload = workloads[workload_id]
        identity = identities[workload_id]
        directories = [root / f"generation-{pass_name}" / workload_id for pass_name in ("a", "b")]
        for directory in directories:
            fixture = directory / "fixture.bin"
            manifest_path = directory / "manifest.json"
            manifest = load_json(manifest_path)
            reject_100k_claim(manifest, str(manifest_path))
            expected = {
                "workload_id": workload_id,
                "classification": workload.classification,
                "active_records": workload.active_records,
                "deleted_records": workload.deleted_records,
                "active_chunks": workload.active_chunks,
                "deleted_chunks": workload.deleted_chunks,
                "graph_nodes": workload.graph_nodes,
                "graph_edges": workload.graph_edges,
            }
            require(all(manifest.get(key) == value for key, value in expected.items()), "manifest counts mismatch")
            require(manifest.get("policy_sha256") == policy, "manifest policy hash mismatch")
            require(fixture.stat().st_size == identity["fixture_bytes"], "fixture byte count mismatch")
            require(sha256_file(fixture) == identity["fixture_sha256"], "fixture SHA-256 mismatch")
            require(manifest_path.stat().st_size == identity["manifest_bytes"], "manifest bytes mismatch")
            require(sha256_file(manifest_path) == identity["manifest_sha256"], "manifest hash mismatch")
            validate_fixture_binary(fixture, workload, policy)
        for name in ("fixture.bin", "manifest.json"):
            require(
                (directories[0] / name).read_bytes() == (directories[1] / name).read_bytes(),
                f"repeated generation differs for {workload_id}/{name}",
            )
        results.append(identity)
    return results


def validate_mac_report(report: dict[str, Any], workload: Workload) -> None:
    reject_100k_claim(report, "Mac report")
    require(report.get("workload_id") == workload.workload_id, "Mac workload mismatch")
    require(report.get("classification") == workload.classification, "Mac class mismatch")
    require(report.get("status") == "passed", "Mac report did not pass")
    require(report.get("host", {}).get("required_host_match") is True, "Mac host mismatch")
    require(report.get("host", {}).get("release_build") is True, "Mac build is not release")
    for key in ("active_records", "deleted_records", "active_chunks", "deleted_chunks", "graph_nodes", "graph_edges"):
        require(report.get(key) == getattr(workload, key), f"Mac {key} mismatch")
    rows = report.get("configurations", [])
    require([row.get("encoding") for row in rows] == ["f32", "i8"], "Mac encoding matrix mismatch")
    for row in rows:
        require(row.get("status") == "passed", "Mac encoding failed")
        require(tuple(row.get("checks", [])) == CHECKS, "Mac correctness check set mismatch")
        require(
            tuple(stage.get("stage") for stage in row.get("stages", [])) == CORRECTNESS_STAGES,
            "Mac lifecycle boundaries mismatch",
        )
        require(all(stage.get("elapsed_nanoseconds", 0) > 0 for stage in row["stages"]), "bad lifecycle duration")
        components = row.get("persisted_components", {})
        component_keys = (
            "corpus_chunks_bytes", "vectors_quantization_bytes", "lexical_bm25_bytes",
            "graph_schema_bytes", "manifest_validation_bytes",
        )
        require(
            sum(components.get(key, -1) for key in component_keys)
            == components.get("complete_directory_bytes")
            == row.get("persisted_total_bytes"),
            "persisted component sum mismatch",
        )
        require(components.get("component_sum_matches_directory") is True, "component proof missing")
    preflight = report.get("device_preflight", {})
    require(preflight.get("estimated_peak_memory_bytes", 0) > 0, "memory preflight missing")
    require(preflight.get("persisted_f32_bytes") == rows[0]["persisted_total_bytes"], "F32 preflight mismatch")
    require(preflight.get("persisted_i8_bytes") == rows[1]["persisted_total_bytes"], "I8 preflight mismatch")


def validate_staged_report(report: dict[str, Any], workload: Workload) -> None:
    reject_100k_claim(report, "staged report")
    require(report.get("workload_id") == workload.workload_id, "staged workload mismatch")
    require(report.get("classification") == workload.classification, "staged class mismatch")
    require(report.get("build_configuration") == "release", "staged build is not release")
    require(report.get("embedding_included") is False, "staged report includes embedding")
    require(report.get("warmups") == 100, "staged warmup count mismatch")
    require(report.get("samples_per_stage") == 1000, "staged sample count mismatch")
    require(report.get("percentile_method") == "nearest_rank", "percentile policy mismatch")
    require(tuple(report.get("stages", [])) == STAGES, "stage declaration mismatch")
    rows = report.get("configurations", [])
    require([row.get("encoding") for row in rows] == ["f32", "i8"], "staged encodings mismatch")
    for row in rows:
        samples = row.get("samples", [])
        require(len(samples) == 1000, "raw query sample count mismatch")
        values = {stage: [] for stage in STAGES}
        identities = (
            row.get("result_identity_sha256"), row.get("selection_identity_sha256"),
            row.get("path_identity_sha256"), row.get("filter_identity_sha256"),
        )
        require(all(isinstance(value, str) and len(value) == 64 for value in identities), "identity hash missing")
        for index, sample in enumerate(samples):
            require(sample.get("sample_index") == index, "sample index gap")
            require(sample.get("deleted_results") == 0, "deleted result leaked")
            sample_stages = sample.get("stages", [])
            require(len(sample_stages) == len(STAGES), "sample stage boundary count mismatch")
            for sequence, (stage, stage_sample) in enumerate(zip(STAGES, sample_stages, strict=True)):
                require(stage_sample.get("stage") == stage, "sample stage order mismatch")
                require(stage_sample.get("sequence") == sequence, "sample stage sequence mismatch")
                require(isinstance(stage_sample.get("duration_ns"), int), "stage duration is not integer")
                require(
                    stage_sample.get("directly_measured") is (stage == "end_to_end_total"),
                    "direct-total marker mismatch",
                )
                values[stage].append(stage_sample["duration_ns"])
            sample_identities = (
                sample.get("result_identity_sha256"), sample.get("selection_identity_sha256"),
                sample.get("path_identity_sha256"), sample.get("filter_identity_sha256"),
            )
            require(sample_identities == identities, "stable sample identity changed")
        distributions = {item.get("stage"): item for item in row.get("distributions", [])}
        require(tuple(distributions) == STAGES, "distribution stage set/order mismatch")
        for stage in STAGES:
            distribution = distributions[stage]
            samples_for_stage = values[stage]
            require(distribution.get("sample_count") == 1000, "distribution count mismatch")
            require(distribution.get("min_ns") == min(samples_for_stage), "minimum mismatch")
            require(distribution.get("max_ns") == max(samples_for_stage), "maximum mismatch")
            require(distribution.get("mean_ns") == sum(samples_for_stage) // 1000, "mean mismatch")
            for percentile in (50, 95, 99):
                require(
                    distribution.get(f"p{percentile}_ns") == nearest_rank(samples_for_stage, percentile),
                    f"P{percentile} is not nearest-rank",
                )


def validate_linkage(base_binary: Path, graph_binary: Path) -> None:
    nm = shutil.which("nm")
    strings = shutil.which("strings")
    require(nm is not None and strings is not None, "nm and strings are required")
    base_symbols = subprocess.run([nm, "-g", str(base_binary)], check=True, capture_output=True, text=True).stdout
    graph_symbols = subprocess.run([nm, "-g", str(graph_binary)], check=True, capture_output=True, text=True).stdout
    require("_vectorkit_graph_" not in base_symbols, "graph-free binary contains graph symbols")
    require("_vectorkit_phase4_graph_free_regression_json" in base_symbols, "graph-free API missing")
    require("_vectorkit_graph_ffi_abi_version" in graph_symbols, "graph-capable API missing")
    require("_vectorkit_phase4_graph_free_regression_json" in graph_symbols, "candidate base API missing")
    subprocess.run([strings, str(base_binary)], check=True, capture_output=True, text=True)


def validate_memory(value: dict[str, Any], label: str) -> None:
    require(value.get("sample_interval_ms") == 1, f"{label}: RSS interval mismatch")
    samples = value.get("samples")
    require(isinstance(samples, list) and len(samples) >= 2, f"{label}: raw RSS samples missing")
    offsets = [item.get("offset_ns") for item in samples]
    residents = [item.get("resident_bytes") for item in samples]
    require(all(isinstance(item, int) and item >= 0 for item in offsets), f"{label}: bad RSS offsets")
    require(offsets == sorted(offsets) and len(set(offsets)) == len(offsets), f"{label}: RSS offsets unordered")
    require(all(isinstance(item, int) and item > 0 for item in residents), f"{label}: bad RSS values")
    baseline = value.get("baseline_resident_bytes")
    peak = value.get("peak_resident_bytes")
    delta = value.get("peak_delta_bytes")
    require(peak == max(residents), f"{label}: RSS peak mismatch")
    require(isinstance(baseline, int) and delta == max(0, peak - baseline), f"{label}: RSS delta mismatch")


def validate_envelope(
    value: dict[str, Any], path: Path, role: str, product: str,
    authorization: EvidenceAuthorization, process_ids: set[int],
) -> None:
    hashes = authorization.hashes
    require(value.get("ok") is True and value.get("collector_exit_code") == 0, f"{path}: failed attempt")
    require(value.get("atomic_write_completed") is True, f"{path}: atomic write proof missing")
    require(value.get("device_role") == role, f"{path}: device role mismatch")
    require(value.get("host_device_identifier") == hashes["core_device_id"], f"{path}: host device mismatch")
    require(
        value.get("authorization_sha256") == authorization.authorization_sha256,
        f"{path}: unauthorized evidence",
    )
    require(value.get("product_role") == product, f"{path}: product mismatch")
    require(value.get("app_executable_sha256") == hashes[f"{product}_app"], f"{path}: stale app")
    require(value.get("framework_binary_sha256") == hashes[f"{product}_framework"], f"{path}: stale framework")
    environment = value.get("environment", {})
    require(environment.get("physical_device") is True, f"{path}: physical device required")
    require(environment.get("simulator") is False, f"{path}: simulator evidence rejected")
    require(environment.get("build_configuration") == "release", f"{path}: release required")
    require(environment.get("device_identifier") == hashes["product_type"], f"{path}: hardware ID mismatch")
    require(environment.get("hardware_model") == hashes["hardware_model"], f"{path}: hardware model mismatch")
    allowed_os_builds = hashes.get("allowed_os_builds", (hashes["os_build"],))
    require(
        any(build in str(environment.get("os_build")) for build in allowed_os_builds),
        f"{path}: OS build mismatch",
    )
    require(environment.get("thermal_state_start") in ("nominal", "fair"), f"{path}: thermal start invalid")
    require(environment.get("thermal_state_end") in ("nominal", "fair"), f"{path}: thermal end invalid")
    require(environment.get("one_scenario_per_fresh_process") is True, f"{path}: fresh process missing")
    require(environment.get("low_power_mode") is False, f"{path}: Low Power Mode invalid")
    require(environment.get("foreground") is True, f"{path}: foreground execution required")
    require(environment.get("network_disabled") is True, f"{path}: network isolation missing")
    require(environment.get("physical_memory_bytes", 0) > 0, f"{path}: physical memory missing")
    process_id = environment.get("process_id")
    require(isinstance(process_id, int) and process_id not in process_ids, f"{path}: process reused")
    process_ids.add(process_id)


def validate_query_report(value: dict[str, Any], path: Path, workload: str, encoding: str) -> tuple[str, ...]:
    report = value.get("report", {})
    reject_100k_claim(report, str(path))
    require(report.get("artifact_type") == "phase4b_device_query_session", f"{path}: query artifact mismatch")
    require(report.get("workload_id") == workload and report.get("encoding") == encoding, f"{path}: query config mismatch")
    require(report.get("warmups_per_scenario") == 100, f"{path}: warmup mismatch")
    require(report.get("samples_per_scenario") == 1000, f"{path}: query sample count mismatch")
    require(report.get("percentile_method") == "nearest_rank", f"{path}: percentile policy mismatch")
    require(tuple(report.get("stages", [])) == STAGES, f"{path}: stage declaration mismatch")
    categories = ("semantic", "exact_name", "hybrid", "metadata_filter", "graph_1hop", "graph_2hop", "graph_3hop", "graph_filter")
    require(tuple(report.get("query_categories", [])) == categories, f"{path}: category declaration mismatch")
    require(bool(report.get("correctness_checks")), f"{path}: correctness checks missing")
    scenarios = report.get("scenarios", [])
    require(tuple(item.get("query_category") for item in scenarios) == categories, f"{path}: scenario matrix mismatch")
    identity_set: list[str] = []
    for scenario in scenarios:
        samples = scenario.get("samples", [])
        require(len(samples) == 1000, f"{path}: raw sample count mismatch")
        identities = tuple(scenario.get(key) for key in (
            "result_identity_sha256", "selection_identity_sha256", "path_identity_sha256", "filter_identity_sha256"
        ))
        require(all(isinstance(item, str) and len(item) == 64 for item in identities), f"{path}: identity hash missing")
        identity_set.extend(identities)
        stage_values = {stage: [] for stage in STAGES}
        for index, sample in enumerate(samples):
            require(sample.get("sample_index") == index and sample.get("deleted_results") == 0, f"{path}: sample correctness failure")
            require(tuple(sample.get(key) for key in (
                "result_identity_sha256", "selection_identity_sha256", "path_identity_sha256", "filter_identity_sha256"
            )) == identities, f"{path}: sample identity drift")
            stages = sample.get("stages", [])
            require(len(stages) == len(STAGES), f"{path}: stage sample missing")
            for sequence, (stage, measured) in enumerate(zip(STAGES, stages, strict=True)):
                require(measured.get("stage") == stage and measured.get("sequence") == sequence, f"{path}: stage order mismatch")
                duration = measured.get("duration_ns")
                require(isinstance(duration, int) and duration >= 0, f"{path}: bad stage duration")
                require(measured.get("directly_measured") is (stage == "end_to_end_total"), f"{path}: direct total marker mismatch")
                stage_values[stage].append(duration)
        distributions = scenario.get("distributions", [])
        require(tuple(item.get("stage") for item in distributions) == STAGES, f"{path}: distribution mismatch")
        for stage, distribution in zip(STAGES, distributions, strict=True):
            values = stage_values[stage]
            require(distribution.get("sample_count") == 1000, f"{path}: distribution count mismatch")
            require(distribution.get("min_ns") == min(values) and distribution.get("max_ns") == max(values), f"{path}: range mismatch")
            require(distribution.get("mean_ns") == sum(values) // 1000, f"{path}: mean mismatch")
            for percentile in (50, 95, 99):
                require(distribution.get(f"p{percentile}_ns") == nearest_rank(values, percentile), f"{path}: P{percentile} mismatch")
    validate_memory(value.get("memory_evidence", {}), str(path))
    return tuple(identity_set)


def validate_lifecycle(
    directory: Path, role: str, workload: str, encoding: str,
    authorizations: AuthorizationResolver, process_ids: set[int],
) -> int:
    prepare_path = directory / "prepare.json"
    prepare = load_json(prepare_path)
    validate_envelope(
        prepare,
        prepare_path,
        role,
        "candidate",
        authorizations.context_for(prepare_path),
        process_ids,
    )
    validate_memory(prepare.get("memory_evidence", {}), str(prepare_path))
    persisted = prepare.get("report", {}).get("persisted_components", {})
    component_keys = ("corpus_chunks_bytes", "vectors_quantization_bytes", "lexical_bm25_bytes", "graph_schema_bytes", "manifest_validation_bytes")
    require(sum(persisted.get(key, -1) for key in component_keys) == persisted.get("complete_directory_bytes"), f"{prepare_path}: component sum mismatch")
    count = 1
    for operation in ("build", "save", "read_only_validation", "cold_load", "warm_load", "replay"):
        operation_root = directory / operation
        warmups = sorted(operation_root.glob("warmup-*.json"))
        samples = sorted(operation_root.glob("sample-*.json"))
        require(len(warmups) == (0 if operation == "cold_load" else 3), f"{operation_root}: lifecycle warmup mismatch")
        require(len(samples) == 20, f"{operation_root}: lifecycle sample mismatch")
        for path in warmups + samples:
            value = load_json(path)
            validate_envelope(
                value,
                path,
                role,
                "candidate",
                authorizations.context_for(path),
                process_ids,
            )
            report = value.get("report", {})
            require(report.get("artifact_type") == "phase4b_device_lifecycle_sample", f"{path}: lifecycle artifact mismatch")
            require(report.get("workload_id") == workload and report.get("encoding") == encoding, f"{path}: lifecycle config mismatch")
            require(report.get("operation") == operation and report.get("operation_duration_ns", 0) > 0, f"{path}: lifecycle operation mismatch")
            if operation in ("cold_load", "warm_load", "replay"):
                require(report.get("replay_equivalent") is True, f"{path}: replay equivalence missing")
            validate_memory(value.get("memory_evidence", {}), str(path))
            count += 1
    return count


def validate_graph_free(
    root: Path, role: str, authorizations: AuthorizationResolver, process_ids: set[int],
) -> tuple[int, dict[str, float]]:
    count = 0
    regressions: dict[str, float] = {}
    for encoding in ("f32", "i8"):
        products: dict[str, list[dict[str, Any]]] = {}
        for product in ("baseline", "candidate"):
            paths = sorted((root / encoding / product).glob("session-*.json"))
            require(len(paths) == 3, f"{role}/{encoding}/{product}: three graph-free sessions required")
            reports = []
            for path in paths:
                value = load_json(path)
                validate_envelope(
                    value,
                    path,
                    role,
                    product,
                    authorizations.context_for(path),
                    process_ids,
                )
                report = value.get("report", {})
                require(report.get("artifact_type") == "phase4b_graph_free_regression_session", f"{path}: graph-free artifact mismatch")
                require(report.get("workload_id") == "10k-384d-v3" and report.get("encoding") == encoding, f"{path}: graph-free config mismatch")
                require(report.get("warmups") == 100 and report.get("samples") == 1000, f"{path}: graph-free counts mismatch")
                require(set(report.get("graph_counters", {}).values()) == {0}, f"{path}: graph activity detected")
                for scenario in report.get("scenarios", []):
                    raw = scenario.get("raw_duration_ns", [])
                    require(len(raw) == 1000 and scenario.get("deleted_hits") == 0, f"{path}: graph-free samples invalid")
                    for percentile in (50, 95, 99):
                        require(scenario.get(f"p{percentile}_ns") == nearest_rank(raw, percentile), f"{path}: graph-free P{percentile} mismatch")
                reports.append(report)
                count += 1
            products[product] = reports
        for scenario_index, scenario_name in enumerate(("semantic_exact_vector", "bm25_internal", "hybrid_weighted_normalized_0.6_0.4")):
            baseline_rows = [report["scenarios"][scenario_index] for report in products["baseline"]]
            candidate_rows = [report["scenarios"][scenario_index] for report in products["candidate"]]
            require(all(row.get("scenario") == scenario_name for row in baseline_rows + candidate_rows), f"{role}/{encoding}: scenario mismatch")
            baseline_hashes = {row.get("result_identity_sha256") for row in baseline_rows}
            candidate_hashes = {row.get("result_identity_sha256") for row in candidate_rows}
            require(len(baseline_hashes) == 1 and baseline_hashes == candidate_hashes, f"{role}/{encoding}/{scenario_name}: result mismatch")
            baseline_p95 = statistics.median(row["p95_ns"] for row in baseline_rows)
            candidate_p95 = statistics.median(row["p95_ns"] for row in candidate_rows)
            ratio = candidate_p95 / baseline_p95
            require(ratio <= 1.03, f"{role}/{encoding}/{scenario_name}: regression {ratio:.6f} exceeds 1.03")
            regressions[f"{encoding}/{scenario_name}"] = ratio
    return count, regressions


def validate_authorization(
    repo: Path, path: Path, base_binary: Path, graph_binary: Path,
    base_framework: Path, graph_framework: Path,
) -> tuple[dict[str, Any], str]:
    value = load_json(path)
    require(value.get("schema_version") == 1 and value.get("artifact_type") == "phase4b_execution_authorization", "authorization schema mismatch")
    commit = value.get("authorized_source_commit")
    require(isinstance(commit, str) and len(commit) == 40, "authorization source commit missing")
    subprocess.run(["git", "cat-file", "-e", f"{commit}^{{commit}}"], cwd=repo, check=True)
    expected_files = {
        "protocol_sha256": repo / "benchmarks/device-graph/protocol-v1.json",
        "workloads_sha256": repo / "benchmarks/device-graph/workloads-v1.json",
        "fixture_registry_sha256": repo / "benchmarks/device-graph/fixture-identities-v1.json",
    }
    for key, file in expected_files.items():
        require(value.get(key) == sha256_file(file), f"authorization {key} mismatch")
    products = value.get("products", {})
    require(products.get("baseline", {}).get("app_executable_sha256") == sha256_file(base_binary), "authorized base executable mismatch")
    require(products.get("candidate", {}).get("app_executable_sha256") == sha256_file(graph_binary), "authorized graph executable mismatch")
    require(products.get("baseline", {}).get("framework_binary_sha256") == sha256_file(base_framework), "authorized base framework mismatch")
    require(products.get("candidate", {}).get("framework_binary_sha256") == sha256_file(graph_framework), "authorized graph framework mismatch")
    return value, sha256_file(path)


def validate_device_sessions(
    repo: Path, root: Path, authorization_path: Path, base_binary: Path, graph_binary: Path,
    base_framework: Path, graph_framework: Path,
    prior_authorization_path: Path | None = None,
    prior_base_binary: Path | None = None,
    prior_graph_binary: Path | None = None,
    prior_base_framework: Path | None = None,
    prior_graph_framework: Path | None = None,
) -> dict[str, Any]:
    authorization, authorization_sha256 = validate_authorization(
        repo, authorization_path, base_binary, graph_binary, base_framework, graph_framework,
    )
    prior_authorization: dict[str, Any] | None = None
    prior_authorization_sha256: str | None = None
    if authorization.get("evidence_lineage") is not None:
        require(prior_authorization_path is not None, "lineage requires --prior-authorization")
        require(prior_base_binary is not None, "lineage requires --prior-base-binary")
        require(prior_graph_binary is not None, "lineage requires --prior-graph-binary")
        require(prior_base_framework is not None, "lineage requires --prior-base-framework")
        require(prior_graph_framework is not None, "lineage requires --prior-graph-framework")
        prior_authorization, prior_authorization_sha256 = validate_authorization(
            repo,
            prior_authorization_path,
            prior_base_binary,
            prior_graph_binary,
            prior_base_framework,
            prior_graph_framework,
        )
        validate_linkage(prior_base_binary, prior_graph_binary)
        try:
            validate_lineage(authorization, prior_authorization_sha256, root)
        except LineageError as error:
            raise ValidationError(str(error)) from error
    validate_linkage(base_binary, graph_binary)
    devices_root = root / "devices"
    require(devices_root.is_dir(), "Phase 4b requires a device-scoped 'devices' root")
    expected_directories = {role for role, _, _, _ in DEVICE_MATRIX}
    actual_directories = {path.name for path in devices_root.iterdir() if path.is_dir()}
    require(actual_directories == expected_directories, "physical-device directory matrix mismatch")

    validated: dict[str, int] = {}
    graph_free_regressions: dict[str, Any] = {}
    authorized_devices = authorization.get("devices", {})
    for role, model, identifier, runs_stress in DEVICE_MATRIX:
        device_root = devices_root / role
        registered = authorized_devices.get(role, {})
        require(registered.get("marketing_name") == model and registered.get("product_type") == identifier, f"{role}: authorization device mismatch")
        products = authorization["products"]
        current_hashes = {
            "core_device_id": registered["core_device_id"], "product_type": identifier,
            "hardware_model": registered["hardware_model"], "os_build": registered["os_build"],
            "baseline_app": products["baseline"]["app_executable_sha256"],
            "baseline_framework": products["baseline"]["framework_binary_sha256"],
            "candidate_app": products["candidate"]["app_executable_sha256"],
            "candidate_framework": products["candidate"]["framework_binary_sha256"],
        }
        current_context = EvidenceAuthorization(authorization_sha256, current_hashes)
        prior_context = None
        if prior_authorization is not None and prior_authorization_sha256 is not None:
            prior_registered = prior_authorization.get("devices", {}).get(role, {})
            require(
                prior_registered.get("marketing_name") == model
                and prior_registered.get("product_type") == identifier,
                f"{role}: prior authorization device mismatch",
            )
            prior_products = prior_authorization["products"]
            prior_hashes = {
                "core_device_id": prior_registered["core_device_id"],
                "product_type": identifier,
                "hardware_model": prior_registered["hardware_model"],
                "os_build": prior_registered["os_build"],
                "allowed_os_builds": tuple(
                    authorization["evidence_lineage"]["prior_allowed_os_builds"]
                ),
                "baseline_app": prior_products["baseline"]["app_executable_sha256"],
                "baseline_framework": prior_products["baseline"]["framework_binary_sha256"],
                "candidate_app": prior_products["candidate"]["app_executable_sha256"],
                "candidate_framework": prior_products["candidate"]["framework_binary_sha256"],
            }
            prior_context = EvidenceAuthorization(prior_authorization_sha256, prior_hashes)
        authorizations = AuthorizationResolver(root, current_context, prior_context)
        process_ids: set[int] = set()
        count = 0
        for workload in SUPPORTED_WORKLOAD_IDS:
            for encoding in ("f32", "i8"):
                config_root = device_root / "supported" / workload / encoding
                sessions = sorted((config_root / "query").glob("session-*.json"))
                require(len(sessions) == 5, f"{role}/{workload}/{encoding}: five query/RSS sessions required")
                identities = []
                for path in sessions:
                    value = load_json(path)
                    validate_envelope(
                        value,
                        path,
                        role,
                        "candidate",
                        authorizations.context_for(path),
                        process_ids,
                    )
                    identities.append(validate_query_report(value, path, workload, encoding))
                    count += 1
                require(len(set(identities)) == 1, f"{role}/{workload}/{encoding}: session identity drift")
                count += validate_lifecycle(
                    config_root / "lifecycle", role, workload, encoding,
                    authorizations, process_ids,
                )
        graph_count, ratios = validate_graph_free(
            device_root / "graph-free", role, authorizations, process_ids,
        )
        count += graph_count
        graph_free_regressions[role] = ratios
        stress_root = device_root / "stress"
        if runs_stress:
            for encoding in ("f32", "i8"):
                config_root = stress_root / STRESS_WORKLOAD_ID / encoding
                preflight = load_json(config_root / "preflight.json")
                validate_envelope(
                    preflight,
                    config_root / "preflight.json",
                    role,
                    "candidate",
                    authorizations.context_for(config_root / "preflight.json"),
                    process_ids,
                )
                require(preflight.get("classification") == "stress" and preflight.get("marketing_eligible") is False, "100K preflight classification mismatch")
                sessions = sorted((config_root / "query").glob("session-*.json"))
                require(len(sessions) == 5, f"{role}/100K/{encoding}: five query sessions required")
                for path in sessions:
                    value = load_json(path)
                    validate_envelope(
                        value,
                        path,
                        role,
                        "candidate",
                        authorizations.context_for(path),
                        process_ids,
                    )
                    validate_query_report(value, path, STRESS_WORKLOAD_ID, encoding)
                    count += 1
                count += validate_lifecycle(
                    config_root / "lifecycle",
                    role,
                    STRESS_WORKLOAD_ID,
                    encoding,
                    authorizations,
                    process_ids,
                )
        else:
            require(
                not stress_root.exists(),
                f"{role}: the 100K stress lane is iPhone-17-only",
            )
        validated[role] = count
    return {"validated_artifacts": validated, "graph_free_ratios": graph_free_regressions}


def validate_phase4a(repo: Path, root: Path, base_binary: Path, graph_binary: Path) -> dict[str, Any]:
    workloads = workload_map(repo)
    identities = validate_generations(repo, root, workloads)
    for workload_id, workload in workloads.items():
        validate_mac_report(load_json(root / "mac" / workload_id / "mac-correctness-report.json"), workload)
        validate_staged_report(
            load_json(root / "measurements" / workload_id / "staged-measurement-report.json"), workload
        )
    validate_linkage(base_binary, graph_binary)
    return {
        "ok": True,
        "mode": "phase4a",
        "workloads": list(WORKLOAD_IDS),
        "fixture_identities": identities,
        "mac_configurations": 8,
        "staged_query_samples": 8_000,
        "physical_device_execution": False,
        "phase4b_pending": True,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("phase4a", "phase4b"), required=True)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--base-binary", type=Path)
    parser.add_argument("--graph-binary", type=Path)
    parser.add_argument("--base-framework", type=Path)
    parser.add_argument("--graph-framework", type=Path)
    parser.add_argument("--authorization", type=Path)
    parser.add_argument("--prior-authorization", type=Path)
    parser.add_argument("--prior-base-binary", type=Path)
    parser.add_argument("--prior-graph-binary", type=Path)
    parser.add_argument("--prior-base-framework", type=Path)
    parser.add_argument("--prior-graph-framework", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.mode == "phase4a":
            require(args.base_binary is not None, "--base-binary is required in Phase 4a")
            require(args.graph_binary is not None, "--graph-binary is required in Phase 4a")
            result = validate_phase4a(
                args.repo.resolve(), args.artifact_root.resolve(),
                args.base_binary.resolve(), args.graph_binary.resolve(),
            )
        else:
            require(args.authorization is not None, "--authorization is required in Phase 4b")
            require(args.base_binary is not None, "--base-binary is required in Phase 4b")
            require(args.graph_binary is not None, "--graph-binary is required in Phase 4b")
            require(args.base_framework is not None, "--base-framework is required in Phase 4b")
            require(args.graph_framework is not None, "--graph-framework is required in Phase 4b")
            sessions = validate_device_sessions(
                args.repo.resolve(), args.artifact_root.resolve(), args.authorization.resolve(),
                args.base_binary.resolve(), args.graph_binary.resolve(),
                args.base_framework.resolve(), args.graph_framework.resolve(),
                args.prior_authorization.resolve() if args.prior_authorization else None,
                args.prior_base_binary.resolve() if args.prior_base_binary else None,
                args.prior_graph_binary.resolve() if args.prior_graph_binary else None,
                args.prior_base_framework.resolve() if args.prior_base_framework else None,
                args.prior_graph_framework.resolve() if args.prior_graph_framework else None,
            )
            result = {
                "ok": True,
                "mode": "phase4b",
                "physical_device_matrix": "passed",
                **sessions,
            }
    except (ValidationError, OSError, subprocess.CalledProcessError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, sort_keys=True))
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
