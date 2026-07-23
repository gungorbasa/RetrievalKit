#!/usr/bin/env python3
"""Independently reconstruct and cross-check V3 Phase 1.2b Run D.

The oracle reads only the frozen collection plus finalized Rust artifacts. It
does not invoke RetrievalKit and does not use Rust graph traces as calculation
inputs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import unicodedata
from fractions import Fraction
from pathlib import Path
from typing import Any

if __package__:
    from . import validate_v3_conformance as foundation
    from .validate_v3_phase_1_2a import (
        ValidationError,
        filter_matches,
        read_json,
        read_jsonl,
        tagged_value,
        verify_frozen_fixture,
    )
else:
    import validate_v3_conformance as foundation
    from validate_v3_phase_1_2a import (
        ValidationError,
        filter_matches,
        read_json,
        read_jsonl,
        tagged_value,
        verify_frozen_fixture,
    )


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
REPORT_NAME = "graph-independent-cross-check.json"
REPORT_SCHEMA = "phase-1.2b-independent-cross-check-v1"
D_METRICS = (
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


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value, allow_nan=False, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")


def canonical(value: Any) -> str:
    return canonical_bytes(value).decode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def assert_exact(expected: Any, actual: Any, label: str) -> None:
    if expected != actual:
        raise ValidationError(
            f"{label} mismatch\nexpected={canonical(expected)}\nactual={canonical(actual)}"
        )


def record_node(node_type: str, record_id: str) -> dict[str, Any]:
    return {
        "node_type": node_type,
        "source": {"kind": "record", "record_id": record_id},
    }


def identity(record_id: str, chunk_key: str) -> dict[str, str]:
    return {"chunk_key": chunk_key, "record_id": record_id}


def load_collection(collection: Path) -> dict[str, Any]:
    header = read_json(collection / "collection.json")
    records = read_jsonl(collection / "records.jsonl")
    queries = read_jsonl(collection / "queries.jsonl")
    schema = read_json(collection / "graph-schema.json")
    seed_policy = read_json(collection / "manifests/seed-policy.json")
    evidence = {row["query_id"]: row for row in read_jsonl(collection / "evidence-judgments.jsonl")}
    expected_paths = {
        (row["seed_policy"], row["query_id"]): row
        for row in read_jsonl(collection / "expected-paths.jsonl")
    }
    exclusions = {
        (row["lane"], row["query_id"]): row for row in read_jsonl(collection / "exclusions.jsonl")
    }
    return {
        "collection": header,
        "evidence": evidence,
        "exclusions": exclusions,
        "expected_paths": expected_paths,
        "queries": {row["query_id"]: row for row in queries},
        "records": records,
        "schema": schema,
        "seed_policy": seed_policy,
    }


def frozen_d_runs(collection: Path) -> list[dict[str, Any]]:
    files = {
        entry["path"]: (collection / entry["path"]).read_bytes()
        for entry in read_json(collection / "collection.json")["files"]
    }
    files["collection.json"] = (collection / "collection.json").read_bytes()
    runs = [run for run in foundation.derive_runs(files) if run["configuration"]["run_letter"] == "d"]
    if len(runs) != 3:
        raise ValidationError(f"expected three independently derived D runs, actual {len(runs)}")
    return runs


def generation_fingerprint(collection: Path, header: dict[str, Any]) -> tuple[dict[str, Any], str]:
    def file_array(paths: list[str]) -> str:
        rows = [
            {"path": path, "sha256": sha256((collection / path).read_bytes())}
            for path in sorted(paths)
        ]
        return sha256(canonical_bytes(rows))

    preimage = {
        "corpus_id": header["corpus_id"],
        "corpus_state_sha256": file_array(
            ["manifests/chunking.json", "manifests/preprocessing.json", "records.jsonl"]
        ),
        "graph_state_sha256": file_array(
            ["graph-schema.json", "manifests/graph-construction.json"]
        ),
        "retrieval_state_sha256": None,
        "schema_version": 1,
    }
    return preimage, sha256(canonical_bytes(preimage))


def build_graph(model: dict[str, Any]) -> dict[str, Any]:
    node_type_by_record_type = {
        row["record_type"]: row["node_type"] for row in model["schema"]["record_nodes"]
    }
    records_by_id = {row["record_id"]: row for row in model["records"]}
    nodes = {
        record_id: record_node(node_type_by_record_type[row["record_type"]], record_id)
        for record_id, row in records_by_id.items()
    }
    edges: list[dict[str, Any]] = []
    for relationship in model["schema"]["relationships"]:
        source_type = relationship["source_node_type"]
        field = relationship["source_field"][0]
        for record_id in sorted(records_by_id):
            source = nodes[record_id]
            if source["node_type"] != source_type:
                continue
            tagged = records_by_id[record_id]["fields"].get(field)
            if tagged is None:
                continue
            values = tagged["value"] if tagged["type"] == "list" else [tagged]
            for ordinal, value in enumerate(values):
                target_id = tagged_value(value)
                edges.append(
                    {
                        "occurrence_ordinal": ordinal,
                        "relationship_type": relationship["relationship_type"],
                        "source_node": source,
                        "target_node": nodes[target_id],
                    }
                )
    chunks: list[dict[str, Any]] = []
    for record_id in sorted(records_by_id):
        record = records_by_id[record_id]
        inherited = {field: tagged_value(value) for field, value in record["metadata"].items()}
        for chunk in sorted(record["chunks"], key=lambda row: row["chunk_key"]):
            metadata = dict(inherited)
            metadata.update(
                {field: tagged_value(value) for field, value in chunk["metadata"].items()}
            )
            chunks.append(
                {
                    "identity": identity(record_id, chunk["chunk_key"]),
                    "metadata": metadata,
                    "record_id": record_id,
                }
            )
    return {"chunks": chunks, "edges": edges, "nodes": nodes}


def normalize_text(text: str) -> str:
    # The frozen strings are ASCII; this still applies the declared NFC/full-fold/
    # Unicode-whitespace pipeline without relying on RetrievalKit.
    folded = unicodedata.normalize("NFC", text).casefold()
    return " ".join(folded.split())


def derive_seeds(model: dict[str, Any]) -> tuple[dict[tuple[str, str], dict[str, Any]], list[dict[str, Any]]]:
    parameters = model["seed_policy"]["parameters"]
    normalization_version = parameters["normalization"]["normalization_version"]
    resolved: dict[tuple[str, str], dict[str, Any]] = {}
    diagnostics: list[dict[str, Any]] = []
    explicit_provenance = {
        row["query_id"]: row for row in parameters["explicit_policy"]["provenance"]
    }
    for query_id, query in model["queries"].items():
        if query["explicit_seed"] is None:
            continue
        provenance = explicit_provenance[query_id]
        resolved[("explicit", query_id)] = {
            "canonical": query["explicit_seed"],
            "provenance": {
                "kind": "explicit",
                "source_id": provenance["source_id"],
                "transformation_id": provenance["transformation_id"],
            },
        }
    for policy in parameters["derived_policies"]:
        policy_id = policy["policy_id"]
        policy_sha = sha256(canonical_bytes(policy))
        for query_id, query in sorted(model["queries"].items()):
            if query["derived_seed_policy_id"] != policy_id:
                continue
            normalized = normalize_text(query["text"])
            matches: list[dict[str, Any]] = []
            longest = -1
            for alias in policy["aliases"]:
                needle = alias["normalized_alias"]
                start = normalized.find(needle)
                if start < 0:
                    continue
                end = start + len(needle)
                if len(needle) > longest:
                    matches = []
                    longest = len(needle)
                if len(needle) == longest:
                    matches.append(
                        {
                            "alias": alias["alias"],
                            "normalized_end": end,
                            "normalized_start": start,
                            "original_end": end,
                            "original_start": start,
                            "seed": alias["seed"],
                            "source": alias["source"],
                        }
                    )
            matches.sort(key=canonical)
            candidates = sorted({canonical(row["seed"]): row["seed"] for row in matches}.values(), key=canonical)
            failure = None
            selected = None
            if not candidates:
                failure = "derived_seed_no_match"
            elif len(candidates) > 1:
                failure = "derived_seed_ambiguous"
            else:
                selected = candidates[0]
                resolved[(policy_id, query_id)] = {
                    "canonical": selected,
                    "provenance": {
                        "alias_table_sha256": policy["alias_table_sha256"],
                        "kind": "derived",
                        "matched_aliases": matches,
                        "normalization_version": normalization_version,
                        "policy_id": policy_id,
                        "policy_sha256": policy_sha,
                        "policy_version": policy["policy_version"],
                    },
                }
            diagnostics.append(
                {
                    "alias_table_sha256": policy["alias_table_sha256"],
                    "candidate_seeds": candidates,
                    "failure_reason": failure,
                    "matched_aliases": matches,
                    "normalization_version": normalization_version,
                    "policy_id": policy_id,
                    "policy_sha256": policy_sha,
                    "policy_version": policy["policy_version"],
                    "query_id": query_id,
                    "selected_seed": selected,
                }
            )
    diagnostics.sort(key=lambda row: (row["policy_id"], row["query_id"]))
    return resolved, diagnostics


def traverse(graph: dict[str, Any], seed: dict[str, Any], query: dict[str, Any]) -> tuple[list[dict[str, Any]], dict[str, int]]:
    states = [(node, []) for node in seed["nodes"]]
    traversed_edges = 0
    for step in query["traversal"]["steps"]:
        output: list[tuple[dict[str, Any], list[dict[str, Any]]]] = []
        frontier = states
        seen = {canonical(node) for node, _ in frontier}
        for hop in range(step["max_hops"] + 1):
            if hop >= step["min_hops"]:
                output.extend(frontier)
            if hop == step["max_hops"]:
                break
            next_frontier = []
            for node, path in frontier:
                for edge in graph["edges"]:
                    if edge["relationship_type"] != step["relationship_type"]:
                        continue
                    if step["direction"] == "outgoing" and edge["source_node"] == node:
                        neighbor = edge["target_node"]
                    elif step["direction"] == "incoming" and edge["target_node"] == node:
                        neighbor = edge["source_node"]
                    else:
                        continue
                    traversed_edges += 1
                    key = canonical(neighbor)
                    if key not in seen:
                        seen.add(key)
                        next_frontier.append((neighbor, path + [edge]))
            frontier = next_frontier
        states = output
    unique = {(canonical(node), canonical(path)): (node, path) for node, path in states}
    matches = [
        {"node": node, "path": path}
        for node, path in sorted(unique.values(), key=lambda row: (canonical(row[0]), canonical(row[1])))
    ]
    trace = {
        "diagnostics": 0,
        "result_count": len(matches),
        "seed_count": len(seed["nodes"]),
        "traversed_edges": traversed_edges,
        "visited_states": 1 if not query["traversal"]["steps"] else 1 + len(matches),
    }
    return matches, trace


def path_row(match: dict[str, Any], query_id: str, run_id: str) -> dict[str, Any]:
    matched = match["node"]
    path = match["path"]
    current = matched
    directions: list[str] = []
    for edge in reversed(path):
        if edge["target_node"] == current:
            directions.append("outgoing")
            current = edge["source_node"]
        elif edge["source_node"] == current:
            directions.append("incoming")
            current = edge["target_node"]
        else:
            raise ValidationError("independent graph path is not contiguous")
    directions.reverse()
    edges = [
        {
            "direction": direction,
            "occurrence_ordinal": edge["occurrence_ordinal"],
            "relationship_type": edge["relationship_type"],
            "source_node": edge["source_node"],
            "target_node": edge["target_node"],
        }
        for edge, direction in zip(path, directions, strict=True)
    ]
    return {
        "depth": len(edges),
        "edges": edges,
        "matched_node": matched,
        "path_ordinal": 0,
        "query_id": query_id,
        "run_id": run_id,
    }


def best_evidence(documents: set[str], judgment: dict[str, Any]) -> tuple[int, int]:
    choices = []
    for required in judgment["evidence_sets"]:
        matched = len(documents & set(required))
        choices.append((matched, len(required), canonical(required)))
    choices.sort(key=lambda row: (-Fraction(row[0], row[1]), -row[0], row[1], row[2]))
    return choices[0][0], choices[0][1]


def metric(status: str, value: float | None = None) -> dict[str, Any]:
    return {"status": status, "value": value}


def query_metrics(model: dict[str, Any], lane: str, execution: dict[str, Any]) -> dict[str, Any]:
    query = execution["query"]
    projected = len(execution["identities"])
    values = {name: metric("not_applicable") for name in D_METRICS}
    values["candidate_reduction_ratio"] = (
        metric("undefined")
        if projected == 0
        else metric("valid", execution["eligible_chunks"] / projected)
    )
    values["empty_scope"] = metric("valid", 1.0 if projected == 0 else 0.0)
    for name in (
        "truncated",
        "truncated_max_hops",
        "truncated_max_results",
        "truncated_max_visited",
        "truncated_max_working_bytes",
    ):
        values[name] = metric("valid", 0.0)
    if "evidence" in query["tasks"]:
        matched, required = best_evidence(
            execution["documents"], model["evidence"][query["query_id"]]
        )
        values["candidate_recall"] = metric("valid", matched / required)
        values["candidate_complete_evidence"] = metric(
            "valid", 1.0 if matched == required else 0.0
        )
    if "path" in query["tasks"]:
        expected = model["expected_paths"].get((lane, query["query_id"]))
        if expected is not None:
            actual_paths = {canonical(row["edges"]) for row in execution["paths"]}
            expected_paths = {canonical(path) for path in expected["expected_paths"]}
            values["path_accuracy"] = metric(
                "valid", 1.0 if actual_paths & expected_paths else 0.0
            )
    return values


def macro_metrics(queries: list[dict[str, Any]]) -> dict[str, Any]:
    output = {}
    statuses = ("excluded_pre_freeze", "invalid_execution", "not_applicable", "undefined", "valid")
    for name in D_METRICS:
        counts = {status: 0 for status in statuses}
        values = []
        for query in queries:
            row = query["metrics"][name]
            counts[row["status"]] += 1
            if row["status"] == "valid":
                values.append(row["value"])
        numerator = sum(values)
        output[name] = {
            "denominator": len(values),
            "numerator": numerator,
            "status_counts": counts,
            "value": None if not values else numerator / len(values),
        }
    return output


def micro_metrics(model: dict[str, Any], executions: list[dict[str, Any]]) -> dict[str, Any]:
    matched = required = eligible = projected = empty = 0
    for execution in executions:
        eligible += execution["eligible_chunks"]
        projected += len(execution["identities"])
        empty += int(not execution["identities"])
        query = execution["query"]
        if "evidence" in query["tasks"]:
            selected = best_evidence(execution["documents"], model["evidence"][query["query_id"]])
            matched += selected[0]
            required += selected[1]
    count = len(executions)
    def ratio(left: int, right: int) -> float | None:
        return None if right == 0 else left / right

    return {
        "candidate_recall": {"matched_documents": matched, "required_documents": required, "value": ratio(matched, required)},
        "candidate_reduction_ratio": {"candidate_chunks": projected, "eligible_chunks": eligible, "value": ratio(eligible, projected)},
        "empty_scope_rate": {"empty_scopes": empty, "graph_valid_queries": count, "value": ratio(empty, count)},
        "supporting_document_recall_at_10": {"matched_documents": 0, "required_documents": 0, "value": None},
        "supporting_document_recall_at_5": {"matched_documents": 0, "required_documents": 0, "value": None},
        "truncation_rate": {"affected_queries": 0, "graph_valid_queries": count, "value": ratio(0, count)},
        "truncation_rate_max_hops": {"affected_queries": 0, "graph_valid_queries": count, "value": ratio(0, count)},
        "truncation_rate_max_results": {"affected_queries": 0, "graph_valid_queries": count, "value": ratio(0, count)},
        "truncation_rate_max_visited": {"affected_queries": 0, "graph_valid_queries": count, "value": ratio(0, count)},
        "truncation_rate_max_working_bytes": {"affected_queries": 0, "graph_valid_queries": count, "value": ratio(0, count)},
    }


def execute(model: dict[str, Any], runs: list[dict[str, Any]], fingerprint: str) -> dict[str, Any]:
    graph = build_graph(model)
    seeds, diagnostics = derive_seeds(model)
    all_chunks = graph["chunks"]
    run_artifacts = []
    for run in runs:
        lane = run["configuration"]["seed_lane"]
        selections = []
        paths = []
        projections = []
        executions = []
        result_queries = []
        for query_id in run["declared_population"]:
            query = model["queries"][query_id]
            if query_id not in run["execution_population"]:
                exclusion = model["exclusions"][(lane, query_id)]
                result_queries.append(result_query(query, run, "excluded_pre_freeze", exclusion["reason"]))
                continue
            seed = seeds[(lane, query_id)]
            matches, trace = traverse(graph, seed["canonical"], query)
            matched_nodes = sorted((match["node"] for match in matches), key=canonical)
            projected_records = {node["source"]["record_id"] for node in matched_nodes}
            before_filter = [chunk for chunk in all_chunks if chunk["record_id"] in projected_records]
            filtered = [
                chunk for chunk in before_filter if filter_matches(query["metadata_filter"], chunk["metadata"])
            ]
            eligible = [
                chunk for chunk in all_chunks if filter_matches(query["metadata_filter"], chunk["metadata"])
            ]
            identities = [chunk["identity"] for chunk in filtered]
            documents = {row["record_id"] for row in identities}
            query_paths = [path_row(match, query_id, run["run_id"]) for match in matches]
            query_paths.sort(key=lambda row: (row["query_id"], canonical(row["matched_node"]), canonical(row["edges"])))
            paths.extend(query_paths)
            execution = {
                "documents": documents,
                "eligible_chunks": len(eligible),
                "identities": identities,
                "paths": query_paths,
                "query": query,
            }
            execution["metrics"] = query_metrics(model, lane, execution)
            executions.append(execution)
            selections.append(
                {
                    "active_corpus_chunks_before_filter": len(all_chunks),
                    "corpus_id": model["collection"]["corpus_id"],
                    "eligible_corpus_chunks_after_filter": len(eligible),
                    "generation_fingerprint": fingerprint,
                    "matched_nodes": matched_nodes,
                    "projected_chunks_after_filter": len(identities),
                    "projected_chunks_before_filter": len(before_filter),
                    "projected_documents_after_filter": len(documents),
                    "query_id": query_id,
                    "resolved_seed": seed["canonical"],
                    "run_id": run["run_id"],
                    "seed_lane": lane,
                    "seed_provenance": seed["provenance"],
                    "seed_status": "resolved",
                    "stale": False,
                    "trace": trace,
                    "truncated_reason": None,
                }
            )
            projections.append({"candidates": identities, "query_id": query_id, "run_id": run["run_id"]})
            result_queries.append(result_query(query, run, "valid", None))
        selections.sort(key=lambda row: row["query_id"])
        paths.sort(key=lambda row: (row["query_id"], canonical(row["matched_node"]), canonical(row["edges"])))
        result_queries.sort(key=lambda row: row["query_id"])
        metric_queries = []
        by_id = {row["query"]["query_id"]: row for row in executions}
        for query_id in run["declared_population"]:
            if query_id in by_id:
                row = by_id[query_id]
                metric_queries.append(
                    {
                        "candidate_counts": {"eligible_chunks": row["eligible_chunks"], "projected_chunks": len(row["identities"])},
                        "execution_status": "valid",
                        "metrics": row["metrics"],
                        "query_id": query_id,
                    }
                )
            else:
                metric_queries.append(
                    {
                        "candidate_counts": None,
                        "execution_status": "excluded_pre_freeze",
                        "metrics": {name: metric("excluded_pre_freeze") for name in D_METRICS},
                        "query_id": query_id,
                    }
                )
        metrics = {
            "counts": {
                "attempted": len(run["execution_population"]),
                "declared": len(run["declared_population"]),
                "excluded_pre_freeze": len(run["declared_population"]) - len(run["execution_population"]),
                "invalid_execution": 0,
                "valid_execution": len(run["execution_population"]),
            },
            "declared_population_sha256": foundation.population_hash(run["declared_population"]),
            "execution_population_sha256": foundation.population_hash(run["execution_population"]),
            "macro": macro_metrics(metric_queries),
            "micro": micro_metrics(model, executions),
            "queries": metric_queries,
            "run_id": run["run_id"],
            "status": "valid",
        }
        run_artifacts.append(
            {
                "metrics": metrics,
                "paths": paths,
                "projections": projections,
                "result": {"queries": result_queries, "run_id": run["run_id"], "status": "valid"},
                "run_id": run["run_id"],
                "selections": selections,
            }
        )
    run_artifacts.sort(key=lambda row: row["run_id"])
    return {"diagnostics": diagnostics, "runs": run_artifacts}


def result_query(query: dict[str, Any], run: dict[str, Any], status: str, reason: str | None) -> dict[str, Any]:
    return {
        "candidate_limits": {"keyword": None, "vector": None},
        "chunk_hits": [],
        "duplicate_collapse_count": 0,
        "execution_status": status,
        "filter": query["metadata_filter"],
        "projected_documents": [],
        "query_id": query["query_id"],
        "selection_run_id": run["run_id"],
        "status_reason": reason,
    }


def validate(collection: Path, artifacts: Path) -> dict[str, Any]:
    verify_frozen_fixture(collection)
    model = load_collection(collection)
    runs = frozen_d_runs(collection)
    preimage, fingerprint = generation_fingerprint(collection, model["collection"])
    expected = execute(model, runs, fingerprint)

    fingerprint_artifact = read_json(artifacts / "graph-generation-fingerprint.json")
    assert_exact(
        {"fingerprint": fingerprint, "preimage": preimage, "schema_version": 1},
        fingerprint_artifact,
        "graph generation fingerprint",
    )
    seed_artifact = read_json(artifacts / "seed-resolution-diagnostics.json")
    assert_exact(
        {"schema_version": 3, "seed_resolutions": expected["diagnostics"]},
        seed_artifact,
        "seed resolution diagnostics",
    )

    all_projections = []
    for run in expected["runs"]:
        actual_selections = read_jsonl(artifacts / "graph-selections" / f"{run['run_id']}.jsonl")
        actual_paths = read_jsonl(artifacts / "graph-paths" / f"{run['run_id']}.jsonl")
        assert_exact(run["selections"], actual_selections, f"{run['run_id']} selections")
        assert_exact(run["paths"], actual_paths, f"{run['run_id']} paths")
        all_projections.extend(run["projections"])
    all_projections.sort(key=lambda row: (row["run_id"], row["query_id"]))
    assert_exact(
        all_projections,
        read_jsonl(artifacts / "graph-projection-identities.jsonl"),
        "stable candidate identities",
    )

    expected_metrics = {
        "collection_id": model["collection"]["collection_id"],
        "collection_version": model["collection"]["collection_version"],
        "generation_fingerprint": fingerprint,
        "metric_definition_version": "graph-retrieval-v3-r2",
        "partial": True,
        "publication_ready": False,
        "runs": [run["metrics"] for run in expected["runs"]],
        "schema_version": 3,
    }
    assert_exact(expected_metrics, read_json(artifacts / "graph-metrics.json"), "D metrics")
    expected_results = {
        "collection_id": model["collection"]["collection_id"],
        "collection_version": model["collection"]["collection_version"],
        "runs": [run["result"] for run in expected["runs"]],
        "schema_version": 3,
        "seed_resolutions": expected["diagnostics"],
    }
    assert_exact(expected_results, read_json(artifacts / "graph-rust-results.json"), "Rust D results")

    persistence = read_json(artifacts / "graph-persistence-validation.json")
    assert_exact(
        {
            "runs": [
                {
                    "run_id": run["run_id"],
                    "save_validate_load_equivalent": True,
                    "stable_generation_fingerprint": fingerprint,
                }
                for run in expected["runs"]
            ],
            "schema_version": 1,
            "status": "valid",
        },
        persistence,
        "persistence validation",
    )
    return {
        "artifact_schema": REPORT_SCHEMA,
        "checked_path_rows": sum(len(run["paths"]) for run in expected["runs"]),
        "checked_query_runs": sum(len(run["selections"]) for run in expected["runs"]),
        "generation_fingerprint": fingerprint,
        "included_run_letters": ["d"],
        "partial": True,
        "publication_ready": False,
        "run_ids": [run["run_id"] for run in expected["runs"]],
        "status": "passed",
    }


def write_report(path: Path, report: dict[str, Any]) -> None:
    if path.exists():
        raise ValidationError(f"refusing to overwrite graph cross-check report '{path}'")
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
