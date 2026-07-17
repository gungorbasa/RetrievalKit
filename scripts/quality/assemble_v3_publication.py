#!/usr/bin/env python3
"""Assemble an atomic canonical V3 public artifact from a release execution."""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import locale
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

if __package__:
    from . import bootstrap_v3_trec_eval as trec_bootstrap
    from . import validate_v3_conformance as foundation
    from . import validate_v3_trec_eval
    from .validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from .validate_v3_phase_1_2b import canonical_bytes
else:
    import bootstrap_v3_trec_eval as trec_bootstrap
    import validate_v3_conformance as foundation
    import validate_v3_trec_eval
    from validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from validate_v3_phase_1_2b import canonical_bytes


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
METRIC_NAMES = (
    "ap",
    "candidate_complete_evidence",
    "candidate_recall",
    "candidate_reduction_ratio",
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "empty_scope",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "path_accuracy",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
    "truncated",
    "truncated_max_hops",
    "truncated_max_results",
    "truncated_max_visited",
    "truncated_max_working_bytes",
)
RETRIEVAL_METRICS = {
    "ap",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
}
EVIDENCE_METRICS = {
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
}
EXCLUSION_REASONS = (
    "derived_seed_ambiguous",
    "derived_seed_no_match",
    "duplicate_identity",
    "filter_label_conflict",
    "invalid_upstream_record",
    "missing_complete_evidence",
    "no_relevant_documents",
    "not_in_frozen_corpus",
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]


def write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_bytes(value) + b"\n")


