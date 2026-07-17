#!/usr/bin/env python3
"""Cross-check HotpotQA Phase 3a with ir_measures and official trec_eval."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import tempfile
from pathlib import Path
from typing import Any

import ir_measures
from ir_measures import AP, Judged, P, RR, Recall, Success, nDCG

import validate_hotpotqa_phase_3 as phase3
import validate_v3_trec_eval as official

TOLERANCE = 1.0e-9
IR_MAPPINGS = (
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


def rust_metrics(matrix: Path) -> dict[str, dict[str, Any]]:
    baseline = phase3.read_json(matrix / "metrics.json")["runs"]
    scoped = phase3.read_json(matrix / "graph-retrieval-metrics.json")["runs"]
    return {row["run_id"]: row for row in baseline + scoped}


def compare(wanted: float, actual: float, label: str, maximum: float) -> float:
    difference = abs(wanted - actual)
    if difference > TOLERANCE:
        raise phase3.ValidationError(f"{label} differs by {difference}")
    return max(maximum, difference)


def ir_cross_check(collection: Path, matrix: Path) -> dict[str, Any]:
    version = importlib.metadata.version("ir_measures")
    if version != "0.4.3":
        raise phase3.ValidationError(f"ir_measures expected 0.4.3, actual {version}")
    run_ids = {
        row["letter"]: row["run_id"]
        for row in phase3.read_json(matrix / "phase-3a-development-matrix.json")[
            "run_ids"
        ]
        if row["letter"] != "d"
    }
    metrics = rust_metrics(matrix)
    qrels = list(ir_measures.read_trec_qrels(str(collection / "qrels.tsv")))
    maximum_query = 0.0
    maximum_aggregate = 0.0
    reports = []
    measures = [measure for _, measure in IR_MAPPINGS]
    for letter in "abcefg":
        run_id = run_ids[letter]
        run = metrics[run_id]
        population = {
            row["query_id"]
            for row in run["queries"]
            if row["execution_status"] == "valid"
        }
        selected_qrels = [row for row in qrels if row.query_id in population]
        trec = list(ir_measures.read_trec_run(str(matrix / "runs" / f"{run_id}.trec")))
        independent_queries = {
            (row.query_id, str(row.measure)): float(row.value)
            for row in ir_measures.iter_calc(measures, selected_qrels, trec)
        }
        independent_aggregate = ir_measures.calc_aggregate(
            measures, selected_qrels, trec
        )
        rust_queries = {row["query_id"]: row for row in run["queries"]}
        run_query_max = 0.0
        run_aggregate_max = 0.0
        for metric_name, measure in IR_MAPPINGS:
            for query_id in sorted(population):
                wanted = float(
                    phase3.metric_value(rust_queries[query_id]["metrics"][metric_name])
                )
                actual = independent_queries[(query_id, str(measure))]
                run_query_max = compare(
                    wanted,
                    actual,
                    f"ir_measures/{run_id}/{query_id}/{metric_name}",
                    run_query_max,
                )
            wanted = float(phase3.metric_value(run["macro"][metric_name]))
            actual = float(independent_aggregate[measure])
            run_aggregate_max = compare(
                wanted, actual, f"ir_measures/{run_id}/{metric_name}", run_aggregate_max
            )
        maximum_query = max(maximum_query, run_query_max)
        maximum_aggregate = max(maximum_aggregate, run_aggregate_max)
        reports.append(
            {
                "aggregate_maximum_absolute_difference": run_aggregate_max,
                "query_count": len(population),
                "query_maximum_absolute_difference": run_query_max,
                "run_id": run_id,
                "status": "passed",
            }
        )
    return {
        "maximum_absolute_differences": {
            "aggregate": maximum_aggregate,
            "per_query": maximum_query,
        },
        "runs": reports,
        "status": "passed",
        "version": version,
    }


def official_cross_check(
    collection: Path,
    matrix: Path,
    binary: Path,
    identity_path: Path,
) -> dict[str, Any]:
    dependency = official.load_dependency_identity(identity_path, binary)
    run_ids = {
        row["letter"]: row["run_id"]
        for row in phase3.read_json(matrix / "phase-3a-development-matrix.json")[
            "run_ids"
        ]
        if row["letter"] != "d"
    }
    metrics = rust_metrics(matrix)
    qrels_by_query = official.parse_qrels((collection / "qrels.tsv").read_bytes())
    maximum_query = 0.0
    maximum_aggregate = 0.0
    reports = []
    for letter in "abcefg":
        run_id = run_ids[letter]
        run = metrics[run_id]
        population = [
            row["query_id"]
            for row in run["queries"]
            if row["execution_status"] == "valid"
        ]
        run_path = matrix / "runs" / f"{run_id}.trec"
        official.parse_run(run_path.read_bytes(), run_id)
        with tempfile.TemporaryDirectory(
            prefix="hotpotqa-phase3-trec-eval-"
        ) as directory:
            temporary = Path(directory)
            selected_qrels = temporary / "qrels.tsv"
            selected_qrels.write_bytes(
                b"".join(
                    official.exponential_gain_qrel(line)
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
            direct_measures = {
                mapping[1]
                for mapping in official.SUPPORTED_MAPPINGS
                if mapping[3] != "top_10_truncated_run"
            }
            direct = official.parse_trec_eval_output(
                official.run_official(
                    [
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
                ),
                direct_measures,
                set(population),
            )
            reciprocal = official.parse_trec_eval_output(
                official.run_official(
                    [
                        str(binary),
                        "-q",
                        "-c",
                        "-l1",
                        "-m",
                        "recip_rank",
                        str(selected_qrels),
                        str(top_ten),
                    ]
                ),
                {"recip_rank"},
                set(population),
            )
        _, _, query_max, aggregate_max = official.compare_run_metrics(
            run_id, run, population, direct | reciprocal
        )
        maximum_query = max(maximum_query, query_max)
        maximum_aggregate = max(maximum_aggregate, aggregate_max)
        reports.append(
            {
                "aggregate_maximum_absolute_difference": aggregate_max,
                "query_count": len(population),
                "query_maximum_absolute_difference": query_max,
                "run_id": run_id,
                "status": "passed",
            }
        )
    return {
        "dependency": dependency,
        "maximum_absolute_differences": {
            "aggregate": maximum_aggregate,
            "per_query": maximum_query,
        },
        "runs": reports,
        "status": "passed",
        "unsupported_metrics": list(official.UNSUPPORTED_METRICS),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, default=phase3.DEFAULT_COLLECTION)
    parser.add_argument("--matrix", type=Path, default=phase3.DEFAULT_MATRIX)
    parser.add_argument("--trec-eval", type=Path, required=True)
    parser.add_argument("--tool-identity", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        collection = args.collection.resolve()
        matrix = args.matrix.resolve()
        for path in (collection, matrix, args.output.resolve()):
            phase3.guard_development_path(path)
        report = {
            "artifact_schema": "hotpotqa-phase-3a-external-cross-check-v1",
            "ir_measures": ir_cross_check(collection, matrix),
            "official_trec_eval": official_cross_check(
                collection,
                matrix,
                args.trec_eval.resolve(),
                args.tool_identity.resolve(),
            ),
            "status": "passed",
            "tolerance": TOLERANCE,
        }
        output = args.output.resolve()
        if output.exists():
            raise phase3.ValidationError(f"refusing to overwrite {output}")
        output.write_bytes(phase3.canonical(report) + b"\n")
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except (OSError, KeyError, TypeError, ValueError, phase3.ValidationError) as error:
        print(f"error: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
