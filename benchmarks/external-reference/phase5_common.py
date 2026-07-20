"""Shared benchmark-only helpers for Phase 5 external adapters.

The independent validator deliberately does not import this module.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import platform
import resource
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

GENERATOR_ID = "phase5-generator-v1"
TOP_K = 10
CHUNKS_PER_RECORD = 4


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
        raise ValueError("non-finite JSON numbers are forbidden")
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
    raise TypeError(f"unsupported canonical JSON value: {type(value).__name__}")


def canonical(value: Any) -> bytes:
    return _encode(value).encode()


def canonical_file(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value) + b"\n")


def canonical_jsonl(path: Path, values: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"".join(canonical(value) + b"\n" for value in values))


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def stable_chunk_id(ordinal: int) -> str:
    return f"chunk-{ordinal:08d}"


def stable_record_id(ordinal: int) -> str:
    return f"record-{ordinal:08d}"


def chunk_text(ordinal: int) -> str:
    return (
        f"topic{ordinal % 32} exact{ordinal} category{ordinal % 7} "
        f"tenant{ordinal % 10} local retrieval graph evidence"
    )


@dataclass(frozen=True)
class QuerySpec:
    query_id: str
    target_ordinal: int
    tenant: str
    query_text: str
    seed_record_ordinal: int

    def as_dict(self) -> dict[str, Any]:
        return {
            "query_id": self.query_id,
            "query_text": self.query_text,
            "seed_record_id": stable_record_id(self.seed_record_ordinal),
            "target_chunk_id": stable_chunk_id(self.target_ordinal),
            "tenant": self.tenant,
        }


@dataclass
class WorkloadData:
    spec: dict[str, Any]
    vectors: np.ndarray
    queries: np.ndarray
    query_specs: list[QuerySpec]
    input_manifest: dict[str, Any]

    @property
    def active_chunks(self) -> int:
        return int(self.spec["active_chunks"])

    @property
    def deleted_chunks(self) -> int:
        return int(self.spec["deleted_chunks"])

    @property
    def dimension(self) -> int:
        return int(self.spec["dimension"])

    @property
    def total_chunks(self) -> int:
        return self.active_chunks + self.deleted_chunks

    @property
    def active_records(self) -> int:
        return self.active_chunks // CHUNKS_PER_RECORD


def validate_workload_spec(spec: dict[str, Any]) -> None:
    expected = {
        "active_chunks",
        "deleted_chunks",
        "dimension",
        "query_count",
        "seed",
        "workload_id",
    }
    if set(spec) != expected:
        raise ValueError(f"workload fields must be exactly {sorted(expected)}")
    for field in ["active_chunks", "deleted_chunks", "dimension", "query_count"]:
        if not isinstance(spec[field], int) or spec[field] <= 0:
            raise ValueError(f"{field} must be a positive integer")
    if spec["active_chunks"] % CHUNKS_PER_RECORD != 0:
        raise ValueError("active_chunks must be divisible by four")
    if spec["deleted_chunks"] % CHUNKS_PER_RECORD != 0:
        raise ValueError("deleted_chunks must be divisible by four")
    if spec["query_count"] > spec["active_chunks"]:
        raise ValueError("query_count cannot exceed active_chunks")


def generate_workload(spec: dict[str, Any]) -> WorkloadData:
    validate_workload_spec(spec)
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
        QuerySpec(
            query_id=f"q-{index:04d}",
            target_ordinal=target,
            tenant=f"tenant-{target % 10}",
            query_text=f"topic{target % 32} exact{target}",
            seed_record_ordinal=((target // CHUNKS_PER_RECORD) - 1)
            % (active // CHUNKS_PER_RECORD),
        )
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
        "metadata_policy": {
            "category_modulus": 7,
            "tenant_modulus": 10,
        },
        "query_specs": [value.as_dict() for value in query_specs],
        "seed": int(spec["seed"]),
        "text_policy": "topic/exact/category/tenant/local-retrieval-graph-evidence-v1",
        "workload_id": spec["workload_id"],
    }
    manifest_without_identity = {
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
        "query_specs_sha256": sha256_bytes(
            canonical([value.as_dict() for value in query_specs])
        ),
    }
    manifest = {
        **manifest_without_identity,
        "input_identity_sha256": sha256_bytes(canonical(manifest_without_identity)),
    }
    return WorkloadData(spec, vectors, queries, query_specs, manifest)


def oracle_results(data: WorkloadData, *, filtered: bool) -> list[dict[str, Any]]:
    active_vectors = data.vectors[: data.active_chunks]
    ordinals = np.arange(data.active_chunks, dtype=np.int64)
    results = []
    for query_index, query_spec in enumerate(data.query_specs):
        scores = active_vectors @ data.queries[query_index]
        if filtered:
            allow = (ordinals % 10) == int(query_spec.tenant.removeprefix("tenant-"))
            candidates = ordinals[allow]
            candidate_scores = scores[allow]
        else:
            candidates = ordinals
            candidate_scores = scores
        order = np.lexsort((candidates, -candidate_scores))[:TOP_K]
        ordered = candidates[order]
        results.append(
            {
                "operation_id": "exact_filtered" if filtered else "exact_unfiltered",
                "query_id": query_spec.query_id,
                "result_ids": [stable_chunk_id(int(value)) for value in ordered],
                "scores": [float(candidate_scores[value]) for value in order],
            }
        )
    return results


def distribution(values: list[int]) -> dict[str, int | str]:
    if not values:
        raise ValueError("cannot summarize an empty distribution")
    ordered = sorted(values)

    def percentile(value: float) -> int:
        index = max(1, math.ceil(value * len(ordered))) - 1
        return ordered[index]

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


def result_identity(result_ids: list[str]) -> str:
    return sha256_bytes(canonical(result_ids))


def recall_at_k(actual: list[str], expected: list[str], k: int = TOP_K) -> float:
    expected_set = set(expected[:k])
    if not expected_set:
        return 1.0
    return len(set(actual[:k]).intersection(expected_set)) / len(expected_set)


def directory_sizes(path: Path) -> tuple[int, list[dict[str, Any]]]:
    if not path.exists():
        return 0, []
    rows = []
    total = 0
    for file_path in sorted(item for item in path.rglob("*") if item.is_file()):
        size = file_path.stat().st_size
        total += size
        rows.append({"path": file_path.relative_to(path).as_posix(), "bytes": size})
    return total, rows


def normalized_peak_rss_bytes() -> int:
    value = int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    return value if sys.platform == "darwin" else value * 1024


def command_output(arguments: list[str]) -> str | None:
    try:
        result = subprocess.run(
            arguments,
            check=True,
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip() or result.stderr.strip() or None


def source_revision(repo_root: Path) -> str:
    value = command_output(["git", "-C", str(repo_root), "rev-parse", "HEAD"])
    return value or "unknown"


def runtime_environment(repo_root: Path) -> dict[str, Any]:
    return {
        "architecture": platform.machine(),
        "cpu": command_output(["sysctl", "-n", "machdep.cpu.brand_string"]),
        "logical_cpu_count": os.cpu_count(),
        "machine": platform.machine(),
        "numpy_version": np.__version__,
        "os": platform.system(),
        "os_release": platform.release(),
        "physical_device_execution": False,
        "platform": platform.platform(),
        "python_executable": sys.executable,
        "python_implementation": platform.python_implementation(),
        "python_version": platform.python_version(),
        "repository_revision": source_revision(repo_root),
        "rustc_version": command_output(["rustc", "--version"]),
        "cargo_version": command_output(["cargo", "--version"]),
        "swift_version": command_output(["swift", "--version"]),
        "uv_version": command_output(["uv", "--version"]),
    }


def reset_directory(path: Path) -> None:
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