def checked_command(command: list[str], cwd: Path = ROOT) -> str:
    result = subprocess.run(command, cwd=cwd, capture_output=True, check=False, text=True)
    if result.returncode != 0:
        raise ValidationError(
            f"command failed ({' '.join(command)}): "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def release_revision(executable: Path) -> dict[str, Any]:
    if checked_command(["git", "status", "--porcelain=v1", "--untracked-files=all"]):
        raise ValidationError("publication requires a clean worktree")
    commit = checked_command(["git", "rev-parse", "HEAD"])
    if len(commit) != 40 or any(character not in "0123456789abcdef" for character in commit):
        raise ValidationError(f"invalid clean Git commit '{commit}'")
    if not executable.is_file() or executable.is_symlink():
        raise ValidationError(f"release executable is not a regular file: '{executable}'")
    return {
        "binary_sha256": sha256(executable.read_bytes()),
        "git_commit": commit,
        "source_sha256": None,
    }


def cpu_features() -> list[str]:
    if sys.platform == "darwin":
        output = checked_command(["sysctl", "-a"])
        prefixes = ("hw.optional.arm.", "hw.optional.arm64", "machdep.cpu.features")
        features = []
        for line in output.splitlines():
            if not line.startswith(prefixes) or ":" not in line:
                continue
            key, value = line.split(":", 1)
            value = value.strip()
            if value == "1":
                features.append(key)
            elif key == "machdep.cpu.features":
                features.extend(f"machdep.cpu.features/{item}" for item in value.split())
        return sorted(set(features))
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.is_file():
        features = set()
        for line in cpuinfo.read_text(encoding="utf-8").splitlines():
            if line.lower().startswith(("features", "flags")) and ":" in line:
                features.update(line.split(":", 1)[1].split())
        return sorted(features)
    return []


def verify_floating_point_mode() -> None:
    library = ctypes.CDLL(None)
    function = getattr(library, "fegetround", None)
    if function is None:
        raise ValidationError("runtime does not expose fegetround")
    function.restype = ctypes.c_int
    if function() != 0:
        raise ValidationError("floating-point mode is not round-to-nearest ties-to-even")


def determinism_identity(executable: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    locale.setlocale(locale.LC_ALL, "C")
    if locale.setlocale(locale.LC_ALL) != "C":
        raise ValidationError("failed to set the C locale")
    verify_floating_point_mode()
    rustc = checked_command(["rustc", "--version", "--verbose"])
    values = dict(
        line.split(": ", 1) for line in rustc.splitlines() if ": " in line
    )
    target = values.get("host")
    release = values.get("release")
    if not target or not release:
        raise ValidationError("rustc did not report host and release identity")
    if target.split("-", 1)[0] != platform.machine():
        aliases = {("aarch64", "arm64"), ("x86_64", "AMD64")}
        if (target.split("-", 1)[0], platform.machine()) not in aliases:
            raise ValidationError("target triple does not match executable host architecture")
    if sys.platform == "darwin":
        os_build = checked_command(["sw_vers", "-buildVersion"])
    else:
        os_build = platform.platform()
    environment = {
        "cpu_architecture": platform.machine(),
        "cpu_features": cpu_features(),
        "execution_threads": 1,
        "floating_point_mode": "round_to_nearest_ties_to_even",
        "locale": "C",
        "os_build": os_build,
        "runtime_flags": [],
    }
    context = {
        "binary_sha256": sha256(executable.read_bytes()),
        "environment_sha256": sha256(canonical_bytes(environment)),
        "runtime_id": "rustc",
        "runtime_version": release,
        "target_triple": target,
    }
    return environment, context


def collection_files(collection: Path) -> dict[str, bytes]:
    header = read_json(collection / "collection.json")
    files = {
        row["path"]: (collection / row["path"]).read_bytes() for row in header["files"]
    }
    files["collection.json"] = (collection / "collection.json").read_bytes()
    return files


def evidence_scores(
    ranking: list[str], alternatives: list[list[str]], cutoff: int
) -> tuple[float, float, int, int]:
    returned = set(ranking[:cutoff])
    candidates = []
    for evidence_set in alternatives:
        matched = len(returned & set(evidence_set))
        candidates.append((matched, len(evidence_set), evidence_set))
    candidates.sort(key=lambda row: (-row[0] / row[1], -row[0], row[1], row[2]))
    matched, required, _ = candidates[0]
    return matched / required, float(matched == required), matched, required


def macro(metrics: list[dict[str, dict[str, Any]]], name: str) -> dict[str, Any]:
    counts = {
        "excluded_pre_freeze": 0,
        "invalid_execution": 0,
        "not_applicable": 0,
        "undefined": 0,
        "valid": 0,
    }
    numerator = 0.0
    for row in metrics:
        value = row[name]
        counts[value["status"]] += 1
        if value["status"] == "valid":
            numerator += float(value["value"])
    denominator = counts["valid"]
    return {
        "denominator": denominator,
        "numerator": numerator,
        "status_counts": counts,
        "value": numerator / denominator if denominator else None,
    }


def closed_baseline_runs(collection: Path, qualification: Path) -> list[dict[str, Any]]:
    source_metrics = read_json(qualification / "metrics.json")["runs"]
    source_results = {
        row["run_id"]: row for row in read_json(qualification / "rust-results.json")["runs"]
    }
    queries = {row["query_id"]: row for row in read_jsonl(collection / "queries.jsonl")}
    evidence = {
        row["query_id"]: row["evidence_sets"]
        for row in read_jsonl(collection / "evidence-judgments.jsonl")
    }
    output = []
    for source in source_metrics:
        results = {row["query_id"]: row for row in source_results[source["run_id"]]["queries"]}
        query_rows = []
        micro = {
            5: {"matched": 0, "required": 0},
            10: {"matched": 0, "required": 0},
        }
        for existing in source["queries"]:
            query_id = existing["query_id"]
            if existing["execution_status"] != "valid":
                raise ValidationError(f"baseline {source['run_id']}/{query_id} is invalid")
            values = {}
            for name in METRIC_NAMES:
                if name in RETRIEVAL_METRICS:
                    values[name] = {"status": "valid", "value": existing["metrics"][name]}
                else:
                    values[name] = {"status": "not_applicable", "value": None}
            if "evidence" in queries[query_id]["tasks"]:
                ranking = [row["record_id"] for row in results[query_id]["projected_documents"]]
                for cutoff in (5, 10):
                    recall, complete, matched, required = evidence_scores(
                        ranking, evidence[query_id], cutoff
                    )
                    values[f"supporting_document_recall_at_{cutoff}"] = {
                        "status": "valid",
                        "value": recall,
                    }
                    values[f"complete_evidence_recall_at_{cutoff}"] = {
                        "status": "valid",
                        "value": complete,
                    }
                    micro[cutoff]["matched"] += matched
                    micro[cutoff]["required"] += required
            query_rows.append(
                {
                    "candidate_counts": None,
                    "execution_status": "valid",
                    "metrics": values,
                    "query_id": query_id,
                }
            )
        macros = {name: macro([row["metrics"] for row in query_rows], name) for name in METRIC_NAMES}
        empty_graph_rate = {"empty_scopes": 0, "graph_valid_queries": 0, "value": None}
        empty_truncation = {"affected_queries": 0, "graph_valid_queries": 0, "value": None}
        micro_values: dict[str, Any] = {
            "candidate_recall": {"matched_documents": 0, "required_documents": 0, "value": None},
            "candidate_reduction_ratio": {"candidate_chunks": 0, "eligible_chunks": 0, "value": None},
            "empty_scope_rate": empty_graph_rate,
        }
        for cutoff in (5, 10):
            matched = micro[cutoff]["matched"]
            required = micro[cutoff]["required"]
            micro_values[f"supporting_document_recall_at_{cutoff}"] = {
                "matched_documents": matched,
                "required_documents": required,
                "value": matched / required if required else None,
            }
        for name in (
            "truncation_rate",
            "truncation_rate_max_hops",
            "truncation_rate_max_results",
            "truncation_rate_max_visited",
            "truncation_rate_max_working_bytes",
        ):
            micro_values[name] = empty_truncation.copy()
        count = len(query_rows)
        output.append(
            {
                "counts": {
                    "attempted": count,
                    "declared": count,
                    "excluded_pre_freeze": 0,
                    "invalid_execution": 0,
                    "valid_execution": count,
                },
                "declared_population_sha256": source["declared_population_sha256"],
                "execution_population_sha256": source["execution_population_sha256"],
                "macro": macros,
                "micro": micro_values,
                "queries": query_rows,
                "run_id": source["run_id"],
                "status": "valid",
            }
        )
    return output


def exclusion_summary(collection: Path) -> dict[str, Any]:
    rows = read_jsonl(collection / "exclusions.jsonl")
    policies = {
        row["derived_seed_policy_id"]
        for row in read_jsonl(collection / "queries.jsonl")
        if row["derived_seed_policy_id"] is not None
    }
    lanes = ["global", *sorted(policies)]
    return {
        "by_lane": [
            {"count": sum(row["lane"] == lane for row in rows), "lane": lane}
            for lane in sorted(lanes)
        ],
        "by_reason": [
            {"count": sum(row["reason"] == reason for row in rows), "reason": reason}
            for reason in EXCLUSION_REASONS
        ],
        "total": len(rows),
    }


def seed_coverage(collection: Path) -> list[dict[str, Any]]:
    queries = read_jsonl(collection / "queries.jsonl")
    exclusions = read_jsonl(collection / "exclusions.jsonl")
    policies = sorted(
        {row["derived_seed_policy_id"] for row in queries if row["derived_seed_policy_id"]}
    )
    output = []
    for policy in policies:
        declared = sum(row["derived_seed_policy_id"] == policy for row in queries)
        failed = sum(row["lane"] == policy for row in exclusions)
        successful = declared - failed
        output.append(
            {
                "declared": declared,
                "failed": failed,
                "policy_id": policy,
                "successful": successful,
                "value": successful / declared,
            }
        )
    return output


def assemble_results(collection: Path, qualification: Path) -> dict[str, Any]:
    baseline = read_json(qualification / "rust-results.json")
    graph = read_json(qualification / "graph-rust-results.json")
    scoped = read_json(qualification / "graph-retrieval-rust-results.json")
    runs = sorted(baseline["runs"] + graph["runs"] + scoped["runs"], key=lambda row: row["run_id"])
    if len(runs) != 15 or any(row["status"] != "valid" for row in runs):
        raise ValidationError("publication requires 15 valid A-G Rust result runs")
    return {
        "collection_id": baseline["collection_id"],
        "collection_version": baseline["collection_version"],
        "runs": runs,
        "schema_version": 3,
        "seed_resolutions": graph["seed_resolutions"],
    }


def assemble_metrics(collection: Path, qualification: Path) -> dict[str, Any]:
    baseline = closed_baseline_runs(collection, qualification)
    graph = read_json(qualification / "graph-metrics.json")["runs"]
    scoped = read_json(qualification / "graph-retrieval-metrics.json")
    runs = sorted(baseline + graph + scoped["runs"], key=lambda row: row["run_id"])
    if len(runs) != 15 or any(row["status"] != "valid" for row in runs):
        raise ValidationError("publication requires 15 valid A-G metric runs")
    return {
        "collection_id": scoped["collection_id"],
        "collection_version": scoped["collection_version"],
        "exclusions": exclusion_summary(collection),
        "metric_definition_version": "graph-retrieval-v3-r2",
        "paired_comparisons": scoped["paired_comparisons"],
        "publication_status": "valid",
        "runs": runs,
        "schema_version": 3,
        "seed_resolution_coverage": seed_coverage(collection),
    }


def validate_qualification_gates(qualification: Path) -> None:
    marker = read_json(qualification / "qualification.json")
    if marker.get("status") != "qualification_only_no_final_manifest":
        raise ValidationError("release qualification is not valid")
    for name in (
        "graph-persistence-validation.json",
        "graph-retrieval-persistence-validation.json",
        "graph-retrieval-selection-path-equality.json",
    ):
        if read_json(qualification / name).get("status") != "valid":
            raise ValidationError(f"publication gate '{name}' did not pass")


def build_manifest(
    collection: Path,
    staging: Path,
    revision: dict[str, Any],
    environment: dict[str, Any],
    context: dict[str, Any],
) -> dict[str, Any]:
    files = collection_files(collection)
    runs = foundation.derive_runs(files, revision)
    fingerprints = foundation.derive_generation_fingerprints(files, runs)
    fingerprint_by_run = {
        row["run_id"]: row["fingerprint"] for row in fingerprints["bindings"]
    }
    run_configurations = []
    for run in runs:
        run_configurations.append(
            {
                "configuration": run["configuration"],
                "declared_population_sha256": run["declared_population_sha256"],
                "execution_population_sha256": run["execution_population_sha256"],
                "generation_fingerprint": fingerprint_by_run.get(run["run_id"]),
                "logical_run_sha256": run["logical_run_sha256"],
                "run_id": run["run_id"],
            }
        )
    indexed = []
    for path in sorted(path for path in staging.rglob("*") if path.is_file()):
        relative = path.relative_to(staging).as_posix()
        data = path.read_bytes()
        indexed.append({"bytes": len(data), "path": relative, "sha256": sha256(data)})
    if len(indexed) != 43:
        raise ValidationError(f"pre-manifest inventory expected 43 files, actual {len(indexed)}")
    expected_paths = [row["path"] for row in indexed]
    metrics = read_json(staging / "metrics.json")
    header = read_json(collection / "collection.json")
    return {
        "collection_id": header["collection_id"],
        "collection_version": header["collection_version"],
        "determinism_context": context,
        "determinism_environment": environment,
        "deterministic_files": expected_paths,
        "files": indexed,
        "generation_fingerprints": fingerprints["preimages"],
        "implementation_revision": revision,
        "metric_definition_version": metrics["metric_definition_version"],
        "population_hashes": [
            {
                "declared": run["declared_population_sha256"],
                "execution": run["execution_population_sha256"],
                "run_id": run["run_id"],
            }
            for run in runs
        ],
        "profile": "deterministic_quality",
        "publication_status": "valid",
        "run_configurations": run_configurations,
        "schema_version": 3,
    }


def assemble(
    collection: Path,
    qualification: Path,
    output: Path,
    revision: dict[str, Any],
    environment: dict[str, Any],
    context: dict[str, Any],
    pre_finalize: Callable[[Path], None] | None = None,
) -> dict[str, Any]:
    if output.exists():
        raise ValidationError(f"refusing to overwrite public artifact root '{output}'")
    validate_qualification_gates(qualification)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f".{output.name}-staging-", dir=output.parent) as directory:
        staging = Path(directory) / "artifacts"
        staging.mkdir()
        for name in (
            "qrels.tsv",
            "evidence-judgments.jsonl",
            "expected-paths.jsonl",
            "exclusions.jsonl",
        ):
            shutil.copyfile(collection / name, staging / name)
        for directory_name in ("runs", "graph-selections", "graph-paths"):
            shutil.copytree(qualification / directory_name, staging / directory_name)
        write_json(staging / "rust-results.json", assemble_results(collection, qualification))
        write_json(staging / "metrics.json", assemble_metrics(collection, qualification))
        (staging / "timing-samples.jsonl").write_bytes(
            b'{"profile":"deterministic_quality","status":"not_measured"}\n'
        )
        manifest = build_manifest(collection, staging, revision, environment, context)
        write_json(staging / "manifest.json", manifest)
        regular = [path for path in staging.rglob("*") if path.is_file()]
        directories = sorted(
            path.relative_to(staging).as_posix() for path in staging.rglob("*") if path.is_dir()
        )
        if len(regular) != 44 or directories != ["graph-paths", "graph-selections", "runs"]:
            raise ValidationError("final public artifact inventory is not the exact 44-file layout")
        if pre_finalize is not None:
            pre_finalize(staging)
        os.replace(staging, output)
    all_files = []
    for path in sorted(path for path in output.rglob("*") if path.is_file()):
        data = path.read_bytes()
        all_files.append(
            {"bytes": len(data), "path": path.relative_to(output).as_posix(), "sha256": sha256(data)}
        )
    return {
        "artifact_set_sha256": sha256(canonical_bytes(all_files)),
        "file_count": len(all_files),
        "manifest_file_count": len(manifest["files"]),
        "run_ids": [row["run_id"] for row in manifest["run_configurations"]],
        "status": "passed",
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--qualification", type=Path)
    parser.add_argument("--qualification-output", type=Path)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trec-eval", type=Path)
    parser.add_argument("--tool-identity", type=Path)
    parser.add_argument("--gate-report-root", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        collection = args.collection.resolve()
        verify_frozen_fixture(collection)
        executable = args.executable.resolve()
        revision = release_revision(executable)
        environment, context = determinism_identity(executable)
        if context["binary_sha256"] != revision["binary_sha256"]:
            raise ValidationError("release executable identity changed during inspection")
        qualification = args.qualification.resolve() if args.qualification else None
        if qualification is None:
            if args.qualification_output is None:
                raise ValidationError("--qualification-output is required when executing a release")
            qualification = args.qualification_output.resolve()
            environment_variables = os.environ.copy()
            environment_variables.update({"LC_ALL": "C", "LANG": "C"})
            result = subprocess.run(
                [
                    str(executable),
                    "bench",
                    "quality-v3",
                    "--collection",
                    str(collection),
                    "--release-qualification-artifacts",
                    str(qualification),
                ],
                cwd=ROOT,
                env=environment_variables,
                capture_output=True,
                check=False,
                text=True,
            )
            if result.returncode != 0:
                raise ValidationError(
                    f"release execution failed: {result.stderr.strip() or result.stdout.strip()}"
                )
            execution = json.loads(result.stdout)
            if execution.get("implementation_revision") != revision:
                raise ValidationError("release executable reported a different implementation revision")
        gate_root = (args.gate_report_root or qualification.parent).resolve()
        gate_root.mkdir(parents=True, exist_ok=True)
        revision_path = gate_root / f"{qualification.name}-implementation-revision.json"
        if revision_path.exists():
            raise ValidationError(f"refusing to overwrite release revision '{revision_path}'")
        write_json(revision_path, revision)
        trec_report = validate_v3_trec_eval.validate(
            collection,
            qualification,
            (args.trec_eval or trec_bootstrap.DEFAULT_TOOL_ROOT / "bin/trec_eval").resolve(),
            (args.tool_identity or trec_bootstrap.DEFAULT_TOOL_ROOT / trec_bootstrap.IDENTITY_NAME).resolve(),
            revision,
        )
        trec_path = gate_root / f"{qualification.name}-trec-eval-cross-check.json"
        validate_v3_trec_eval.write_report(trec_path, trec_report)
        try:
            if __package__:
                from . import validate_v3_ir_measures
            else:
                import validate_v3_ir_measures
        except ImportError as error:
            raise ValidationError(
                "pinned ir_measures==0.4.3 is required for publication assembly"
            ) from error
        ir_report = validate_v3_ir_measures.validate(collection, qualification, revision)
        ir_path = gate_root / f"{qualification.name}-ir-measures-cross-check.json"
        validate_v3_ir_measures.write_report(ir_path, ir_report)
        if __package__:
            from . import validate_v3_publication
        else:
            import validate_v3_publication

        def independently_validate(staging: Path) -> None:
            validate_v3_publication.validate(
                collection, staging, executable, trec_path, ir_path
            )

        result = assemble(
            collection,
            qualification,
            args.output.resolve(),
            revision,
            environment,
            context,
            independently_validate,
        )
        result.update(
            {
                "determinism_context": context,
                "determinism_environment": environment,
                "implementation_revision": revision,
                "ir_measures_report": str(ir_path),
                "trec_eval_report": str(trec_path),
            }
        )
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
