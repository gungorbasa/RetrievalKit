#!/usr/bin/env python3
"""Build deterministic Phase 3b comparisons, slices, and closed error analysis."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


class AnalysisError(ValueError):
    pass


def canonical(value: Any) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, allow_nan=False, separators=(",", ":"), sort_keys=True
    ).encode()


def read_json(path: Path) -> Any:
    data = path.read_bytes()
    value = json.loads(data)
    if data != canonical(value) + b"\n":
        raise AnalysisError(f"non-canonical JSON: {path}")
    return value


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    data = path.read_bytes()
    if data and not data.endswith(b"\n"):
        raise AnalysisError(f"JSONL missing final LF: {path}")
    rows = [json.loads(line) for line in data.splitlines()]
    if b"".join(canonical(row) + b"\n" for row in rows) != data:
        raise AnalysisError(f"non-canonical JSONL: {path}")
    return rows


def qrels(path: Path) -> dict[str, dict[str, int]]:
    result: dict[str, dict[str, int]] = defaultdict(dict)
    previous = None
    for line in path.read_text().splitlines():
        query, zero, document, grade = line.split(" ")
        key = (query, document)
        if zero != "0" or previous is not None and key <= previous:
            raise AnalysisError("qrels are malformed or reordered")
        previous = key
        result[query][document] = int(grade)
    return dict(result)


def trec(path: Path, run_id: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = defaultdict(list)
    previous = None
    for line in path.read_text().splitlines():
        query, q0, document, rank, score, tag = line.split(" ")
        key = (query, int(rank))
        if q0 != "Q0" or tag != run_id or previous is not None and key <= previous:
            raise AnalysisError(f"malformed TREC run: {path}")
        if int(rank) != len(result[query]) + 1 or int(score) <= 0:
            raise AnalysisError(f"non-contiguous TREC run: {path}")
        result[query].append(document)
        previous = key
    return dict(result)


def retrieval_metrics(documents: list[str], judgments: dict[str, int]) -> dict[str, float]:
    relevant = {document for document, grade in judgments.items() if grade >= 1}

    def recall(cutoff: int) -> float:
        return len(relevant & set(documents[:cutoff])) / len(relevant)

    def judged(cutoff: int) -> float:
        denominator = min(cutoff, len(documents))
        return (
            sum(document in judgments for document in documents[:cutoff]) / denominator
            if denominator
            else 0.0
        )

    def ndcg(cutoff: int) -> float:
        dcg = 0.0
        for rank, document in enumerate(documents[:cutoff], 1):
            dcg += ((1 << judgments.get(document, 0)) - 1) / math.log2(rank + 1)
        ideal = sorted(judgments.items(), key=lambda row: (-row[1], row[0]))[:cutoff]
        idcg = 0.0
        for rank, (_, grade) in enumerate(ideal, 1):
            idcg += ((1 << grade) - 1) / math.log2(rank + 1)
        return dcg / idcg

    hits = 0
    ap = 0.0
    for rank, document in enumerate(documents, 1):
        if document in relevant:
            hits += 1
            ap += hits / rank
    reciprocal = next(
        (
            1.0 / rank
            for rank, document in enumerate(documents[:10], 1)
            if document in relevant
        ),
        0.0,
    )
    return {
        "ap": ap / len(relevant),
        "judged_at_10": judged(10),
        "judged_at_5": judged(5),
        "mrr_at_10": reciprocal,
        "ndcg_at_10": ndcg(10),
        "ndcg_at_5": ndcg(5),
        "precision_at_5": len(relevant & set(documents[:5])) / 5,
        "recall_at_10": recall(10),
        "recall_at_5": recall(5),
        "success_at_1": float(bool(documents and documents[0] in relevant)),
    }


def best_evidence(documents: set[str], alternatives: list[list[str]]) -> tuple[int, int]:
    choices = []
    for required in alternatives:
        matched = len(documents & set(required))
        choices.append((matched, len(required), canonical(required)))
    choices.sort(key=lambda row: (-row[0] / row[1], -row[0], row[1], row[2]))
    return choices[0][:2]


def evidence_metrics(documents: list[str], alternatives: list[list[str]]) -> dict[str, float]:
    result = {}
    for cutoff in (5, 10):
        matched, required = best_evidence(set(documents[:cutoff]), alternatives)
        result[f"supporting_document_recall_at_{cutoff}"] = matched / required
        result[f"complete_evidence_recall_at_{cutoff}"] = float(matched == required)
    return result


def aggregate(rows: dict[str, dict[str, float]]) -> dict[str, float]:
    names = tuple(next(iter(rows.values())))
    return {
        name: sum(rows[query][name] for query in sorted(rows)) / len(rows)
        for name in names
    }


def relative(delta: float, baseline: float) -> float | None:
    return delta / abs(baseline) if baseline != 0 else None


def scope_bucket(count: int) -> str:
    if count == 0:
        return "empty"
    if count <= 10:
        return "1-10"
    if count <= 100:
        return "11-100"
    if count <= 1_000:
        return "101-1000"
    if count <= 10_000:
        return "1001-10000"
    return "above-10000"


def recall_bucket(value: float) -> str:
    if value == 0:
        return "0"
    if value < 0.5:
        return "(0,0.5)"
    if value < 1:
        return "[0.5,1)"
    return "1"


def raw_result_queries(artifacts: Path) -> dict[str, dict[str, Any]]:
    rows = []
    for name in ("rust-results.json", "graph-retrieval-rust-results.json"):
        rows.extend(read_json(artifacts / name)["runs"])
    return {
        row["run_id"]: {query["query_id"]: query for query in row["queries"]}
        for row in rows
    }


def build(collection: Path, artifacts: Path) -> dict[str, Any]:
    configurations = read_json(artifacts / "run-configurations.json")["runs"]
    run_ids = {
        row["configuration"]["run_letter"]: row["run_id"] for row in configurations
    }
    if set(run_ids) != set("abcdefg"):
        raise AnalysisError("locked run matrix is not exactly A-G")
    judgments = qrels(collection / "qrels.tsv")
    evidence = {
        row["query_id"]: row["evidence_sets"]
        for row in read_jsonl(collection / "evidence-judgments.jsonl")
    }
    queries = {
        row["query_id"]: row for row in read_jsonl(collection / "queries.jsonl")
    }
    exclusions = read_jsonl(collection / "exclusions.jsonl")
    excluded = {
        row["query_id"]
        for row in exclusions
        if row["lane"] == "hotpotqa-exact-title-v1"
    }
    rankings = {
        letter: trec(artifacts / "runs" / f"{run_ids[letter]}.trec", run_ids[letter])
        for letter in "abcefg"
    }
    per_query: dict[str, dict[str, dict[str, float]]] = {}
    aggregates = {}
    for letter, run in rankings.items():
        per_query[letter] = {}
        for query_id, documents in run.items():
            per_query[letter][query_id] = {
                **retrieval_metrics(documents, judgments[query_id]),
                **evidence_metrics(documents, evidence[query_id]),
            }
        aggregates[letter] = aggregate(per_query[letter])

    selections = {
        row["query_id"]: row
        for row in read_jsonl(
            artifacts / "graph-selections" / f"{run_ids['d']}.jsonl"
        )
    }
    candidate_metrics = {}
    for query_id, selection in selections.items():
        projected = {
            row["record_id"]
            for row in next(
                row["candidates"]
                for row in read_jsonl(artifacts / "graph-projection-identities.jsonl")
                if row["query_id"] == query_id
            )
        }
        matched, required = best_evidence(projected, evidence[query_id])
        projected_count = selection["projected_chunks_after_filter"]
        candidate_metrics[query_id] = {
            "candidate_complete_evidence": float(matched == required),
            "candidate_recall": matched / required,
            "candidate_reduction_ratio": (
                selection["eligible_corpus_chunks_after_filter"] / projected_count
                if projected_count
                else None
            ),
            "empty_scope": float(projected_count == 0),
        }
    aggregate_candidate = {
        name: (
            sum(float(row[name]) for row in candidate_metrics.values() if row[name] is not None)
            / sum(row[name] is not None for row in candidate_metrics.values())
        )
        for name in next(iter(candidate_metrics.values()))
    }

    comparisons = []
    for baseline, compared in (
        ("a", "e"),
        ("b", "f"),
        ("c", "g"),
        ("a", "b"),
        ("e", "f"),
        ("b", "c"),
        ("f", "g"),
    ):
        population = sorted(per_query[baseline].keys() & per_query[compared].keys())
        metric_rows = {}
        for metric in (
            "ndcg_at_10",
            "recall_at_10",
            "complete_evidence_recall_at_10",
        ):
            left = sum(per_query[baseline][query][metric] for query in population) / len(
                population
            )
            right = sum(per_query[compared][query][metric] for query in population) / len(
                population
            )
            delta = right - left
            metric_rows[metric] = {
                "absolute_delta": delta,
                "baseline": left,
                "compared": right,
                "relative_delta": relative(delta, left),
            }
        wins = ties = losses = 0
        lost = []
        recovered = []
        affected = []
        for query_id in population:
            left_ndcg = per_query[baseline][query_id]["ndcg_at_10"]
            right_ndcg = per_query[compared][query_id]["ndcg_at_10"]
            if right_ndcg > left_ndcg:
                wins += 1
            elif right_ndcg < left_ndcg:
                losses += 1
            else:
                ties += 1
            left_evidence = per_query[baseline][query_id][
                "complete_evidence_recall_at_10"
            ]
            right_evidence = per_query[compared][query_id][
                "complete_evidence_recall_at_10"
            ]
            if right_evidence < left_evidence:
                lost.append(query_id)
            if right_evidence > left_evidence:
                recovered.append(query_id)
            if left_ndcg != right_ndcg or left_evidence != right_evidence:
                affected.append(query_id)
        comparisons.append(
            {
                "affected_query_ids": affected,
                "baseline": baseline.upper(),
                "compared": compared.upper(),
                "evidence_lost_query_ids": lost,
                "evidence_recovered_query_ids": recovered,
                "metrics": metric_rows,
                "wins_ties_losses": {"losses": losses, "ties": ties, "wins": wins},
            }
        )

    slices: dict[str, Counter[str]] = defaultdict(Counter)
    errors: dict[str, set[str]] = defaultdict(set)
    for query_id, query in queries.items():
        question_type, difficulty = query["category"].split(":", 1)
        slices["question_type"][question_type] += 1
        slices["difficulty"][difficulty] += 1
        slices["exact_type_level"][query["category"]] += 1
        slices["evidence_document_count"][
            str(len({document for row in evidence[query_id] for document in row}))
        ] += 1
        slices["filter_selectivity"]["not_applicable"] += 1
        slices["path_accuracy"]["not_applicable"] += 1
        if query_id in excluded:
            slices["derived_seed_status"]["derived_seed_ambiguous"] += 1
            errors["seed ambiguity exclusion"].add(query_id)
            continue
        slices["derived_seed_status"]["resolved"] += 1
        selection = selections[query_id]
        slices["candidate_scope_size"][
            scope_bucket(selection["projected_chunks_after_filter"])
        ] += 1
        slices["truncation_reason"][selection["truncated_reason"] or "none"] += 1
        recall = candidate_metrics[query_id]["candidate_recall"]
        slices["candidate_recall_bucket"][recall_bucket(float(recall))] += 1
        if selection["projected_chunks_after_filter"] == 0:
            errors["empty graph scope"].add(query_id)
        if selection["truncated_reason"] is not None:
            errors["truncated graph selection"].add(query_id)
        if recall < 1:
            errors["supporting evidence absent from scope"].add(query_id)
        if recall == 1 and per_query["g"][query_id]["complete_evidence_recall_at_10"] == 0:
            errors["evidence in scope but missing from top 10"].add(query_id)
    for query_id, row in per_query["a"].items():
        if row["complete_evidence_recall_at_10"] == 0:
            errors["whole-corpus ranking failure"].add(query_id)
    for query_id, row in per_query["e"].items():
        if row["complete_evidence_recall_at_10"] == 0:
            errors["graph-scoped ranking failure"].add(query_id)
    for left, right in (("a", "b"), ("e", "f")):
        for query_id in per_query[left].keys() & per_query[right].keys():
            if rankings[left][query_id] != rankings[right][query_id]:
                errors["I8 ranking divergence"].add(query_id)
    for left, right in (("b", "c"), ("f", "g")):
        for query_id in per_query[left].keys() & per_query[right].keys():
            if per_query[right][query_id]["ndcg_at_10"] < per_query[left][query_id][
                "ndcg_at_10"
            ]:
                errors["hybrid ranking regression"].add(query_id)
    raw = raw_result_queries(artifacts)
    for run in raw.values():
        for query_id, row in run.items():
            if row["duplicate_collapse_count"]:
                errors["duplicate-collapse effect"].add(query_id)
            if row["execution_status"] == "invalid_execution":
                errors["unexpected execution failure"].add(query_id)
    closed_errors = (
        "seed ambiguity exclusion",
        "empty graph scope",
        "truncated graph selection",
        "supporting evidence absent from scope",
        "evidence in scope but missing from top 10",
        "whole-corpus ranking failure",
        "graph-scoped ranking failure",
        "I8 ranking divergence",
        "hybrid ranking regression",
        "duplicate-collapse effect",
        "unexpected execution failure",
    )
    for name in closed_errors:
        errors.setdefault(name, set())
    return {
        "aggregate_candidate_metrics": aggregate_candidate,
        "aggregate_run_metrics": {letter.upper(): values for letter, values in aggregates.items()},
        "comparisons": comparisons,
        "errors": {
            name: {"count": len(ids), "query_ids": sorted(ids)}
            for name, ids in sorted(errors.items())
        },
        "path_accuracy": "not_applicable",
        "schema_version": 1,
        "slices": {
            name: dict(sorted(values.items())) for name, values in sorted(slices.items())
        },
        "status": "valid",
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        output = args.output.resolve()
        if output.exists():
            raise AnalysisError(f"refusing to overwrite {output}")
        report = build(args.collection.resolve(), args.artifacts.resolve())
        output.write_bytes(canonical(report) + b"\n")
        print(
            json.dumps(
                {
                    "output": str(output),
                    "sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
                    "status": "valid",
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0
    except (OSError, KeyError, TypeError, ValueError, AnalysisError) as error:
        print(f"error: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
