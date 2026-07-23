#!/usr/bin/env python3
"""Independently reconstruct and cross-check V3 Phase 1.2c E-G runs.

Expected graph, ranking, metric, paired-comparison, and generation values are
fully calculated from the frozen collection before any Rust artifact is read.
The oracle does not invoke RetrievalKit or use Rust traces as calculation inputs.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import Counter
from pathlib import Path
from typing import Any

if __package__:
    from . import validate_v3_conformance as foundation
    from . import validate_v3_phase_1_2a as retrieval
    from . import validate_v3_phase_1_2b as graph_oracle
else:
    import validate_v3_conformance as foundation
    import validate_v3_phase_1_2a as retrieval
    import validate_v3_phase_1_2b as graph_oracle


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
REPORT_NAME = "graph-retrieval-independent-cross-check.json"
REPORT_SCHEMA = "phase-1.2c-independent-cross-check-v1"
SCORE_TOLERANCE = 2.0e-7
METRIC_TOLERANCE = 1.0e-12
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
PAIRED_METRICS = (
    "ap",
    "complete_evidence_recall_at_10",
    "complete_evidence_recall_at_5",
    "judged_at_10",
    "judged_at_5",
    "mrr_at_10",
    "ndcg_at_10",
    "ndcg_at_5",
    "precision_at_5",
    "recall_at_10",
    "recall_at_5",
    "success_at_1",
    "supporting_document_recall_at_10",
    "supporting_document_recall_at_5",
)


ValidationError = retrieval.ValidationError


def collection_files(collection: Path) -> dict[str, bytes]:
    header = retrieval.read_json(collection / "collection.json")
    files = {entry["path"]: (collection / entry["path"]).read_bytes() for entry in header["files"]}
    files["collection.json"] = (collection / "collection.json").read_bytes()
    return files


def scoped_keyword_candidates(
    all_chunks: list[dict[str, Any]],
    allowed: set[tuple[str, str]],
    query: dict[str, Any],
    limit: int,
) -> list[dict[str, Any]]:
    """Reproduce scoped BM25 membership with whole-index corpus statistics."""

    average_length = retrieval.f32_div(
        sum(len(chunk["tokens"]) for chunk in all_chunks), len(all_chunks)
    )
    query_terms = sorted(set(retrieval.tokenize(query["text"])))
    scores: dict[tuple[str, str], float] = {}
    matches: dict[tuple[str, str], list[str]] = {}
    for term in query_terms:
        document_frequency = sum(term in chunk["tokens"] for chunk in all_chunks)
        if document_frequency == 0:
            continue
        inverse_frequency = retrieval.inverse_document_frequency(
            len(all_chunks), document_frequency
        )
        for chunk in all_chunks:
            identity = chunk["identity"]
            if identity not in allowed or not retrieval.filter_matches(
                query["metadata_filter"], chunk["metadata"]
            ):
                continue
            frequency = Counter(chunk["tokens"])[term]
            if frequency == 0:
                continue
            score = retrieval.bm25_term_score(
                frequency, len(chunk["tokens"]), average_length, inverse_frequency
            )
            scores[identity] = retrieval.f32_add(scores.get(identity, 0.0), score)
            matches.setdefault(identity, []).append(term)
    by_identity = {chunk["identity"]: chunk for chunk in all_chunks}
    candidates = [
        {"chunk": by_identity[identity], "matched_terms": matches[identity], "score": score}
        for identity, score in scores.items()
    ]
    candidates.sort(key=lambda row: (-row["score"], row["chunk"]["identity"]))
    return candidates[:limit]


def scoped_hybrid_hits(
    all_chunks: list[dict[str, Any]],
    scoped_chunks: list[dict[str, Any]],
    query: dict[str, Any],
) -> list[dict[str, Any]]:
    vectors = retrieval.vector_candidates(scoped_chunks, query, "i8", 8)
    allowed = {chunk["identity"] for chunk in scoped_chunks}
    keywords = scoped_keyword_candidates(all_chunks, allowed, query, 8)
    vector_by_id = {
        row["chunk"]["identity"]: (rank, row) for rank, row in enumerate(vectors, 1)
    }
    keyword_by_id = {
        row["chunk"]["identity"]: (rank, row) for rank, row in enumerate(keywords, 1)
    }
    vector_scores = [row["score"] for row in vectors]
    keyword_scores = [row["score"] for row in keywords]
    vector_range = (min(vector_scores), max(vector_scores))
    keyword_range = (
        (min(keyword_scores), max(keyword_scores)) if keyword_scores else None
    )
    candidates = []
    for identity in sorted(set(vector_by_id) | set(keyword_by_id)):
        vector = vector_by_id.get(identity)
        keyword = keyword_by_id.get(identity)
        vector_score = None if vector is None else vector[1]["score"]
        keyword_score = None if keyword is None else keyword[1]["score"]
        vector_normalized = (
            None
            if vector_score is None
            else retrieval.normalized_score(vector_score, *vector_range)
        )
        keyword_normalized = (
            None
            if keyword_score is None or keyword_range is None
            else retrieval.normalized_score(keyword_score, *keyword_range)
        )
        fusion = retrieval.f32_add(
            retrieval.f32_mul(0.6, 0.0 if vector_normalized is None else vector_normalized),
            retrieval.f32_mul(0.4, 0.0 if keyword_normalized is None else keyword_normalized),
        )
        candidates.append(
            {
                "bm25_normalized_score": keyword_normalized,
                "bm25_score": keyword_score,
                "chunk_key": identity[1],
                "fusion_score": fusion,
                "keyword_rank": None if keyword is None else keyword[0],
                "matched_terms": [] if keyword is None else keyword[1]["matched_terms"],
                "native_rank": 0,
                "record_id": identity[0],
                "vector_normalized_score": vector_normalized,
                "vector_rank": None if vector is None else vector[0],
                "vector_score": vector_score,
                "_chunk_id": (vector or keyword)[1]["chunk"]["chunk_id"],
            }
        )
    candidates.sort(
        key=lambda row: (
            -row["fusion_score"],
            row["vector_rank"] if row["vector_rank"] is not None else math.inf,
            row["keyword_rank"] if row["keyword_rank"] is not None else math.inf,
            row["_chunk_id"],
        )
    )
    for rank, row in enumerate(candidates, 1):
        row["native_rank"] = rank
        del row["_chunk_id"]
    return candidates


def metric(status: str, value: float | None = None) -> dict[str, Any]:
    return {"status": status, "value": value}


def documents_as_ids(documents: list[dict[str, Any]]) -> list[str]:
    return [document["record_id"] for document in documents]


def qrels_for_query(qrels: dict[str, dict[str, int]], query_id: str) -> dict[str, int]:
    return qrels[query_id]


def best_evidence(documents: set[str], judgment: dict[str, Any]) -> tuple[int, int]:
    return graph_oracle.best_evidence(documents, judgment)


def retrieval_metric_values(
    documents: list[dict[str, Any]], qrels: dict[str, int]
) -> dict[str, float]:
    return retrieval.retrieval_metrics(documents, qrels)


def path_matches(paths: list[dict[str, Any]], expected: dict[str, Any]) -> bool:
    actual_rows = {graph_oracle.canonical(row["edges"]) for row in paths}
    expected_rows = {graph_oracle.canonical(row) for row in expected["expected_paths"]}
    return not actual_rows.isdisjoint(expected_rows)


def query_metrics(
    model: dict[str, Any],
    qrels: dict[str, dict[str, int]],
    lane: str,
    query: dict[str, Any],
    result: dict[str, Any],
    selection: dict[str, Any],
    projection: dict[str, Any],
    paths: list[dict[str, Any]],
) -> dict[str, Any]:
    values = {name: metric("not_applicable") for name in METRIC_NAMES}
    for name, value in retrieval_metric_values(
        result["projected_documents"], qrels_for_query(qrels, query["query_id"])
    ).items():
        values[name] = metric("valid", value)
    projected = selection["projected_chunks_after_filter"]
    eligible = selection["eligible_corpus_chunks_after_filter"]
    values["candidate_reduction_ratio"] = (
        metric("undefined") if projected == 0 else metric("valid", eligible / projected)
    )
    values["empty_scope"] = metric("valid", 1.0 if projected == 0 else 0.0)
    reason = selection["truncated_reason"]
    values["truncated"] = metric("valid", 1.0 if reason is not None else 0.0)
    for name, expected in (
        ("truncated_max_hops", "max_hops"),
        ("truncated_max_results", "max_results"),
        ("truncated_max_visited", "max_visited"),
        ("truncated_max_working_bytes", "max_working_bytes"),
    ):
        values[name] = metric("valid", 1.0 if reason == expected else 0.0)
    if "evidence" in query["tasks"]:
        evidence = model["evidence"][query["query_id"]]
        candidates = {row["record_id"] for row in projection["candidates"]}
        matched, required = best_evidence(candidates, evidence)
        values["candidate_recall"] = metric("valid", matched / required)
        values["candidate_complete_evidence"] = metric(
            "valid", 1.0 if matched == required else 0.0
        )
        document_ids = documents_as_ids(result["projected_documents"])
        for cutoff in (5, 10):
            matched, required = best_evidence(set(document_ids[:cutoff]), evidence)
            values[f"supporting_document_recall_at_{cutoff}"] = metric(
                "valid", matched / required
            )
            values[f"complete_evidence_recall_at_{cutoff}"] = metric(
                "valid", 1.0 if matched == required else 0.0
            )
    if "path" in query["tasks"]:
        expected = model["expected_paths"].get((lane, query["query_id"]))
        if expected is not None:
            values["path_accuracy"] = metric(
                "valid", 1.0 if path_matches(paths, expected) else 0.0
            )
    return values


def macro_metric(rows: list[dict[str, Any]]) -> dict[str, Any]:
    counts = {
        status: 0
        for status in (
            "excluded_pre_freeze",
            "invalid_execution",
            "not_applicable",
            "undefined",
            "valid",
        )
    }
    values = []
    for row in rows:
        counts[row["status"]] += 1
        if row["status"] == "valid":
            values.append(row["value"])
    numerator = sum(values)
    return {
        "denominator": len(values),
        "numerator": numerator,
        "status_counts": counts,
        "value": None if not values else numerator / len(values),
    }


def micro_metrics(
    model: dict[str, Any], query_rows: list[dict[str, Any]], run: dict[str, Any]
) -> dict[str, Any]:
    supporting = {5: [0, 0], 10: [0, 0]}
    candidate = [0, 0]
    eligible = projected = empty = 0
    truncated = {name: 0 for name in ("all", "max_hops", "max_results", "max_visited", "max_working_bytes")}
    result_by_id = {row["query_id"]: row for row in run["result"]["queries"]}
    selection_by_id = {row["query_id"]: row for row in run["selections"]}
    projection_by_id = {row["query_id"]: row for row in run["projections"]}
    for row in query_rows:
        if row["execution_status"] != "valid":
            continue
        query_id = row["query_id"]
        counts = row["candidate_counts"]
        eligible += counts["eligible_chunks"]
        projected += counts["projected_chunks"]
        empty += counts["projected_chunks"] == 0
        reason = selection_by_id[query_id]["truncated_reason"]
        if reason is not None:
            truncated["all"] += 1
            truncated[reason] += 1
        query = model["queries"][query_id]
        if "evidence" not in query["tasks"]:
            continue
        evidence = model["evidence"][query_id]
        documents = documents_as_ids(result_by_id[query_id]["projected_documents"])
        candidates = {row["record_id"] for row in projection_by_id[query_id]["candidates"]}
        matched, required = best_evidence(candidates, evidence)
        candidate[0] += matched
        candidate[1] += required
        for cutoff in (5, 10):
            matched, required = best_evidence(set(documents[:cutoff]), evidence)
            supporting[cutoff][0] += matched
            supporting[cutoff][1] += required
    valid_count = sum(row["execution_status"] == "valid" for row in query_rows)

    def ratio(left: int, right: int) -> float | None:
        return None if right == 0 else left / right

    output = {
        "candidate_recall": {
            "matched_documents": candidate[0],
            "required_documents": candidate[1],
            "value": ratio(*candidate),
        },
        "candidate_reduction_ratio": {
            "candidate_chunks": projected,
            "eligible_chunks": eligible,
            "value": ratio(eligible, projected),
        },
        "empty_scope_rate": {
            "empty_scopes": empty,
            "graph_valid_queries": valid_count,
            "value": ratio(empty, valid_count),
        },
    }
    for cutoff in (10, 5):
        output[f"supporting_document_recall_at_{cutoff}"] = {
            "matched_documents": supporting[cutoff][0],
            "required_documents": supporting[cutoff][1],
            "value": ratio(*supporting[cutoff]),
        }
    for suffix, key in (
        ("", "all"),
        ("_max_hops", "max_hops"),
        ("_max_results", "max_results"),
        ("_max_visited", "max_visited"),
        ("_max_working_bytes", "max_working_bytes"),
    ):
        output[f"truncation_rate{suffix}"] = {
            "affected_queries": truncated[key],
            "graph_valid_queries": valid_count,
            "value": ratio(truncated[key], valid_count),
        }
    return output


def calculate_metrics(
    model: dict[str, Any], qrels: dict[str, dict[str, int]], runs: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    outputs = []
    for run in runs:
        identity = run["identity"]
        lane = identity["configuration"]["seed_lane"]
        result_by_id = {row["query_id"]: row for row in run["result"]["queries"]}
        selection_by_id = {row["query_id"]: row for row in run["selections"]}
        projection_by_id = {row["query_id"]: row for row in run["projections"]}
        query_rows = []
        for query_id in identity["declared_population"]:
            result = result_by_id[query_id]
            if result["execution_status"] == "excluded_pre_freeze":
                query_rows.append(
                    {
                        "candidate_counts": None,
                        "execution_status": "excluded_pre_freeze",
                        "metrics": {
                            name: metric("excluded_pre_freeze") for name in METRIC_NAMES
                        },
                        "query_id": query_id,
                    }
                )
                continue
            paths = [row for row in run["paths"] if row["query_id"] == query_id]
            values = query_metrics(
                model,
                qrels,
                lane,
                model["queries"][query_id],
                result,
                selection_by_id[query_id],
                projection_by_id[query_id],
                paths,
            )
            query_rows.append(
                {
                    "candidate_counts": {
                        "eligible_chunks": selection_by_id[query_id][
                            "eligible_corpus_chunks_after_filter"
                        ],
                        "projected_chunks": selection_by_id[query_id][
                            "projected_chunks_after_filter"
                        ],
                    },
                    "execution_status": "valid",
                    "metrics": values,
                    "query_id": query_id,
                }
            )
        outputs.append(
            {
                "counts": {
                    "attempted": len(identity["execution_population"]),
                    "declared": len(identity["declared_population"]),
                    "excluded_pre_freeze": len(identity["declared_population"])
                    - len(identity["execution_population"]),
                    "invalid_execution": 0,
                    "valid_execution": len(identity["execution_population"]),
                },
                "declared_population_sha256": identity["declared_population_sha256"],
                "execution_population_sha256": identity["execution_population_sha256"],
                "macro": {
                    name: macro_metric([row["metrics"][name] for row in query_rows])
                    for name in METRIC_NAMES
                },
                "micro": micro_metrics(model, query_rows, run),
                "queries": query_rows,
                "run_id": identity["run_id"],
                "status": "valid",
            }
        )
    return sorted(outputs, key=lambda row: row["run_id"])


def execute_expected(collection: Path) -> dict[str, Any]:
    retrieval.verify_frozen_fixture(collection)
    files = collection_files(collection)
    identities = [
        run
        for run in foundation.derive_runs(files)
        if run["configuration"]["run_letter"] in {"e", "f", "g"}
    ]
    if len(identities) != 9:
        raise ValidationError(f"expected nine independently derived E-G runs, got {len(identities)}")
    generated = foundation.derive_generation_fingerprints(files, identities)
    fingerprint_by_run = {
        row["run_id"]: row["fingerprint"] for row in generated["bindings"]
    }
    model = graph_oracle.load_collection(collection)
    graph = graph_oracle.build_graph(model)
    seeds, _ = graph_oracle.derive_seeds(model)
    all_chunks, retrieval_queries = retrieval.load_fixture(collection)
    query_inputs = {row["query_id"]: row for row in retrieval_queries}
    qrels = retrieval.load_qrels(collection)
    run_outputs = []
    for identity in identities:
        letter = identity["configuration"]["run_letter"]
        lane = identity["configuration"]["seed_lane"]
        fingerprint = fingerprint_by_run[identity["run_id"]]
        results = []
        selections = []
        paths = []
        projections = []
        trec_rows = []
        for query_id in identity["declared_population"]:
            query = model["queries"][query_id]
            limits = identity["configuration"]["candidate_limits"]
            if query_id not in identity["execution_population"]:
                results.append(
                    {
                        "candidate_limits": limits,
                        "chunk_hits": [],
                        "duplicate_collapse_count": 0,
                        "execution_status": "excluded_pre_freeze",
                        "filter": query["metadata_filter"],
                        "projected_documents": [],
                        "query_id": query_id,
                        "selection_run_id": identity["run_id"],
                        "status_reason": model["exclusions"][(lane, query_id)]["reason"],
                    }
                )
                continue
            seed = seeds[(lane, query_id)]
            matches, trace = graph_oracle.traverse(graph, seed["canonical"], query)
            matched_nodes = sorted((row["node"] for row in matches), key=graph_oracle.canonical)
            record_ids = {row["source"]["record_id"] for row in matched_nodes}
            before_filter = [chunk for chunk in all_chunks if chunk["identity"][0] in record_ids]
            scoped_chunks = [
                chunk
                for chunk in before_filter
                if retrieval.filter_matches(query["metadata_filter"], chunk["metadata"])
            ]
            eligible = [
                chunk
                for chunk in all_chunks
                if retrieval.filter_matches(query["metadata_filter"], chunk["metadata"])
            ]
            input_query = query_inputs[query_id]
            hits = (
                retrieval.semantic_hits(scoped_chunks, input_query, "f32" if letter == "e" else "i8")
                if letter in {"e", "f"}
                else scoped_hybrid_hits(all_chunks, scoped_chunks, input_query)
            )
            documents, duplicates = retrieval.project(
                hits, model["collection"]["evaluation_depth"]
            )
            query_paths = [
                graph_oracle.path_row(row, query_id, identity["run_id"]) for row in matches
            ]
            query_paths.sort(
                key=lambda row: (
                    row["query_id"],
                    graph_oracle.canonical(row["matched_node"]),
                    graph_oracle.canonical(row["edges"]),
                )
            )
            paths.extend(query_paths)
            projection = {
                "candidates": [
                    {"chunk_key": row["identity"][1], "record_id": row["identity"][0]}
                    for row in scoped_chunks
                ],
                "query_id": query_id,
                "run_id": identity["run_id"],
            }
            projections.append(projection)
            selection = {
                "active_corpus_chunks_before_filter": len(all_chunks),
                "corpus_id": model["collection"]["corpus_id"],
                "eligible_corpus_chunks_after_filter": len(eligible),
                "generation_fingerprint": fingerprint,
                "matched_nodes": matched_nodes,
                "projected_chunks_after_filter": len(scoped_chunks),
                "projected_chunks_before_filter": len(before_filter),
                "projected_documents_after_filter": len(
                    {row["identity"][0] for row in scoped_chunks}
                ),
                "query_id": query_id,
                "resolved_seed": seed["canonical"],
                "run_id": identity["run_id"],
                "seed_lane": lane,
                "seed_provenance": seed["provenance"],
                "seed_status": "resolved",
                "stale": False,
                "trace": trace,
                "truncated_reason": None,
            }
            selections.append(selection)
            results.append(
                {
                    "candidate_limits": limits,
                    "chunk_hits": hits,
                    "duplicate_collapse_count": duplicates,
                    "execution_status": "valid",
                    "filter": query["metadata_filter"],
                    "projected_documents": documents,
                    "query_id": query_id,
                    "selection_run_id": identity["run_id"],
                    "status_reason": None,
                }
            )
            for document in documents:
                trec_rows.append(
                    f"{query_id} Q0 {document['record_id']} {document['document_rank']} "
                    f"{11 - document['document_rank']} {identity['run_id']}\n"
                )
        results.sort(key=lambda row: row["query_id"])
        selections.sort(key=lambda row: row["query_id"])
        paths.sort(
            key=lambda row: (
                row["query_id"],
                graph_oracle.canonical(row["matched_node"]),
                graph_oracle.canonical(row["edges"]),
            )
        )
        projections.sort(key=lambda row: row["query_id"])
        run_outputs.append(
            {
                "identity": identity,
                "paths": paths,
                "projections": projections,
                "result": {"queries": results, "run_id": identity["run_id"], "status": "valid"},
                "selections": selections,
                "trec": "".join(trec_rows),
            }
        )
    run_outputs.sort(key=lambda row: row["identity"]["run_id"])
    metrics = calculate_metrics(model, qrels, run_outputs)
    baseline_documents = calculate_baseline_documents(all_chunks, retrieval_queries)
    contract_pairs, diagnostic_pairs = calculate_pairs(
        model, qrels, run_outputs, metrics, baseline_documents
    )
    fingerprints = {
        "fingerprints": generated["preimages"],
        "schema_version": 1,
    }
    return {
        "diagnostic_pairs": diagnostic_pairs,
        "fingerprints": fingerprints,
        "metrics": metrics,
        "model": model,
        "paired": contract_pairs,
        "qrels": qrels,
        "runs": run_outputs,
    }


def calculate_baseline_documents(
    chunks: list[dict[str, Any]], queries: list[dict[str, Any]]
) -> dict[str, dict[str, list[dict[str, Any]]]]:
    output = {letter: {} for letter in ("a", "b", "c")}
    for query in queries:
        output["a"][query["query_id"]] = retrieval.project(
            retrieval.semantic_hits(chunks, query, "f32"), 10
        )[0]
        output["b"][query["query_id"]] = retrieval.project(
            retrieval.semantic_hits(chunks, query, "i8"), 10
        )[0]
        output["c"][query["query_id"]] = retrieval.project(
            retrieval.hybrid_hits(chunks, query), 10
        )[0]
    return output


def paired_query_metric(
    model: dict[str, Any],
    qrels: dict[str, int],
    query: dict[str, Any],
    name: str,
    documents: list[dict[str, Any]],
) -> dict[str, Any]:
    standard = retrieval_metric_values(documents, qrels)
    if name in standard:
        return metric("valid", standard[name])
    if name.startswith(("supporting_document_recall", "complete_evidence_recall")):
        if "evidence" not in query["tasks"]:
            return metric("not_applicable")
        cutoff = 5 if name.endswith("_at_5") else 10
        returned = set(documents_as_ids(documents)[:cutoff])
        matched, required = best_evidence(returned, model["evidence"][query["query_id"]])
        value = 1.0 if matched == required else 0.0
        if name.startswith("supporting"):
            value = matched / required
        return metric("valid", value)
    raise ValidationError(f"unsupported paired metric '{name}'")


def calculate_pairs(
    model: dict[str, Any],
    qrels: dict[str, dict[str, int]],
    runs: list[dict[str, Any]],
    metrics: list[dict[str, Any]],
    baseline_documents: dict[str, dict[str, list[dict[str, Any]]]],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    baseline_ids = {
        "e": "v3-a-whole-semantic-f32-na-cfg-984e4c3bf991",
        "f": "v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53",
        "g": "v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0",
    }
    baseline_letters = {"e": "a", "f": "b", "g": "c"}
    metrics_by_id = {row["run_id"]: row for row in metrics}
    contracts = []
    diagnostics = []
    for run in runs:
        identity = run["identity"]
        letter = identity["configuration"]["run_letter"]
        run_id = identity["run_id"]
        result_by_id = {row["query_id"]: row for row in run["result"]["queries"]}
        metric_by_id = {row["query_id"]: row for row in metrics_by_id[run_id]["queries"]}
        contract_metrics = {}
        diagnostic_metrics = {}
        for name in PAIRED_METRICS:
            baseline_rows = []
            scoped_rows = []
            wins = ties = losses = 0
            for query_id in identity["execution_population"]:
                query = model["queries"][query_id]
                baseline = paired_query_metric(
                    model,
                    qrels[query_id],
                    query,
                    name,
                    baseline_documents[baseline_letters[letter]][query_id],
                )
                scoped = metric_by_id[query_id]["metrics"][name]
                if baseline["status"] == scoped["status"] == "valid":
                    if scoped["value"] > baseline["value"]:
                        wins += 1
                    elif scoped["value"] < baseline["value"]:
                        losses += 1
                    else:
                        ties += 1
                baseline_rows.append(baseline)
                scoped_rows.append(scoped)
            baseline_macro = macro_metric(baseline_rows)
            scoped_macro = macro_metric(scoped_rows)
            delta = (
                None
                if baseline_macro["value"] is None or scoped_macro["value"] is None
                else scoped_macro["value"] - baseline_macro["value"]
            )
            contract_metrics[name] = {
                "baseline": baseline_macro,
                "delta": delta,
                "scoped": scoped_macro,
            }
            relative = (
                None
                if baseline_macro["value"] in (None, 0)
                else delta / baseline_macro["value"]
            )
            diagnostic_metrics[name] = {
                "baseline": baseline_macro["value"],
                "delta": delta,
                "losses": losses,
                "relative_delta": relative,
                "scoped": scoped_macro["value"],
                "ties": ties,
                "wins": wins,
            }
        candidate = candidate_summary(
            model,
            qrels,
            identity,
            run,
            baseline_documents[baseline_letters[letter]],
            result_by_id,
        )
        contracts.append(
            {
                "baseline_run_id": baseline_ids[letter],
                "metrics": contract_metrics,
                "query_population_sha256": identity["execution_population_sha256"],
                "scoped_run_id": run_id,
                "seed_lane": identity["configuration"]["seed_lane"],
                "status": "valid",
            }
        )
        diagnostics.append(
            {
                "baseline_run_id": baseline_ids[letter],
                "candidate_and_loss": candidate,
                "metrics": diagnostic_metrics,
                "query_population_sha256": identity["execution_population_sha256"],
                "scoped_run_id": run_id,
                "seed_lane": identity["configuration"]["seed_lane"],
            }
        )
    return contracts, diagnostics


def candidate_summary(
    model: dict[str, Any],
    qrels: dict[str, dict[str, int]],
    identity: dict[str, Any],
    run: dict[str, Any],
    baselines: dict[str, list[dict[str, Any]]],
    result_by_id: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    selection_by_id = {row["query_id"]: row for row in run["selections"]}
    eligible = projected = relevant_lost = evidence_lost = 0
    per_query = []
    for query_id in identity["execution_population"]:
        selection = selection_by_id[query_id]
        eligible_count = selection["eligible_corpus_chunks_after_filter"]
        projected_count = selection["projected_chunks_after_filter"]
        eligible += eligible_count
        projected += projected_count
        baseline = baselines[query_id][:10]
        scoped = result_by_id[query_id]["projected_documents"][:10]
        baseline_relevant = retrieval.relevant_count(baseline, qrels[query_id], 10)
        scoped_relevant = retrieval.relevant_count(scoped, qrels[query_id], 10)
        query_relevant_lost = max(0, baseline_relevant - scoped_relevant)
        relevant_lost += query_relevant_lost
        query = model["queries"][query_id]
        query_evidence_lost = 0
        if "evidence" in query["tasks"]:
            evidence = model["evidence"][query_id]
            baseline_match = best_evidence(set(documents_as_ids(baseline)), evidence)[0]
            scoped_match = best_evidence(set(documents_as_ids(scoped)), evidence)[0]
            query_evidence_lost = max(0, baseline_match - scoped_match)
        evidence_lost += query_evidence_lost
        per_query.append(
            {
                "eligible_chunks": eligible_count,
                "evidence_documents_lost_at_10": query_evidence_lost,
                "projected_chunks": projected_count,
                "query_id": query_id,
                "relevant_documents_lost_at_10": query_relevant_lost,
            }
        )
    return {
        "candidate_reduction_ratio": None if projected == 0 else eligible / projected,
        "eligible_chunks": eligible,
        "evidence_documents_lost_at_10": evidence_lost,
        "per_query": per_query,
        "projected_chunks": projected,
        "relevant_documents_lost_at_10": relevant_lost,
    }


def assert_structure(
    expected: Any,
    actual: Any,
    path: str,
    differences: dict[str, float],
    tolerance: float,
) -> None:
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        if not isinstance(actual, (int, float)) or isinstance(actual, bool):
            raise ValidationError(f"{path}: expected numeric value, actual {actual!r}")
        difference = abs(float(expected) - float(actual))
        differences["maximum_numeric"] = max(differences["maximum_numeric"], difference)
        if difference > tolerance:
            raise ValidationError(
                f"{path}: expected {expected!r}, actual {actual!r}, difference {difference}"
            )
        return
    if type(expected) is not type(actual):
        raise ValidationError(
            f"{path}: type mismatch expected {type(expected).__name__}, actual {type(actual).__name__}"
        )
    if isinstance(expected, dict):
        if expected.keys() != actual.keys():
            raise ValidationError(
                f"{path}: key mismatch expected {sorted(expected)}, actual {sorted(actual)}"
            )
        for key in expected:
            assert_structure(expected[key], actual[key], f"{path}.{key}", differences, tolerance)
        return
    if isinstance(expected, list):
        if len(expected) != len(actual):
            raise ValidationError(f"{path}: length expected {len(expected)}, actual {len(actual)}")
        for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
            assert_structure(left, right, f"{path}[{index}]", differences, tolerance)
        return
    if expected != actual:
        raise ValidationError(f"{path}: expected {expected!r}, actual {actual!r}")


def validate(collection: Path, artifacts: Path) -> dict[str, Any]:
    # This call must remain before the first artifact read.
    expected = execute_expected(collection)
    differences = {"maximum_numeric": 0.0, "maximum_score": 0.0, "maximum_metric": 0.0}

    fingerprints = retrieval.read_json(artifacts / "graph-retrieval-generation-fingerprints.json")
    assert_structure(expected["fingerprints"], fingerprints, "generation fingerprints", differences, 0.0)
    actual_results = retrieval.read_json(artifacts / "graph-retrieval-rust-results.json")
    expected_results = {
        "collection_id": expected["model"]["collection"]["collection_id"],
        "collection_version": expected["model"]["collection"]["collection_version"],
        "runs": [row["result"] for row in expected["runs"]],
        "schema_version": 3,
    }
    score_differences = {"maximum_numeric": 0.0}
    assert_structure(expected_results, actual_results, "Rust E-G results", score_differences, SCORE_TOLERANCE)
    differences["maximum_score"] = score_differences["maximum_numeric"]

    all_projections = []
    for run in expected["runs"]:
        run_id = run["identity"]["run_id"]
        assert_structure(
            run["selections"],
            retrieval.read_jsonl(artifacts / "graph-selections" / f"{run_id}.jsonl"),
            f"{run_id} selections",
            differences,
            0.0,
        )
        assert_structure(
            run["paths"],
            retrieval.read_jsonl(artifacts / "graph-paths" / f"{run_id}.jsonl"),
            f"{run_id} paths",
            differences,
            0.0,
        )
        actual_trec = (artifacts / "runs" / f"{run_id}.trec").read_text(encoding="utf-8")
        if actual_trec != run["trec"]:
            raise ValidationError(f"{run_id}: TREC rows differ")
        all_projections.extend(run["projections"])
    all_projections.sort(key=lambda row: (row["run_id"], row["query_id"]))
    assert_structure(
        all_projections,
        retrieval.read_jsonl(artifacts / "graph-retrieval-projection-identities.jsonl"),
        "projection identities",
        differences,
        0.0,
    )
    d_run_ids = {
        "explicit": "v3-d-selection-none-none-explicit-cfg-13feb2a18ac3",
        "team": "v3-d-selection-none-none-team-cfg-7278e2315c8f",
        "topic": "v3-d-selection-none-none-topic-cfg-bf6bed5c72e7",
    }
    expected_equality = {
        "runs": [
            {
                "d_run_id": d_run_ids[row["identity"]["configuration"]["seed_lane"]],
                "path_equal": True,
                "path_rows": len(row["paths"]),
                "query_count": len(row["selections"]),
                "run_id": row["identity"]["run_id"],
                "selection_equal": True,
            }
            for row in expected["runs"]
        ],
        "schema_version": 1,
        "status": "valid",
    }
    assert_structure(
        expected_equality,
        retrieval.read_json(artifacts / "graph-retrieval-selection-path-equality.json"),
        "selection/path equality with D",
        differences,
        0.0,
    )

    actual_metrics = retrieval.read_json(artifacts / "graph-retrieval-metrics.json")
    expected_metrics = {
        "collection_id": expected["model"]["collection"]["collection_id"],
        "collection_version": expected["model"]["collection"]["collection_version"],
        "metric_definition_version": "graph-retrieval-v3-r2",
        "paired_comparisons": expected["paired"],
        "partial": True,
        "publication_ready": False,
        "runs": expected["metrics"],
        "schema_version": 3,
    }
    metric_differences = {"maximum_numeric": 0.0}
    assert_structure(expected_metrics, actual_metrics, "E-G metrics", metric_differences, METRIC_TOLERANCE)
    expected_diagnostics = {
        "comparisons": expected["diagnostic_pairs"],
        "schema_version": 1,
        "status": "valid",
    }
    assert_structure(
        expected_diagnostics,
        retrieval.read_json(artifacts / "graph-retrieval-paired-comparisons.json"),
        "paired diagnostics",
        metric_differences,
        METRIC_TOLERANCE,
    )
    differences["maximum_metric"] = metric_differences["maximum_numeric"]

    expected_persistence = {
        "runs": [
            {
                "generation_equal": True,
                "path_equal": True,
                "projection_equal": True,
                "ranking_equal": True,
                "run_id": row["identity"]["run_id"],
                "save_validate_load_equivalent": True,
                "selection_equal": True,
                "stable_generation_fingerprint": row["selections"][0]["generation_fingerprint"],
            }
            for row in expected["runs"]
        ],
        "schema_version": 1,
        "status": "valid",
    }
    assert_structure(
        expected_persistence,
        retrieval.read_json(artifacts / "graph-retrieval-persistence-validation.json"),
        "persistence",
        differences,
        0.0,
    )
    return {
        "artifact_schema": REPORT_SCHEMA,
        "checked_path_rows": sum(len(row["paths"]) for row in expected["runs"]),
        "checked_query_runs": sum(len(row["selections"]) for row in expected["runs"]),
        "checked_run_ids": [row["identity"]["run_id"] for row in expected["runs"]],
        "maximum_absolute_differences": {
            "metrics": differences["maximum_metric"],
            "scores": differences["maximum_score"],
            "structural_numeric": differences["maximum_numeric"],
        },
        "partial": True,
        "publication_ready": False,
        "status": "passed",
        "tolerances": {"metrics": METRIC_TOLERANCE, "scores": SCORE_TOLERANCE},
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite independent cross-check report '{path}'")
    path.write_bytes(graph_oracle.canonical_bytes(report) + b"\n")


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
