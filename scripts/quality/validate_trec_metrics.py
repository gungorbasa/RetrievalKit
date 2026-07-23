#!/usr/bin/env python3
"""Cross-check RetrievalKit's deterministic metrics with ir_measures and trec_eval."""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import tempfile
from pathlib import Path
from typing import Any


METRIC_FIELDS = {
    "nDCG@5": "ndcg_at_5",
    "nDCG@10": "ndcg_at_10",
    "R@5": "recall_at_5",
    "R@10": "recall_at_10",
    "Success@1": "success_at_1",
    "P@5": "precision_at_5",
    "RR@10": "mrr_at_10",
    "AP": "average_precision",
    "Judged@5": "judged_at_5",
    "Judged@10": "judged_at_10",
}

MAP_METRIC_FIELDS = {
    "nDCG": "ndcg_at",
    "R": "recall_at",
    "P": "precision_at",
}

# Official trec_eval prints four digits after the decimal point. Allow one-half
# unit in the final printed place plus a small float-parsing margin.
TREC_EVAL_ROUNDING_TOLERANCE = 5.1e-5


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--tolerance", type=float, default=1e-9)
    parser.add_argument(
        "--trec-eval",
        type=Path,
        help="Optional path to the official trec_eval binary.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Defaults to <artifacts>/external-validation.json.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if not math.isfinite(args.tolerance) or args.tolerance < 0:
        raise SystemExit("--tolerance must be a finite non-negative number")
    try:
        import ir_measures
        from ir_measures import AP, Judged, P, R, RR, Success, nDCG
    except ImportError as error:
        raise SystemExit(
            "ir_measures is required; install scripts/quality/requirements.txt"
        ) from error

    metrics_path = args.artifacts / "metrics.json"
    qrels_path = args.artifacts / "qrels.tsv"
    runs_path = args.artifacts / "runs"
    rust = json.loads(metrics_path.read_text(encoding="utf-8"))
    if rust.get("schema_version") != 1:
        raise SystemExit(f"unsupported metrics schema in {metrics_path}")

    gains = {grade: (2**grade) - 1 for grade in range(128)}
    measures_by_name = {}
    measures = [
        nDCG(gains=gains) @ 5,
        nDCG(gains=gains) @ 10,
        R(rel=1) @ 5,
        R(rel=1) @ 10,
        Success(rel=1) @ 1,
        P(rel=1) @ 5,
        RR(rel=1) @ 10,
        AP(rel=1),
        Judged @ 5,
        Judged @ 10,
    ]
    cutoffs = [int(cutoff) for cutoff in rust.get("beir_cutoffs", [])]
    for cutoff in cutoffs:
        measures.extend(
            [
                nDCG(gains=gains) @ cutoff,
                R(rel=1) @ cutoff,
                P(rel=1) @ cutoff,
            ]
        )
    for measure in measures:
        measures_by_name[canonical_metric_name(str(measure))] = measure
    measures = list(measures_by_name.values())
    # ir_measures accepts strings and file objects. pathlib.Path is iterable to
    # some reader implementations and can be silently interpreted as no rows.
    qrels = list(ir_measures.read_trec_qrels(str(qrels_path)))
    if not qrels:
        raise SystemExit(f"no qrels found in {qrels_path}")
    comparisons: list[dict[str, Any]] = []
    failures: list[str] = []

    for rust_run in rust["runs"]:
        run_id = rust_run["run_id"]
        run_path = runs_path / f"{run_id}.trec"
        run = list(ir_measures.read_trec_run(str(run_path)))
        if not run:
            raise SystemExit(f"no run rows found in {run_path}")
        expected_by_query = {item["query_id"]: item for item in rust_run["queries"]}
        observed_by_query: dict[str, dict[str, float]] = {}
        for metric in ir_measures.iter_calc(measures, qrels, run):
            observed_by_query.setdefault(metric.query_id, {})[
                canonical_metric_name(str(metric.measure))
            ] = float(metric.value)

        run_differences = compare_run(
            run_id,
            rust_run["aggregate"],
            expected_by_query,
            observed_by_query,
            cutoffs,
            args.tolerance,
        )
        if args.trec_eval is not None:
            run_differences.extend(
                validate_with_trec_eval(
                    args.trec_eval,
                    qrels_path,
                    run_path,
                    rust_run,
                    max(args.tolerance, TREC_EVAL_ROUNDING_TOLERANCE),
                )
            )
        failures.extend(run_differences)
        comparisons.append(
            {
                "run_id": run_id,
                "passed": not run_differences,
                "differences": run_differences,
            }
        )

    report = {
        "schema_version": 1,
        "provider": "ir_measures",
        "trec_eval": str(args.trec_eval) if args.trec_eval is not None else None,
        "passed": not failures,
        "tolerance": args.tolerance,
        "runs": comparisons,
        "failures": failures,
    }
    output = args.output or args.artifacts / "external-validation.json"
    output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if failures:
        raise SystemExit("\n".join(failures))
    print(f"Validated {len(comparisons)} runs; wrote {output}")


def canonical_metric_name(value: str) -> str:
    if value.startswith("nDCG"):
        return f"nDCG@{value.rsplit('@', 1)[1]}"
    return value.replace("(rel=1)", "")


def compare_run(
    run_id: str,
    aggregate: dict[str, float],
    expected_by_query: dict[str, dict[str, Any]],
    observed_by_query: dict[str, dict[str, float]],
    cutoffs: list[int],
    tolerance: float,
) -> list[str]:
    failures: list[str] = []
    for query_id, expected in expected_by_query.items():
        observed = observed_by_query.get(query_id, {})
        for external_name, field in METRIC_FIELDS.items():
            actual = observed.get(external_name, 0.0)
            wanted = float(expected[field])
            if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
                failures.append(
                    f"{run_id}/{query_id}/{field}: Rust={wanted:.17g} "
                    f"ir_measures={actual:.17g}"
                )
        for external_prefix, field in MAP_METRIC_FIELDS.items():
            for cutoff in cutoffs:
                actual = observed.get(f"{external_prefix}@{cutoff}", 0.0)
                wanted = float(expected[field][str(cutoff)])
                if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
                    failures.append(
                        f"{run_id}/{query_id}/{field}[{cutoff}]: "
                        f"Rust={wanted:.17g} ir_measures={actual:.17g}"
                    )
    for external_name, field in METRIC_FIELDS.items():
        values = [
            observed_by_query.get(query_id, {}).get(external_name, 0.0)
            for query_id in expected_by_query
        ]
        actual = sum(values) / len(values)
        wanted = float(aggregate[field])
        if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
            failures.append(
                f"{run_id}/all/{field}: Rust={wanted:.17g} ir_measures={actual:.17g}"
            )
    for external_prefix, field in MAP_METRIC_FIELDS.items():
        for cutoff in cutoffs:
            values = [
                observed_by_query.get(query_id, {}).get(
                    f"{external_prefix}@{cutoff}", 0.0
                )
                for query_id in expected_by_query
            ]
            actual = sum(values) / len(values)
            wanted = float(aggregate[field][str(cutoff)])
            if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
                failures.append(
                    f"{run_id}/all/{field}[{cutoff}]: Rust={wanted:.17g} "
                    f"ir_measures={actual:.17g}"
                )
    return failures


def validate_with_trec_eval(
    binary: Path,
    qrels_path: Path,
    run_path: Path,
    rust_run: dict[str, Any],
    tolerance: float,
) -> list[str]:
    """Validate binary metrics supported without custom gain mappings."""
    with tempfile.TemporaryDirectory(prefix="retrievalkit-trec-eval-") as directory:
        top_ten = Path(directory) / "top-ten.trec"
        ranks: dict[str, int] = {}
        selected: list[str] = []
        for line in run_path.read_text(encoding="utf-8").splitlines():
            query_id = line.split()[0]
            ranks[query_id] = ranks.get(query_id, 0) + 1
            if ranks[query_id] <= 10:
                selected.append(line)
        top_ten.write_text("\n".join(selected) + "\n", encoding="utf-8")
        command = [
            str(binary),
            "-q",
            "-c",
            "-m",
            "P.5",
            "-m",
            "recall.5",
            "-m",
            "recip_rank",
            str(qrels_path),
            str(top_ten),
        ]
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        if result.returncode != 0:
            return [
                f"{rust_run['run_id']}: trec_eval failed: "
                f"{result.stderr.strip() or result.stdout.strip()}"
            ]
        observed: dict[tuple[str, str], float] = {}
        for line in result.stdout.splitlines():
            fields = line.split()
            if len(fields) == 3:
                observed[(fields[0], fields[1])] = float(fields[2])
        mapping = {
            "P_5": "precision_at_5",
            "recall_5": "recall_at_5",
            "recip_rank": "mrr_at_10",
        }
        failures: list[str] = []
        for query in rust_run["queries"]:
            for trec_name, field in mapping.items():
                actual = observed.get((trec_name, query["query_id"]), 0.0)
                wanted = float(query[field])
                if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
                    failures.append(
                        f"{rust_run['run_id']}/{query['query_id']}/{field}: "
                        f"Rust={wanted:.17g} trec_eval={actual:.17g}"
                    )
        for trec_name, field in mapping.items():
            actual = observed.get((trec_name, "all"), 0.0)
            wanted = float(rust_run["aggregate"][field])
            if not math.isclose(actual, wanted, rel_tol=0.0, abs_tol=tolerance):
                failures.append(
                    f"{rust_run['run_id']}/all/{field}: "
                    f"Rust={wanted:.17g} trec_eval={actual:.17g}"
                )
        return failures


if __name__ == "__main__":
    main()
