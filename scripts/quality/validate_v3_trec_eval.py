#!/usr/bin/env python3
"""Cross-check all V3 ranked runs with the pinned official NIST trec_eval."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

if __package__:
    from . import bootstrap_v3_trec_eval as bootstrap
    from . import validate_v3_conformance as foundation
    from .validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from .validate_v3_phase_1_2b import canonical_bytes
else:
    import bootstrap_v3_trec_eval as bootstrap
    import validate_v3_conformance as foundation
    from validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from validate_v3_phase_1_2b import canonical_bytes


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
REPORT_NAME = "trec-eval-cross-check.json"
REPORT_SCHEMA = "v3-official-trec-eval-cross-check-v1"
TOLERANCE = 1.0e-9
RUN_LINE = re.compile(
    rb"([A-Za-z0-9][A-Za-z0-9._:-]{0,127}) Q0 "
    rb"([A-Za-z0-9][A-Za-z0-9._:-]{0,127}) ([1-9][0-9]*) ([1-9][0-9]*) "
    rb"([a-z0-9][a-z0-9-]{0,95})\n"
)
SUPPORTED_MAPPINGS = (
    ("ndcg_at_5", "ndcg_cut_5", "ndcg_cut.5", "qrel_gain_2^rel-1"),
    ("ndcg_at_10", "ndcg_cut_10", "ndcg_cut.10", "qrel_gain_2^rel-1"),
    ("recall_at_5", "recall_5", "recall.5", "direct"),
    ("recall_at_10", "recall_10", "recall.10", "direct"),
    ("precision_at_5", "P_5", "P.5", "direct"),
    ("mrr_at_10", "recip_rank", "recip_rank", "top_10_truncated_run"),
    ("ap", "map", "map", "per_query_ap_and_aggregate_map"),
    ("success_at_1", "success_1", "success.1", "direct"),
)
UNSUPPORTED_METRICS = (
    {"metric": "judged_at_5", "reason": "no exact official trec_eval measure"},
    {"metric": "judged_at_10", "reason": "no exact official trec_eval measure"},
    {
        "metric": "graph_and_evidence_metrics",
        "reason": "no exact official trec_eval measures",
    },
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_dependency_identity(identity_path: Path, binary: Path) -> dict[str, Any]:
    identity = read_json(identity_path)
    expected = {
        "archive_sha256": bootstrap.ARCHIVE_SHA256,
        "archive_url": bootstrap.ARCHIVE_URL,
        "upstream_commit": bootstrap.UPSTREAM_COMMIT,
        "upstream_url": bootstrap.UPSTREAM_URL,
        "version": bootstrap.UPSTREAM_VERSION,
    }
    for field, value in expected.items():
        if identity.get(field) != value:
            raise ValidationError(
                f"trec_eval dependency identity {field} mismatch: expected {value!r}, "
                f"actual {identity.get(field)!r}"
            )
    archive = identity_path.parent / "source.tar.gz"
    if not archive.is_file() or sha256(archive.read_bytes()) != bootstrap.ARCHIVE_SHA256:
        raise ValidationError("trec_eval source archive checksum mismatch")
    source = identity_path.parent / "source"
    try:
        source_sha256, source_files = bootstrap.source_tree_identity(source)
    except bootstrap.BootstrapError as error:
        raise ValidationError(str(error)) from error
    if source_sha256 != identity.get("source_tree_sha256"):
        raise ValidationError(
            "trec_eval source tree checksum mismatch: "
            f"expected {identity.get('source_tree_sha256')}, actual {source_sha256}"
        )
    if source_sha256 != bootstrap.SOURCE_TREE_SHA256:
        raise ValidationError(
            "trec_eval pinned source tree checksum mismatch: "
            f"expected {bootstrap.SOURCE_TREE_SHA256}, actual {source_sha256}"
        )
    if len(source_files) != identity.get("source_file_count"):
        raise ValidationError("trec_eval source tree file-count mismatch")
    try:
        executable_sha256 = bootstrap.verify_executable(binary)
    except bootstrap.BootstrapError as error:
        raise ValidationError(str(error)) from error
    if executable_sha256 != identity.get("executable_sha256"):
        raise ValidationError(
            "trec_eval executable checksum mismatch: "
            f"expected {identity.get('executable_sha256')}, actual {executable_sha256}"
        )
    compiler = identity.get("compiler")
    if not isinstance(compiler, dict) or set(compiler) != {"executable", "version"}:
        raise ValidationError("trec_eval compiler identity is missing or malformed")
    return identity


def frozen_ranked_runs(
    collection: Path, revision: dict[str, Any] | None = None
) -> list[dict[str, Any]]:
    header = read_json(collection / "collection.json")
    files = {
        entry["path"]: (collection / entry["path"]).read_bytes()
        for entry in header["files"]
    }
    files["collection.json"] = (collection / "collection.json").read_bytes()
    runs = [
        run
        for run in foundation.derive_runs(files, revision)
        if run["configuration"]["run_letter"] in {"a", "b", "c", "e", "f", "g"}
    ]
    if len(runs) != 12:
        raise ValidationError(f"expected 12 frozen ranked runs, actual {len(runs)}")
    return runs


def validate_run_inventory(actual: list[str], expected: list[str]) -> None:
    if actual != sorted(expected):
        missing = sorted(set(expected) - set(actual))
        extra = sorted(set(actual) - set(expected))
        raise ValidationError(f"ranked run inventory mismatch; missing {missing}, extra {extra}")


def parse_qrels(data: bytes) -> dict[str, list[bytes]]:
    rows: dict[str, list[bytes]] = {}
    previous: tuple[str, str] | None = None
    for line in data.splitlines(keepends=True):
        fields = line.removesuffix(b"\n").split(b" ")
        if len(fields) != 4 or fields[1] != b"0" or not fields[3].isdigit():
            raise ValidationError("qrels.tsv contains a malformed row")
        try:
            query_id = fields[0].decode("ascii")
            record_id = fields[2].decode("ascii")
        except UnicodeDecodeError as error:
            raise ValidationError("qrels.tsv contains a non-ASCII identifier") from error
        key = (query_id, record_id)
        if previous is not None and key <= previous:
            raise ValidationError("qrels.tsv is duplicated or reordered")
        previous = key
        rows.setdefault(query_id, []).append(line)
    return rows


def exponential_gain_qrel(line: bytes) -> bytes:
    fields = line.removesuffix(b"\n").split(b" ")
    relevance = int(fields[3])
    fields[3] = str((2**relevance) - 1).encode("ascii")
    return b" ".join(fields) + b"\n"


def parse_run(data: bytes, run_id: str) -> list[dict[str, Any]]:
    if data and not data.endswith(b"\n"):
        raise ValidationError(f"run '{run_id}' does not end in LF")
    offset = 0
    rows: list[dict[str, Any]] = []
    previous: tuple[str, int] | None = None
    seen_documents: set[tuple[str, str]] = set()
    for match in RUN_LINE.finditer(data):
        if match.start() != offset:
            raise ValidationError(f"run '{run_id}' contains a malformed row")
        offset = match.end()
        query_id, record_id, rank, score, tag = match.groups()
        query = query_id.decode("ascii")
        document = record_id.decode("ascii")
        actual_run_id = tag.decode("ascii")
        integer_rank = int(rank)
        integer_score = int(score)
        if actual_run_id != run_id:
            raise ValidationError(
                f"run '{run_id}' contains unexpected TREC tag '{actual_run_id}'"
            )
        key = (query, integer_rank)
        if previous is not None and key <= previous:
            raise ValidationError(f"run '{run_id}' is reordered")
        if previous is None or query != previous[0]:
            if integer_rank != 1:
                raise ValidationError(f"run '{run_id}' query '{query}' does not start at rank 1")
        elif integer_rank != previous[1] + 1:
            raise ValidationError(f"run '{run_id}' query '{query}' has nonconsecutive ranks")
        document_key = (query, document)
        if document_key in seen_documents:
            raise ValidationError(f"run '{run_id}' contains a duplicate row/document")
        seen_documents.add(document_key)
        rows.append(
            {
                "query_id": query,
                "record_id": document,
                "rank": integer_rank,
                "score": integer_score,
            }
        )
        previous = key
    if offset != len(data):
        raise ValidationError(f"run '{run_id}' contains a malformed row")
    return rows


def metric_value(value: Any, label: str) -> float:
    if isinstance(value, dict):
        if value.get("status") != "valid" or value.get("value") is None:
            raise ValidationError(f"Rust metric '{label}' is not valid")
        value = value["value"]
    result = float(value)
    if not math.isfinite(result):
        raise ValidationError(f"Rust metric '{label}' is not finite")
    return result


def aggregate_value(value: Any, label: str) -> float:
    if isinstance(value, dict):
        if value.get("value") is None:
            raise ValidationError(f"Rust aggregate '{label}' is undefined")
        value = value["value"]
    result = float(value)
    if not math.isfinite(result):
        raise ValidationError(f"Rust aggregate '{label}' is not finite")
    return result


def parse_trec_eval_output(
    output: str, measures: set[str], queries: set[str]
) -> dict[tuple[str, str], float]:
    observed: dict[tuple[str, str], float] = {}
    allowed_queries = queries | {"all"}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) != 3 or fields[0] not in measures or fields[1] not in allowed_queries:
            raise ValidationError(f"unexpected trec_eval output row '{line}'")
        key = (fields[0], fields[1])
        if key in observed:
            raise ValidationError(f"duplicate trec_eval output row {key}")
        try:
            value = float(fields[2])
        except ValueError as error:
            raise ValidationError(f"non-numeric trec_eval value in row '{line}'") from error
        if not math.isfinite(value):
            raise ValidationError(f"non-finite trec_eval value in row '{line}'")
        observed[key] = value
    expected = {(measure, query) for measure in measures for query in allowed_queries}
    missing = sorted(expected - observed.keys())
    unexpected = sorted(observed.keys() - expected)
    if missing:
        raise ValidationError(f"trec_eval output is missing rows {missing}")
    if unexpected:
        raise ValidationError(f"trec_eval output has unexpected rows {unexpected}")
    return observed


def run_official(
    command: list[str],
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> str:
    result = runner(command, capture_output=True, check=False, text=True)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValidationError(
            f"official trec_eval exited with status {result.returncode}: {detail}"
        )
    return result.stdout


def compare_run_metrics(
    run_id: str,
    rust_run: dict[str, Any],
    population: list[str],
    observed: dict[tuple[str, str], float],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], float, float]:
    rust_queries = {
        row["query_id"]: row
        for row in rust_run["queries"]
        if row["query_id"] in set(population)
    }
    if set(rust_queries) != set(population):
        raise ValidationError(f"Rust query population mismatch for '{run_id}'")
    query_checks: list[dict[str, Any]] = []
    aggregate_checks: list[dict[str, Any]] = []
    maximum_query = 0.0
    maximum_aggregate = 0.0
    for rust_name, official_name, _, _ in SUPPORTED_MAPPINGS:
        for query_id in population:
            wanted = metric_value(
                rust_queries[query_id]["metrics"][rust_name], f"{run_id}/{query_id}/{rust_name}"
            )
            actual = observed[(official_name, query_id)]
            difference = abs(wanted - actual)
            maximum_query = max(maximum_query, difference)
            query_checks.append(
                {
                    "absolute_difference": difference,
                    "metric": rust_name,
                    "official_value": actual,
                    "query_id": query_id,
                    "rust_value": wanted,
                    "status": "passed" if difference <= TOLERANCE else "failed",
                }
            )
            if difference > TOLERANCE:
                raise ValidationError(
                    f"{run_id}/{query_id}/{rust_name} differs by {difference}"
                )
        wanted = aggregate_value(rust_run["macro"][rust_name], f"{run_id}/{rust_name}")
        actual = observed[(official_name, "all")]
        difference = abs(wanted - actual)
        maximum_aggregate = max(maximum_aggregate, difference)
        aggregate_checks.append(
            {
                "absolute_difference": difference,
                "metric": rust_name,
                "official_value": actual,
                "rust_value": wanted,
                "status": "passed" if difference <= TOLERANCE else "failed",
            }
        )
        if difference > TOLERANCE:
            raise ValidationError(f"{run_id}/{rust_name} aggregate differs by {difference}")
    return query_checks, aggregate_checks, maximum_query, maximum_aggregate


def validate(
    collection: Path,
    artifacts: Path,
    binary: Path,
    identity_path: Path,
    implementation_revision: dict[str, Any] | None = None,
    runner: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> dict[str, Any]:
    verify_frozen_fixture(collection)
    dependency = load_dependency_identity(identity_path, binary)
    expected_runs = frozen_ranked_runs(collection, implementation_revision)
    expected_run_ids = [run["run_id"] for run in expected_runs]
    actual_run_files = sorted(path.stem for path in (artifacts / "runs").glob("*.trec"))
    validate_run_inventory(actual_run_files, expected_run_ids)

    metrics = read_json(artifacts / "metrics.json")["runs"]
    metrics += read_json(artifacts / "graph-retrieval-metrics.json")["runs"]
    results = read_json(artifacts / "rust-results.json")["runs"]
    results += read_json(artifacts / "graph-retrieval-rust-results.json")["runs"]
    metrics_by_id = {row["run_id"]: row for row in metrics}
    results_by_id = {row["run_id"]: row for row in results}
    if set(metrics_by_id) != set(expected_run_ids) or set(results_by_id) != set(expected_run_ids):
        raise ValidationError("Rust metrics/results ranked run population mismatch")
    qrels_by_query = parse_qrels((collection / "qrels.tsv").read_bytes())
    evaluation_depth = read_json(collection / "collection.json")["evaluation_depth"]
    report_runs = []
    maximum_query = 0.0
    maximum_aggregate = 0.0
    for expected_run in expected_runs:
        run_id = expected_run["run_id"]
        population = expected_run["execution_population"]
        metrics_run = metrics_by_id[run_id]
        results_run = results_by_id[run_id]
        if metrics_run["status"] != "valid" or results_run["status"] != "valid":
            raise ValidationError(f"run '{run_id}' is not valid")
        if metrics_run["execution_population_sha256"] != expected_run["execution_population_sha256"]:
            raise ValidationError(f"run '{run_id}' execution population hash mismatch")
        result_queries = {row["query_id"]: row for row in results_run["queries"]}
        if set(result_queries) != set(expected_run["declared_population"]):
            raise ValidationError(f"run '{run_id}' declared query population mismatch")
        expected_rows: list[tuple[str, str, int, int]] = []
        for query_id in population:
            query = result_queries[query_id]
            if query["execution_status"] != "valid":
                raise ValidationError(f"run '{run_id}' query '{query_id}' is not valid")
            for document in query["projected_documents"]:
                rank = document["document_rank"]
                expected_rows.append(
                    (query_id, document["record_id"], rank, evaluation_depth - rank + 1)
                )
        run_path = artifacts / "runs" / f"{run_id}.trec"
        parsed_rows = parse_run(run_path.read_bytes(), run_id)
        actual_rows = [
            (row["query_id"], row["record_id"], row["rank"], row["score"])
            for row in parsed_rows
        ]
        if actual_rows != expected_rows:
            raise ValidationError(f"run '{run_id}' rows differ from rank-derived Rust results")

        with tempfile.TemporaryDirectory(prefix="retrievalkit-v3-trec-eval-") as directory:
            temporary = Path(directory)
            selected_qrels = temporary / "qrels.tsv"
            selected_qrels.write_bytes(
                b"".join(
                    exponential_gain_qrel(line)
                    for query in population
                    for line in qrels_by_query[query]
                )
            )
            top_ten = temporary / "top-10.trec"
            top_ten.write_bytes(
                b"".join(
                    line
                    for line in run_path.read_bytes().splitlines(keepends=True)
                    if int(line.split(b" ")[3]) <= 10
                )
            )
            direct_measures = {mapping[1] for mapping in SUPPORTED_MAPPINGS if mapping[3] != "top_10_truncated_run"}
            direct_command = [
                str(binary),
                "-q",
                "-c",
                "-l1",
                "-m",
                "ndcg_cut.5,10",
                "-m",
                "recall.5,10",
                "-m",
                "P.5",
                "-m",
                "map",
                "-m",
                "success.1",
                str(selected_qrels),
                str(run_path),
            ]
            direct = parse_trec_eval_output(
                run_official(direct_command, runner), direct_measures, set(population)
            )
            reciprocal_command = [
                str(binary),
                "-q",
                "-c",
                "-l1",
                "-m",
                "recip_rank",
                str(selected_qrels),
                str(top_ten),
            ]
            reciprocal = parse_trec_eval_output(
                run_official(reciprocal_command, runner), {"recip_rank"}, set(population)
            )
        observed = direct | reciprocal
        query_checks, aggregate_checks, query_max, aggregate_max = compare_run_metrics(
            run_id, metrics_run, population, observed
        )
        maximum_query = max(maximum_query, query_max)
        maximum_aggregate = max(maximum_aggregate, aggregate_max)
        report_runs.append(
            {
                "aggregate_checks": aggregate_checks,
                "aggregate_maximum_absolute_difference": aggregate_max,
                "declared_population_sha256": expected_run["declared_population_sha256"],
                "execution_population_sha256": expected_run["execution_population_sha256"],
                "query_checks": query_checks,
                "query_count": len(population),
                "query_maximum_absolute_difference": query_max,
                "run_id": run_id,
                "status": "passed",
            }
        )
    return {
        "artifact_schema": REPORT_SCHEMA,
        "dependency": dependency,
        "maximum_absolute_differences": {
            "aggregate": maximum_aggregate,
            "per_query": maximum_query,
        },
        "metric_mappings": [
            {
                "official_measure": official_measure,
                "projection": projection,
                "rust_metric": rust_metric,
            }
            for rust_metric, _, official_measure, projection in SUPPORTED_MAPPINGS
        ],
        "runs": report_runs,
        "status": "passed",
        "tolerance": TOLERANCE,
        "unsupported_metrics": list(UNSUPPORTED_METRICS),
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite trec_eval report '{path}'")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_bytes(report) + b"\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--trec-eval", type=Path)
    parser.add_argument("--tool-identity", type=Path)
    parser.add_argument("--implementation-revision", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    tool_root = bootstrap.DEFAULT_TOOL_ROOT
    binary = (args.trec_eval or tool_root / "bin/trec_eval").resolve()
    identity = (args.tool_identity or tool_root / bootstrap.IDENTITY_NAME).resolve()
    artifacts = args.artifacts.resolve()
    output = (args.output or artifacts.parent / f"{artifacts.name}-{REPORT_NAME}").resolve()
    try:
        revision = (
            read_json(args.implementation_revision.resolve())
            if args.implementation_revision
            else None
        )
        report = validate(args.collection.resolve(), artifacts, binary, identity, revision)
        if not args.check_only:
            write_report(output, report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
