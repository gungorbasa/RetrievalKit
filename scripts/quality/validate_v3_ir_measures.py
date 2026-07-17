#!/usr/bin/env python3
"""Cross-check V3 A-C and E-G retrieval metrics with pinned ir_measures."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import shutil
import sys
from pathlib import Path
from typing import Any

import ir_measures
from ir_measures import AP, Judged, P, RR, Recall, Success, nDCG

if __package__:
    from . import validate_v3_conformance as foundation
    from .validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from .validate_v3_phase_1_2b import canonical_bytes
else:
    import validate_v3_conformance as foundation
    from validate_v3_phase_1_2a import ValidationError, read_json, verify_frozen_fixture
    from validate_v3_phase_1_2b import canonical_bytes


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
REPORT_NAME = "ir-measures-cross-check.json"
REPORT_SCHEMA = "phase-1.2c-ir-measures-cross-check-v1"
TOLERANCE = 1.0e-9
MAPPINGS = (
    ("ap", AP),
    ("judged_at_10", Judged @ 10),
    ("judged_at_5", Judged @ 5),
    ("mrr_at_10", RR @ 10),
    ("ndcg_at_10", nDCG(gains={0: 0, 1: 1, 2: 3}) @ 10),
    ("ndcg_at_5", nDCG(gains={0: 0, 1: 1, 2: 3}) @ 5),
    ("precision_at_5", P @ 5),
    ("recall_at_10", Recall @ 10),
    ("recall_at_5", Recall @ 5),
    ("success_at_1", Success @ 1),
)


def frozen_runs(collection: Path) -> list[dict[str, Any]]:
    header = read_json(collection / "collection.json")
    files = {entry["path"]: (collection / entry["path"]).read_bytes() for entry in header["files"]}
    files["collection.json"] = (collection / "collection.json").read_bytes()
    return [
        run
        for run in foundation.derive_runs(files)
        if run["configuration"]["run_letter"] in {"a", "b", "c", "e", "f", "g"}
    ]


def rust_metrics(artifacts: Path) -> dict[str, dict[str, Any]]:
    baseline = read_json(artifacts / "metrics.json")
    scoped = read_json(artifacts / "graph-retrieval-metrics.json")
    return {row["run_id"]: row for row in baseline["runs"] + scoped["runs"]}


def query_metric(row: dict[str, Any], metric_name: str) -> float:
    value = row["metrics"][metric_name]
    if isinstance(value, dict):
        if value["status"] != "valid":
            raise ValidationError(
                f"Rust metric {metric_name} for {row['query_id']} is {value['status']}"
            )
        return float(value["value"])
    return float(value)


def aggregate_metric(run: dict[str, Any], metric_name: str) -> float:
    value = run["macro"][metric_name]
    if isinstance(value, dict):
        if value["value"] is None:
            raise ValidationError(f"Rust aggregate {metric_name} is undefined")
        return float(value["value"])
    return float(value)


def validate(collection: Path, artifacts: Path) -> dict[str, Any]:
    verify_frozen_fixture(collection)
    runs = frozen_runs(collection)
    if len(runs) != 12:
        raise ValidationError(f"expected 12 A-C/E-G runs, actual {len(runs)}")
    rust_by_id = rust_metrics(artifacts)
    qrels = list(ir_measures.read_trec_qrels(str(collection / "qrels.tsv")))
    rows = []
    maximum_query = 0.0
    maximum_aggregate = 0.0
    checked_query_metrics = 0
    for run in runs:
        run_id = run["run_id"]
        population = set(run["execution_population"])
        selected_qrels = [row for row in qrels if row.query_id in population]
        trec = list(
            ir_measures.read_trec_run(str(artifacts / "runs" / f"{run_id}.trec"))
        )
        measures = [measure for _, measure in MAPPINGS]
        independent_queries = {
            (row.query_id, str(row.measure)): float(row.value)
            for row in ir_measures.iter_calc(measures, selected_qrels, trec)
        }
        independent_aggregate = ir_measures.calc_aggregate(measures, selected_qrels, trec)
        rust_run = rust_by_id.get(run_id)
        if rust_run is None:
            raise ValidationError(f"missing Rust metrics for '{run_id}'")
        rust_queries = {
            row["query_id"]: row
            for row in rust_run["queries"]
            if row["query_id"] in population
        }
        if rust_queries.keys() != population:
            raise ValidationError(f"Rust query population mismatch for '{run_id}'")
        run_query_max = 0.0
        run_aggregate_max = 0.0
        for metric_name, measure in MAPPINGS:
            measure_name = str(measure)
            for query_id in sorted(population):
                key = (query_id, measure_name)
                if key not in independent_queries:
                    raise ValidationError(
                        f"ir_measures omitted {measure_name} for {run_id}/{query_id}"
                    )
                difference = abs(
                    independent_queries[key]
                    - query_metric(rust_queries[query_id], metric_name)
                )
                run_query_max = max(run_query_max, difference)
                maximum_query = max(maximum_query, difference)
                checked_query_metrics += 1
                if difference > TOLERANCE:
                    raise ValidationError(
                        f"{run_id}/{query_id}/{metric_name} differs by {difference}"
                    )
            difference = abs(
                float(independent_aggregate[measure])
                - aggregate_metric(rust_run, metric_name)
            )
            run_aggregate_max = max(run_aggregate_max, difference)
            maximum_aggregate = max(maximum_aggregate, difference)
            if difference > TOLERANCE:
                raise ValidationError(f"{run_id}/{metric_name} aggregate differs by {difference}")
        rows.append(
            {
                "aggregate_maximum_absolute_difference": run_aggregate_max,
                "query_count": len(population),
                "query_maximum_absolute_difference": run_query_max,
                "run_id": run_id,
            }
        )
    return {
        "artifact_schema": REPORT_SCHEMA,
        "checked_query_metrics": checked_query_metrics,
        "checked_runs": rows,
        "dependency": {
            "ir_measures": importlib.metadata.version("ir_measures"),
            "pytrec_eval_terrier": importlib.metadata.version("pytrec-eval-terrier"),
        },
        "maximum_absolute_differences": {
            "aggregate": maximum_aggregate,
            "per_query": maximum_query,
        },
        "metric_mappings": {
            metric_name: str(measure) for metric_name, measure in MAPPINGS
        },
        "partial": True,
        "publication_ready": False,
        "status": "passed",
        "tolerance": TOLERANCE,
        "trec_eval": {
            "available": shutil.which("trec_eval") is not None,
            "publication_gate": "closed" if shutil.which("trec_eval") else "remaining",
        },
        "unsupported_external_mappings": [
            "candidate and graph metrics",
            "complete-evidence recall",
            "supporting-document recall",
        ],
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite ir_measures report '{path}'")
    path.write_bytes(canonical_bytes(report) + b"\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--check-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        report = validate(args.collection.resolve(), args.artifacts.resolve())
        if not args.check_only:
            write_report(args.artifacts.resolve() / REPORT_NAME, report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
