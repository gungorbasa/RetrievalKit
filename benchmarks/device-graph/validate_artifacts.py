#!/usr/bin/env python3
"""Independent validator for frozen Phase 4 target-device graph artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import mmap
import shutil
import struct
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

WORKLOAD_IDS = (
    "10k-384d-v3",
    "25k-384d-v3",
    "50k-384d-v3",
    "100k-384d-v3-stress",
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
    require("_vectorkit_bench_memory_json" in base_symbols, "graph-free base API missing")
    require("_vectorkit_graph_ffi_abi_version" in graph_symbols, "graph-capable API missing")
    base_strings = subprocess.run([strings, str(base_binary)], check=True, capture_output=True, text=True).stdout
    for counter in ("graph_state_creations", "graph_file_opens", "graph_dispatches"):
        require(counter in base_strings, f"graph-free instrumentation missing {counter}")


def validate_device_sessions(root: Path) -> None:
    for workload_id in WORKLOAD_IDS:
        for encoding in ("f32", "i8"):
            directory = root / workload_id / encoding
            sessions = sorted(directory.glob("session-*.json"))
            require(len(sessions) >= 3, f"{workload_id}/{encoding}: three sessions required")
            process_ids: set[int] = set()
            for path in sessions:
                report = load_json(path)
                reject_100k_claim(report, str(path))
                require(report.get("physical_device") is True, f"{path}: physical device required")
                require(report.get("simulator") is False, f"{path}: simulator is invalid")
                for key in ("device_model", "device_identifier", "os_version", "power_state"):
                    require(bool(report.get(key)), f"{path}: missing {key}")
                require(report.get("thermal_state_start") in ("nominal", "fair"), f"{path}: bad thermal start")
                require(report.get("thermal_state_end") in ("nominal", "fair"), f"{path}: bad thermal end")
                require(report.get("rss_interval_ms") == 1, f"{path}: RSS interval mismatch")
                require(report.get("memory_repetitions") == 5, f"{path}: five memory repetitions required")
                require(report.get("lifecycle_samples") == 20, f"{path}: lifecycle samples mismatch")
                process_id = report.get("process_id")
                require(isinstance(process_id, int) and process_id not in process_ids, f"{path}: process reused")
                process_ids.add(process_id)
                graph_free = report.get("graph_free_evidence", {})
                if report.get("lane") == "graph_free":
                    require(
                        graph_free == {"state_creations": 0, "file_opens": 0, "dispatches": 0},
                        f"{path}: graph-free evidence is not zero",
                    )


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
            validate_device_sessions(args.artifact_root.resolve())
            result = {"ok": True, "mode": "phase4b", "physical_device_matrix": "passed"}
    except (ValidationError, OSError, subprocess.CalledProcessError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, sort_keys=True))
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
