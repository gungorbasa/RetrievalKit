#!/usr/bin/env python3
"""Generate and independently validate the synthetic V3 foundation fixture.

This script deliberately does not invoke the Rust evaluator. It constructs every
canonical collection byte stream, population, run configuration, logical run
identity, and generation fingerprint from the frozen synthetic source model.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_COLLECTION = ROOT / "benchmarks/retrieval-quality/v3"
NORMATIVE_AJ_SHA256 = "4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46"
BASE_GIT_COMMIT = "d145b76ef60b964dcf004516fc4b94b00147d7c7"
FOUNDATION_BINARY_BYTES = b"RetrievalKit V3 conformance foundation executable identity v1\n"
LOWERCASE_TABLE_SHA256 = "480dea577027cc707c769048f775be3aafff871a74c41efcbe0eff8314f269fc"


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(
            value,
            allow_nan=False,
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        + b"\n"
    )


def compact_bytes(value: Any) -> bytes:
    return canonical_bytes(value)[:-1]


def jsonl_bytes(rows: list[dict[str, Any]]) -> bytes:
    return b"".join(canonical_bytes(row) for row in rows)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def population_hash(ids: set[str] | list[str]) -> str:
    return sha256(b"".join(value.encode("utf-8") + b"\n" for value in sorted(ids)))


def tagged_string(value: str) -> dict[str, Any]:
    return {"type": "string", "value": value}


def tagged_strings(values: list[str]) -> dict[str, Any]:
    return {"type": "list", "value": [tagged_string(value) for value in values]}


def metadata_string(value: str) -> dict[str, Any]:
    return {"type": "string", "value": value}


def metadata_timestamp(value: int) -> dict[str, Any]:
    return {"type": "timestamp_millis", "value": value}


def record_node(node_type: str, record_id: str) -> dict[str, Any]:
    return {
        "node_type": node_type,
        "source": {"kind": "record", "record_id": record_id},
    }


def node_seed(node_type: str, record_id: str) -> dict[str, Any]:
    return {"kind": "node_ids", "nodes": [record_node(node_type, record_id)]}


def limits() -> dict[str, int]:
    return {
        "max_hops": 3,
        "max_results": 16,
        "max_visited": 64,
        "max_working_bytes": 65536,
    }


def traversal(
    relationship_type: str | None = None,
    direction: str = "outgoing",
) -> dict[str, Any]:
    steps: list[dict[str, Any]] = []
    if relationship_type is not None:
        steps.append(
            {
                "direction": direction,
                "max_hops": 1,
                "min_hops": 0,
                "relationship_type": relationship_type,
            }
        )
    return {"limits": limits(), "steps": steps}


def query(
    query_id: str,
    category: str,
    text: str,
    tasks: list[str],
    *,
    explicit_seed: dict[str, Any] | None = None,
    derived_policy: str | None = None,
    metadata_filter: dict[str, Any] | None = None,
    query_traversal: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "category": category,
        "derived_seed_policy_id": derived_policy,
        "explicit_seed": explicit_seed,
        "metadata_filter": metadata_filter,
        "query_id": query_id,
        "split": "development",
        "tasks": sorted(tasks),
        "text": text,
        "traversal": query_traversal or traversal(),
    }


def fixture_model() -> dict[str, Any]:
    records = [
        {
            "chunks": [
                {
                    "chunk_key": "details",
                    "metadata": {"tenant": metadata_string("red")},
                    "text": "Alpha battery details for the red tenant.",
                },
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Alpha is a battery topic linked to Beta and Gamma.",
                },
            ],
            "content": "Alpha canonical record payload.",
            "fields": {
                "related_ids": tagged_strings(["beta", "gamma"]),
                "title": tagged_string("Alpha"),
            },
            "metadata": {
                "created_ms": metadata_timestamp(1_735_689_600_000),
                "tenant": metadata_string("blue"),
            },
            "record_id": "alpha",
            "record_type": "Topic",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Beta architecture connects to Gamma.",
                }
            ],
            "content": None,
            "fields": {
                "related_ids": tagged_strings(["gamma"]),
                "title": tagged_string("Beta"),
            },
            "metadata": {"tenant": metadata_string("red")},
            "record_id": "beta",
            "record_type": "Topic",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Gamma contains the final battery evidence.",
                }
            ],
            "content": None,
            "fields": {
                "related_ids": tagged_strings([]),
                "title": tagged_string("Gamma"),
            },
            "metadata": {"tenant": metadata_string("red")},
            "record_id": "gamma",
            "record_type": "Topic",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "The Mobile team owns the Phone product.",
                }
            ],
            "content": None,
            "fields": {
                "owns_ids": tagged_strings(["phone"]),
                "title": tagged_string("Mobile"),
            },
            "metadata": {"tenant": metadata_string("blue")},
            "record_id": "mobile",
            "record_type": "Team",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Phone battery support covers Alpha.",
                }
            ],
            "content": None,
            "fields": {
                "covered_topic": tagged_string("alpha"),
                "title": tagged_string("Phone"),
            },
            "metadata": {"tenant": metadata_string("red")},
            "record_id": "phone",
            "record_type": "Product",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Shared east is an ambiguity witness.",
                }
            ],
            "content": None,
            "fields": {
                "related_ids": tagged_strings([]),
                "title": tagged_string("Shared"),
            },
            "metadata": {"tenant": metadata_string("blue")},
            "record_id": "shared-east",
            "record_type": "Topic",
        },
        {
            "chunks": [
                {
                    "chunk_key": "summary",
                    "metadata": {},
                    "text": "Shared west is a second ambiguity witness.",
                }
            ],
            "content": None,
            "fields": {
                "related_ids": tagged_strings([]),
                "title": tagged_string("Shared"),
            },
            "metadata": {"tenant": metadata_string("blue")},
            "record_id": "shared-west",
            "record_type": "Topic",
        },
    ]
    graph_schema = {
        "chunk_nodes": {
            "inverse_relationship": "chunk_of",
            "node_type": "Chunk",
            "owns_relationship": "has_chunk",
        },
        "record_nodes": [
            {
                "node_type": "Topic",
                "queryable_fields": [["title"]],
                "record_type": "Topic",
            },
            {
                "node_type": "Team",
                "queryable_fields": [["title"]],
                "record_type": "Team",
            },
            {
                "node_type": "Product",
                "queryable_fields": [["title"]],
                "record_type": "Product",
            },
        ],
        "relationships": [
            {
                "allow_self_edge": False,
                "cardinality": "many",
                "duplicate_references": "error",
                "inverse_relationship": "related_by",
                "missing_target": "error",
                "relationship_type": "related",
                "source_field": ["related_ids"],
                "source_node_type": "Topic",
                "target_node_type": "Topic",
            },
            {
                "allow_self_edge": False,
                "cardinality": "many",
                "duplicate_references": "error",
                "inverse_relationship": "owned_by",
                "missing_target": "error",
                "relationship_type": "owns",
                "source_field": ["owns_ids"],
                "source_node_type": "Team",
                "target_node_type": "Product",
            },
            {
                "allow_self_edge": False,
                "cardinality": "one",
                "duplicate_references": "error",
                "inverse_relationship": "covered_by",
                "missing_target": "error",
                "relationship_type": "covers",
                "source_field": ["covered_topic"],
                "source_node_type": "Product",
                "target_node_type": "Topic",
            },
        ],
        "version": 1,
    }
    tenant_red = {"field": "tenant", "op": "equals", "value": metadata_string("red")}
    queries = [
        query("qa", "semantic", "battery topic", ["retrieval"]),
        query(
            "qb",
            "explicit-filtered",
            "phone battery",
            ["retrieval"],
            explicit_seed=node_seed("Team", "mobile"),
            metadata_filter=tenant_red,
            query_traversal=traversal("owns"),
        ),
        query(
            "qc",
            "explicit-path",
            "alpha evidence alternatives",
            ["evidence", "path"],
            explicit_seed=node_seed("Topic", "alpha"),
        ),
        query(
            "qd",
            "topic-success-filtered",
            "Alpha battery evidence",
            ["evidence", "retrieval"],
            derived_policy="topic",
            metadata_filter=tenant_red,
            query_traversal=traversal("related"),
        ),
        query(
            "qe",
            "topic-path",
            "Beta architecture path",
            ["evidence", "path"],
            derived_policy="topic",
            query_traversal=traversal("related"),
        ),
        query(
            "qf",
            "topic-no-match",
            "unknown resolver phrase",
            ["retrieval"],
            derived_policy="topic",
            query_traversal=traversal("related"),
        ),
        query(
            "qg",
            "topic-ambiguous",
            "Shared policy details",
            ["retrieval"],
            derived_policy="topic",
            query_traversal=traversal("related"),
        ),
        query(
            "qh",
            "dual-lane",
            "Gamma red tenant evidence",
            ["evidence", "path", "retrieval"],
            explicit_seed=node_seed("Topic", "alpha"),
            derived_policy="topic",
            metadata_filter={
                "children": [
                    tenant_red,
                    {"field": "tenant", "op": "exists"},
                ],
                "op": "all",
            },
            query_traversal=traversal("related", "incoming"),
        ),
        query(
            "qi",
            "team-derived",
            "Mobile team phone",
            ["path", "retrieval"],
            derived_policy="team",
            metadata_filter={
                "field": "tenant",
                "op": "equals",
                "value": metadata_string("blue"),
            },
            query_traversal=traversal("owns"),
        ),
    ]
    qrels = [
        ("qa", "alpha", 2),
        ("qa", "shared-east", 0),
        ("qb", "beta", 0),
        ("qb", "phone", 2),
        ("qd", "alpha", 2),
        ("qd", "gamma", 1),
        ("qd", "phone", 0),
        ("qf", "beta", 1),
        ("qf", "shared-west", 0),
        ("qg", "shared-east", 1),
        ("qg", "shared-west", 0),
        ("qh", "alpha", 0),
        ("qh", "gamma", 2),
        ("qi", "mobile", 2),
        ("qi", "phone", 0),
    ]
    evidence = [
        {"evidence_sets": [["alpha", "beta"], ["alpha", "gamma"]], "query_id": "qc"},
        {"evidence_sets": [["alpha", "gamma"], ["alpha", "phone"]], "query_id": "qd"},
        {"evidence_sets": [["beta", "gamma"]], "query_id": "qe"},
        {"evidence_sets": [["alpha", "gamma"], ["gamma", "phone"]], "query_id": "qh"},
    ]

    def edge(
        relationship_type: str,
        direction: str,
        source: tuple[str, str],
        target: tuple[str, str],
        ordinal: int = 0,
    ) -> dict[str, Any]:
        return {
            "direction": direction,
            "occurrence_ordinal": ordinal,
            "relationship_type": relationship_type,
            "source_node": record_node(*source),
            "target_node": record_node(*target),
        }

    expected_paths = [
        {"expected_paths": [[]], "query_id": "qc", "seed_policy": "explicit"},
        {
            "expected_paths": [[edge("related", "outgoing", ("Topic", "beta"), ("Topic", "gamma"))]],
            "query_id": "qe",
            "seed_policy": "topic",
        },
        {
            "expected_paths": [[edge("related", "incoming", ("Topic", "alpha"), ("Topic", "gamma"))]],
            "query_id": "qh",
            "seed_policy": "explicit",
        },
        {
            "expected_paths": [[edge("related", "incoming", ("Topic", "alpha"), ("Topic", "gamma"))]],
            "query_id": "qh",
            "seed_policy": "topic",
        },
    ]
    exclusions = [
        {
            "details": "No frozen topic alias matches the normalized query.",
            "lane": "topic",
            "phase": "pre_freeze",
            "query_id": "qf",
            "reason": "derived_seed_no_match",
            "source": "synthetic-adapter",
        },
        {
            "details": "The longest Shared alias maps to two distinct topic nodes.",
            "lane": "topic",
            "phase": "pre_freeze",
            "query_id": "qg",
            "reason": "derived_seed_ambiguous",
            "source": "synthetic-adapter",
        },
        {
            "details": "Normative case J has no relevant documents.",
            "lane": "global",
            "phase": "pre_freeze",
            "query_id": "qj",
            "reason": "no_relevant_documents",
            "source": "synthetic-adapter",
        },
    ]
    corpus_embeddings = []
    vectors = [
        [1, 0, 0],
        [0.5, 0.5, 0],
        [0, 1, 0],
        [0, 0.5, 0.5],
        [0, 0, 1],
        [-0.5, 0.5, 0],
        [-1, 0, 0],
        [-0.5, -0.5, 0],
    ]
    vector_offset = 0
    for record in records:
        for chunk in record["chunks"]:
            corpus_embeddings.append(
                {
                    "chunk_key": chunk["chunk_key"],
                    "record_id": record["record_id"],
                    "values": vectors[vector_offset],
                }
            )
            vector_offset += 1
    query_vectors = {
        "qa": [1, 0, 0],
        "qb": [0, 0, 1],
        "qd": [0.5, 0.5, 0],
        "qf": [0, 1, 0],
        "qg": [-0.5, 0.5, 0],
        "qh": [0, 0.5, 0.5],
        "qi": [0, 0, 1],
    }
    query_embeddings = [
        {"query_id": query_id, "values": values}
        for query_id, values in sorted(query_vectors.items())
    ]
    return {
        "corpus_embeddings": corpus_embeddings,
        "evidence": evidence,
        "exclusions": exclusions,
        "expected_paths": expected_paths,
        "graph_schema": graph_schema,
        "qrels": qrels,
        "queries": queries,
        "query_embeddings": query_embeddings,
        "records": records,
    }


def derive_populations(model: dict[str, Any]) -> dict[str, set[str]]:
    queries = model["queries"]
    exclusions = model["exclusions"]
    populations: dict[str, set[str]] = {
        "Q": {row["query_id"] for row in queries},
        "R": {row["query_id"] for row in queries if "retrieval" in row["tasks"]},
        "X_exp": {row["query_id"] for row in queries if row["explicit_seed"] is not None},
    }
    populations["S_exp"] = set(populations["X_exp"])
    for policy in sorted(
        {row["derived_seed_policy_id"] for row in queries if row["derived_seed_policy_id"]}
    ):
        declared = {
            row["query_id"]
            for row in queries
            if row["derived_seed_policy_id"] == policy
        }
        failed = {
            row["query_id"]
            for row in exclusions
            if row["lane"] == policy
            and row["reason"] in {"derived_seed_no_match", "derived_seed_ambiguous"}
        }
        populations[f"X_{policy}"] = declared
        populations[f"F_{policy}"] = failed
        populations[f"S_{policy}"] = declared - failed
    return populations


def reported_populations(model: dict[str, Any]) -> dict[str, set[str]]:
    populations = derive_populations(model)
    result = dict(populations)
    result["X_exp_intersect_R"] = populations["X_exp"] & populations["R"]
    result["X_topic_intersect_R"] = populations["X_topic"] & populations["R"]
    result["S_topic_intersect_R"] = populations["S_topic"] & populations["R"]
    result["X_team_intersect_R"] = populations["X_team"] & populations["R"]
    result["S_team_intersect_R"] = populations["S_team"] & populations["R"]
    return result


def alias_row(alias: str, policy: str, record_id: str) -> dict[str, Any]:
    node_type = "Team" if policy == "team" else "Topic"
    return {
        "alias": alias,
        "normalized_alias": alias.lower(),
        "seed": node_seed(node_type, record_id),
        "source": {"field": ["title"], "record_id": record_id},
    }


def source_streams(model: dict[str, Any]) -> dict[str, bytes]:
    return {
        "upstream/corpus/synthetic-records-v1": canonical_bytes(model["records"]),
        "upstream/graph/synthetic-schema-v1": canonical_bytes(model["graph_schema"]),
        "upstream/judgment/synthetic-judgments-v1": canonical_bytes(
            {"evidence": model["evidence"], "qrels": model["qrels"]}
        ),
        "upstream/license/synthetic-license-v1": b"CC0-1.0 synthetic conformance data\n",
        "upstream/model/synthetic-3d-f32-v1": canonical_bytes(
            {
                "corpus": model["corpus_embeddings"],
                "queries": model["query_embeddings"],
            }
        ),
        "upstream/query/synthetic-queries-v1": canonical_bytes(model["queries"]),
        "upstream/scenario/synthetic-seeds-v1": canonical_bytes(
            {
                "exclusions": model["exclusions"],
                "expected_paths": model["expected_paths"],
            }
        ),
        "upstream/tokenizer/synthetic-tokenizer-v1": (
            b"unicode-segmentation 1.13.3; Unicode 17.0.0; synthetic fixture\n"
        ),
    }


def build_collection_files() -> dict[str, bytes]:
    model = fixture_model()
    populations = derive_populations(model)
    files: dict[str, bytes] = {
        "corpus-embeddings.f32.jsonl": jsonl_bytes(model["corpus_embeddings"]),
        "evidence-judgments.jsonl": jsonl_bytes(model["evidence"]),
        "exclusions.jsonl": jsonl_bytes(model["exclusions"]),
        "expected-paths.jsonl": jsonl_bytes(model["expected_paths"]),
        "graph-schema.json": canonical_bytes(model["graph_schema"]),
        "qrels.tsv": b"".join(
            f"{query_id} 0 {record_id} {grade}\n".encode()
            for query_id, record_id, grade in model["qrels"]
        ),
        "queries.jsonl": jsonl_bytes(model["queries"]),
        "query-embeddings.f32.jsonl": jsonl_bytes(model["query_embeddings"]),
        "records.jsonl": jsonl_bytes(model["records"]),
    }
    streams = source_streams(model)
    inventory = {
        source_id: sha256(data) for source_id, data in sorted(streams.items())
    }

    def inputs(*source_ids: str) -> list[dict[str, str]]:
        return [
            {"sha256": inventory[source_id], "source_id": source_id}
            for source_id in sorted(source_ids)
        ]

    def collection_inputs(*paths: str) -> list[dict[str, str]]:
        return [
            {"sha256": sha256(files[path]), "source_id": f"collection/{path}"}
            for path in sorted(paths)
        ]

    def outputs(*paths: str) -> list[dict[str, str]]:
        return [
            {"path": path, "sha256": sha256(files[path])} for path in sorted(paths)
        ]

    tool = {"name": "vectorkit-v3-synthetic-fixture", "version": "1.0.0"}
    preprocessing = {
        "inputs": inputs("upstream/corpus/synthetic-records-v1"),
        "outputs": [],
        "parameters": {
            "field_selection": [["content"], ["fields"]],
            "source_record_id_path": ["record_id"],
            "source_record_type_path": ["record_type"],
            "source_to_record_mapping": "identity synthetic canonical records",
            "text_join_separator": "\n",
            "title_path": ["fields", "title"],
            "unicode_handling": "preserve valid UTF-8 without normalization",
            "whitespace_rules": "preserve source text byte-for-byte",
        },
        "policy_id": "synthetic-preprocessing-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    chunking = {
        "inputs": inputs("upstream/corpus/synthetic-records-v1"),
        "outputs": outputs("records.jsonl"),
        "parameters": {
            "boundary_policy": "fixture-authored semantic boundaries",
            "chunker_name": "synthetic-fixed-chunker",
            "chunker_version": "1",
            "maximum_size": 256,
            "overlap": 0,
            "source_offset_policy": "not applicable to authored fixture chunks",
            "stable_key_derivation": "authored stable chunk key",
            "units": "unicode scalar values",
        },
        "policy_id": "synthetic-chunking-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    graph_construction = {
        "inputs": collection_inputs("records.jsonl")
        + inputs("upstream/graph/synthetic-schema-v1"),
        "outputs": outputs("graph-schema.json"),
        "parameters": {
            "duplicate_references": "schema-declared exact policy",
            "inverse_edges": True,
            "judgment_inputs_sha256": None,
            "missing_target": "schema-declared exact policy",
            "node_derivation": "record type and optional chunk-node rules",
            "relationship_derivation": "explicit stable record references only",
            "schema_sha256": sha256(files["graph-schema.json"]),
            "self_edges": False,
            "source_fields": [["covered_topic"], ["owns_ids"], ["related_ids"]],
        },
        "policy_id": "synthetic-graph-construction-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    exclusion_counts = []
    global_before = 10
    for reason in [
        "duplicate_identity",
        "filter_label_conflict",
        "invalid_upstream_record",
        "missing_complete_evidence",
        "no_relevant_documents",
        "not_in_frozen_corpus",
    ]:
        excluded = 1 if reason == "no_relevant_documents" else 0
        exclusion_counts.append(
            {
                "after": global_before - excluded,
                "before": global_before,
                "excluded": excluded,
                "lane": "global",
                "reason": reason,
            }
        )
        global_before -= excluded
    for policy in ["team", "topic"]:
        before = len(populations[f"X_{policy}"])
        for reason in ["derived_seed_ambiguous", "derived_seed_no_match"]:
            excluded = sum(
                1
                for row in model["exclusions"]
                if row["lane"] == policy and row["reason"] == reason
            )
            exclusion_counts.append(
                {
                    "after": before - excluded,
                    "before": before,
                    "excluded": excluded,
                    "lane": policy,
                    "reason": reason,
                }
            )
            before -= excluded
    inventory_preimage = [
        {"sha256": digest, "source_id": source_id}
        for source_id, digest in sorted(inventory.items())
    ]
    source_inventory_sha256 = sha256(compact_bytes(inventory_preimage))
    test_lock_preimage = {
        "collection_rule": "normative A-J included cases A-I and global exclusion J",
        "development_population_sha256": population_hash(populations["Q"]),
        "exclusion_counts": exclusion_counts,
        "release_id": "synthetic-v3-conformance-1",
        "source_inventory_sha256": source_inventory_sha256,
        "split_id": "development",
        "test_population_sha256": sha256(b""),
    }
    split = {
        "inputs": collection_inputs("graph-schema.json", "records.jsonl")
        + inputs(
            "upstream/judgment/synthetic-judgments-v1",
            "upstream/license/synthetic-license-v1",
            "upstream/query/synthetic-queries-v1",
            "upstream/scenario/synthetic-seeds-v1",
        ),
        "outputs": outputs(
            "evidence-judgments.jsonl",
            "exclusions.jsonl",
            "expected-paths.jsonl",
            "qrels.tsv",
            "queries.jsonl",
        ),
        "parameters": {
            "archive_sha256": inventory["upstream/corpus/synthetic-records-v1"],
            "archive_url": "synthetic://vectorkit/v3-conformance-1",
            "collection_rule": test_lock_preimage["collection_rule"],
            "development_population_sha256": population_hash(populations["Q"]),
            "exclusion_counts": exclusion_counts,
            "license_id": "CC0-1.0",
            "license_notice_source_id": "upstream/license/synthetic-license-v1",
            "release_id": test_lock_preimage["release_id"],
            "source_inventory_sha256": source_inventory_sha256,
            "split_id": test_lock_preimage["split_id"],
            "test_lock_sha256": sha256(compact_bytes(test_lock_preimage)),
            "test_population_sha256": sha256(b""),
        },
        "policy_id": "synthetic-split-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    topic_aliases = [
        alias_row("Alpha", "topic", "alpha"),
        alias_row("Beta", "topic", "beta"),
        alias_row("Gamma", "topic", "gamma"),
        alias_row("Shared", "topic", "shared-east"),
        alias_row("Shared", "topic", "shared-west"),
    ]
    team_aliases = [alias_row("Mobile", "team", "mobile")]

    def derived_policy(policy: str, aliases: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "alias_table_sha256": sha256(compact_bytes(aliases)),
            "aliases": aliases,
            "declared_population_sha256": population_hash(populations[f"X_{policy}"]),
            "failure_population_sha256": population_hash(populations[f"F_{policy}"]),
            "policy_id": policy,
            "policy_version": "1",
            "source_fields": [["title"]],
            "successful_population_sha256": population_hash(populations[f"S_{policy}"]),
        }

    seed_policy = {
        "inputs": collection_inputs(
            "exclusions.jsonl", "graph-schema.json", "queries.jsonl", "records.jsonl"
        )
        + inputs("upstream/scenario/synthetic-seeds-v1"),
        "outputs": [],
        "parameters": {
            "derived_policies": [
                derived_policy("team", team_aliases),
                derived_policy("topic", topic_aliases),
            ],
            "explicit_policy": {
                "policy_id": "explicit",
                "policy_version": "1",
                "provenance": [
                    {
                        "query_id": query_id,
                        "source_id": "upstream/scenario/synthetic-seeds-v1",
                        "transformation_id": "authored-structured-seed-v1",
                    }
                    for query_id in ["qb", "qc", "qh"]
                ],
            },
            "normalization": {
                "case_folding": "unicode_default_full_case_folding",
                "normalization_form": "NFC",
                "normalization_version": "unicode-15.1-nfc-full-fold-whitespace-v1",
                "punctuation": "preserve",
                "unicode_tables_sha256": inventory[
                    "upstream/tokenizer/synthetic-tokenizer-v1"
                ],
                "unicode_version": "15.1",
                "whitespace": "unicode_white_space_to_ascii_collapse_trim",
            },
        },
        "policy_id": "synthetic-seed-policy-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    quantization = quantization_policy()
    embedding = {
        "inputs": collection_inputs("queries.jsonl", "records.jsonl")
        + inputs(
            "upstream/model/synthetic-3d-f32-v1",
            "upstream/tokenizer/synthetic-tokenizer-v1",
        ),
        "outputs": outputs(
            "corpus-embeddings.f32.jsonl", "query-embeddings.f32.jsonl"
        ),
        "parameters": {
            "dimension": 3,
            "document_prefix": "document: ",
            "input_construction": "prefix plus exact canonical chunk text",
            "model_checksum": inventory["upstream/model/synthetic-3d-f32-v1"],
            "model_id": "synthetic-orthogonal-3d",
            "model_output_normalization": "none",
            "model_revision": "1",
            "pooling": "fixture-authored",
            "quantization": quantization,
            "query_prefix": "query: ",
            "runtime": "checked-in-source-f32",
            "sequence_length": 64,
            "tokenizer_id": "synthetic-whitespace-v1",
            "tokenizer_revision": "1",
            "truncation_policy": "reject over sequence length",
        },
        "policy_id": "synthetic-embedding-v1",
        "policy_version": "1",
        "schema_version": 1,
        "tool": tool,
    }
    manifests = {
        "manifests/chunking.json": chunking,
        "manifests/embedding.json": embedding,
        "manifests/graph-construction.json": graph_construction,
        "manifests/preprocessing.json": preprocessing,
        "manifests/seed-policy.json": seed_policy,
        "manifests/split.json": split,
    }
    files.update({path: canonical_bytes(value) for path, value in manifests.items()})
    paths = {
        "chunking_manifest": "manifests/chunking.json",
        "corpus_embeddings_f32": "corpus-embeddings.f32.jsonl",
        "embedding_manifest": "manifests/embedding.json",
        "evidence_judgments": "evidence-judgments.jsonl",
        "exclusions": "exclusions.jsonl",
        "expected_paths": "expected-paths.jsonl",
        "graph_construction_manifest": "manifests/graph-construction.json",
        "graph_schema": "graph-schema.json",
        "preprocessing_manifest": "manifests/preprocessing.json",
        "qrels": "qrels.tsv",
        "queries": "queries.jsonl",
        "query_embeddings_f32": "query-embeddings.f32.jsonl",
        "records": "records.jsonl",
        "seed_policy_manifest": "manifests/seed-policy.json",
        "split_manifest": "manifests/split.json",
    }
    collection = {
        "collection_id": "vectorkit-v3-conformance",
        "collection_version": "1.0.0",
        "corpus_id": "vectorkit-v3-synthetic-corpus",
        "counts": {
            "chunks": len(model["corpus_embeddings"]),
            "evidence_rows": len(model["evidence"]),
            "exclusion_rows": len(model["exclusions"]),
            "expected_path_rows": len(model["expected_paths"]),
            "qrel_rows": len(model["qrels"]),
            "queries": len(model["queries"]),
            "records": len(model["records"]),
        },
        "evaluation_depth": 10,
        "files": [
            {"bytes": len(data), "path": path, "sha256": sha256(data)}
            for path, data in sorted(files.items())
        ],
        "paths": paths,
        "relevance_threshold": 1,
        "schema_version": 3,
        "split": "development",
        "top_k": 10,
    }
    return {"collection.json": canonical_bytes(collection), **files}


def quantization_policy() -> dict[str, Any]:
    return {
        "arithmetic": "ieee754_f32_each_operation",
        "clamp_max": 127,
        "clamp_min": -128,
        "dot_accumulator": "signed_i32_exact",
        "encoding_expression": "value_times_reciprocal_scale",
        "kind": "symmetric_per_vector_i8",
        "rounding": "half_away_from_zero",
        "scale_divisor": 127,
        "score_expression": "f32_i32_dot_times_query_scale_times_chunk_scale",
        "zero_vector_scale": 0,
    }


def normalization_policy() -> dict[str, Any]:
    return {
        "arithmetic": "ieee754_f32_each_operation",
        "input": "source_f32",
        "inverse_norm": "sqrt_then_reciprocal",
        "kind": "unit_l2_before_encoding",
        "reduction": "index_order_left_to_right",
        "sqrt": "correctly_rounded_f32",
        "zero_vector": "unchanged",
    }


def bm25_policy() -> dict[str, Any]:
    return {
        "b": 0.75,
        "k1": 1.2,
        "lowercase": "rust_str_to_lowercase",
        "stop_words": [],
        "tokenizer_id": "unicode-segmentation-unicode_words",
        "tokenizer_library_sha256": "c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8",
        "tokenizer_version": "1.13.3",
        "unicode_lowercase_tables_sha256": LOWERCASE_TABLE_SHA256,
        "unicode_version": "17.0.0",
    }


def implementation_revision() -> dict[str, Any]:
    return {
        "binary_sha256": sha256(FOUNDATION_BINARY_BYTES),
        "git_commit": BASE_GIT_COMMIT,
        "source_sha256": None,
    }


def derive_runs(
    collection_files: dict[str, bytes],
    revision: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    model = fixture_model()
    populations = derive_populations(model)
    collection = json.loads(collection_files["collection.json"])
    graph_hash = sha256(collection_files["graph-schema.json"])
    seed_hash = sha256(collection_files["manifests/seed-policy.json"])
    specs: list[tuple[str, str, str, str, str, set[str], set[str]]] = []
    for letter, mode, encoding in [
        ("a", "semantic", "f32"),
        ("b", "semantic", "i8"),
        ("c", "weighted", "i8"),
    ]:
        specs.append((letter, "whole", mode, encoding, "none", populations["R"], populations["R"]))
    for lane in ["explicit", "team", "topic"]:
        declared = populations["X_exp" if lane == "explicit" else f"X_{lane}"]
        successful = populations["S_exp" if lane == "explicit" else f"S_{lane}"]
        specs.append(("d", "selection", "none", "none", lane, declared, successful))
        retrieval_declared = declared & populations["R"]
        retrieval_successful = successful & populations["R"]
        for letter, mode, encoding in [
            ("e", "semantic", "f32"),
            ("f", "semantic", "i8"),
            ("g", "weighted", "i8"),
        ]:
            specs.append(
                (
                    letter,
                    "graph",
                    mode,
                    encoding,
                    lane,
                    retrieval_declared,
                    retrieval_successful,
                )
            )
    query_by_id = {row["query_id"]: row for row in model["queries"]}
    quant_hash = sha256(compact_bytes(quantization_policy()))
    runs = []
    for letter, scope, mode, encoding, lane, declared, execution in specs:
        graph = letter in "defg"
        retrieval_run = letter != "d"
        weighted = letter in "cg"
        quantized = letter in "bcfg"
        traversal_hash = None
        if graph:
            traversal_hash = sha256(
                compact_bytes(
                    [
                        {"query_id": query_id, "traversal": query_by_id[query_id]["traversal"]}
                        for query_id in sorted(declared)
                    ]
                )
            )
        configuration = {
            "bm25_policy": bm25_policy() if weighted else None,
            "candidate_limits": {"keyword": 8, "vector": 8}
            if weighted
            else {"keyword": None, "vector": None},
            "collection_id": collection["collection_id"],
            "collection_version": collection["collection_version"],
            "corpus_id": collection["corpus_id"],
            "evaluation_depth": collection["evaluation_depth"],
            "fusion_alpha": 0.6 if weighted else None,
            "graph_schema_sha256": graph_hash if graph else None,
            "implementation_revision": revision or implementation_revision(),
            "metadata_filter_policy_id": "v3-query-filter-ast-v1",
            "metric": "cosine" if retrieval_run else None,
            "normalization": "unit_l2" if retrieval_run else None,
            "normalization_policy": normalization_policy() if retrieval_run else None,
            "quantization_policy_sha256": quant_hash if quantized else None,
            "retrieval_mode": mode,
            "run_letter": letter,
            "schema_version": 3,
            "scope": scope,
            "seed_lane": lane,
            "seed_policy_sha256": seed_hash if graph else None,
            "top_k": collection["top_k"],
            "traversal_policy_sha256": traversal_hash,
            "vector_encoding": encoding,
        }
        preimage = compact_bytes(configuration)
        run_hash = sha256(preimage)
        seed = "na" if lane == "none" else lane
        run_id = f"v3-{letter}-{scope}-{mode}-{encoding}-{seed}-cfg-{run_hash[:12]}"
        logical = dict(configuration)
        del logical["implementation_revision"]
        runs.append(
            {
                "configuration": configuration,
                "configuration_preimage": preimage.decode(),
                "declared_population": sorted(declared),
                "declared_population_sha256": population_hash(declared),
                "execution_population": sorted(execution),
                "execution_population_sha256": population_hash(execution),
                "logical_run_sha256": sha256(compact_bytes(logical)),
                "run_id": run_id,
            }
        )
    return sorted(runs, key=lambda row: row["run_id"])


def derive_generation_fingerprints(
    collection_files: dict[str, bytes], runs: list[dict[str, Any]]
) -> dict[str, Any]:
    def file_array_hash(paths: list[str]) -> str:
        return sha256(
            compact_bytes(
                [
                    {"path": path, "sha256": sha256(collection_files[path])}
                    for path in sorted(paths)
                ]
            )
        )

    collection = json.loads(collection_files["collection.json"])
    corpus_state = file_array_hash(
        ["manifests/chunking.json", "manifests/preprocessing.json", "records.jsonl"]
    )
    graph_state = file_array_hash(
        ["graph-schema.json", "manifests/graph-construction.json"]
    )
    normalization_hash = sha256(compact_bytes(normalization_policy()))
    quantization_hash = sha256(compact_bytes(quantization_policy()))
    bm25_hash = sha256(compact_bytes(bm25_policy()))
    bindings = []
    unique: dict[str, dict[str, Any]] = {}
    for run in runs:
        configuration = run["configuration"]
        letter = configuration["run_letter"]
        if letter not in "defg":
            continue
        retrieval_state = None
        if letter != "d":
            retrieval_state = {
                "bm25_policy_sha256": bm25_hash if letter == "g" else None,
                "files": [
                    {
                        "path": "corpus-embeddings.f32.jsonl",
                        "sha256": sha256(collection_files["corpus-embeddings.f32.jsonl"]),
                    },
                    {
                        "path": "manifests/embedding.json",
                        "sha256": sha256(collection_files["manifests/embedding.json"]),
                    },
                ],
                "metric": "cosine",
                "normalization": "unit_l2",
                "normalization_policy_sha256": normalization_hash,
                "quantization_policy_sha256": quantization_hash
                if letter in "fg"
                else None,
                "vector_encoding": configuration["vector_encoding"],
            }
        retrieval_hash = sha256(compact_bytes(retrieval_state)) if retrieval_state else None
        preimage = {
            "corpus_id": collection["corpus_id"],
            "corpus_state_sha256": corpus_state,
            "graph_state_sha256": graph_state,
            "retrieval_state_sha256": retrieval_hash,
            "schema_version": 1,
        }
        fingerprint = sha256(compact_bytes(preimage))
        unique[fingerprint] = preimage
        bindings.append({"fingerprint": fingerprint, "run_id": run["run_id"]})
    return {
        "bindings": sorted(bindings, key=lambda row: row["run_id"]),
        "foundation_only": True,
        "preimages": [
            {"fingerprint": fingerprint, "preimage": preimage}
            for fingerprint, preimage in sorted(unique.items())
        ],
        "schema_version": 1,
    }


def validate_generation_fingerprint_modes(
    collection_files: dict[str, bytes], runs: list[dict[str, Any]]
) -> None:
    """Bind each E-G fingerprint to its exact section 4.4 retrieval preimage."""

    def file_array_hash(paths: list[str]) -> str:
        return sha256(
            compact_bytes(
                [
                    {"path": path, "sha256": sha256(collection_files[path])}
                    for path in sorted(paths)
                ]
            )
        )

    collection = json.loads(collection_files["collection.json"])
    common = {
        "corpus_id": collection["corpus_id"],
        "corpus_state_sha256": file_array_hash(
            ["manifests/chunking.json", "manifests/preprocessing.json", "records.jsonl"]
        ),
        "graph_state_sha256": file_array_hash(
            ["graph-schema.json", "manifests/graph-construction.json"]
        ),
        "schema_version": 1,
    }
    normalization_hash = sha256(compact_bytes(normalization_policy()))
    quantization_hash = sha256(compact_bytes(quantization_policy()))
    bm25_hash = sha256(compact_bytes(bm25_policy()))
    generated = derive_generation_fingerprints(collection_files, runs)

    cases = [
        (
            "e",
            "f32",
            None,
            None,
            "485f564956610b65f16b7163b69085dad7c1a495aaf99aa44ac98d8aac9a4cef",
        ),
        (
            "f",
            "i8",
            None,
            quantization_hash,
            "9142876c6ff687ae58d8c86ea25b553a9cde7744f2f91fa1bb2c34cf50a8eb1b",
        ),
        (
            "g",
            "i8",
            bm25_hash,
            quantization_hash,
            "7b5d71ac2e583b82bef661aa30ed57ea85e3e10b2fbc468fbbdb6689ef35cdb0",
        ),
    ]
    for letter, encoding, bm25_hash_or_none, quantization_hash_or_none, expected in cases:
        retrieval_preimage = {
            "bm25_policy_sha256": bm25_hash_or_none,
            "files": [
                {
                    "path": "corpus-embeddings.f32.jsonl",
                    "sha256": sha256(collection_files["corpus-embeddings.f32.jsonl"]),
                },
                {
                    "path": "manifests/embedding.json",
                    "sha256": sha256(collection_files["manifests/embedding.json"]),
                },
            ],
            "metric": "cosine",
            "normalization": "unit_l2",
            "normalization_policy_sha256": normalization_hash,
            "quantization_policy_sha256": quantization_hash_or_none,
            "vector_encoding": encoding,
        }
        require(retrieval_preimage["vector_encoding"] == encoding, f"{letter} encoding mismatch")
        require(
            (retrieval_preimage["bm25_policy_sha256"] is None) == (letter != "g"),
            f"{letter} BM25 policy presence mismatch",
        )
        require(
            (retrieval_preimage["quantization_policy_sha256"] is None) == (letter == "e"),
            f"{letter} quantization policy presence mismatch",
        )
        preimage = {
            **common,
            "retrieval_state_sha256": sha256(compact_bytes(retrieval_preimage)),
        }
        fingerprint = sha256(compact_bytes(preimage))
        require(fingerprint == expected, f"{letter} generation fingerprint mismatch")
        expected_run_ids = {
            run["run_id"] for run in runs if run["configuration"]["run_letter"] == letter
        }
        bound_run_ids = {
            binding["run_id"]
            for binding in generated["bindings"]
            if binding["fingerprint"] == expected
        }
        require(bound_run_ids == expected_run_ids, f"{letter} fingerprint binding mismatch")
        require(
            {"fingerprint": expected, "preimage": preimage} in generated["preimages"],
            f"{letter} outer generation preimage mismatch",
        )


def validate_normative_fixture() -> None:
    contract = ROOT / "docs/product/graph-retrieval-evaluation-contract-v3.md"
    text = contract.read_text(encoding="utf-8")
    marker = "```jsonl\n{\"case_id\":\"A\""
    start = text.index(marker) + len("```jsonl\n")
    end = text.index("```", start)
    fixture = text[start:end].encode()
    require(len(fixture) == 2135, f"normative A-J byte length is {len(fixture)}, expected 2135")
    require(
        sha256(fixture) == NORMATIVE_AJ_SHA256,
        f"normative A-J SHA-256 is {sha256(fixture)}, expected {NORMATIVE_AJ_SHA256}",
    )


def validate_collection(root: Path) -> tuple[dict[str, bytes], list[dict[str, Any]]]:
    expected = build_collection_files()
    actual_paths = sorted(
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file() and path.relative_to(root).as_posix() != "README.md"
    )
    require(actual_paths == sorted(expected), "collection file set does not match canonical fixture")
    for path, expected_bytes in sorted(expected.items()):
        actual = (root / path).read_bytes()
        compare(path, actual, expected_bytes)
    populations = reported_populations(fixture_model())
    published = {
        "Q": "91be2f127eff88b3d41229df2904cb3b7203992673711e3ee960ade05c35496d",
        "R": "c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3",
        "X_exp": "533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5",
        "S_exp": "533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5",
        "X_topic": "a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8",
        "F_topic": "f1a82a3707574638a0dff6e16db2616c73c0692bcee0e55a21b565097d3267fb",
        "S_topic": "be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59",
        "X_team": "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
        "F_team": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "S_team": "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
        "X_exp_intersect_R": "2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f",
        "X_topic_intersect_R": "d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082",
        "S_topic_intersect_R": "b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e",
        "X_team_intersect_R": "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
        "S_team_intersect_R": "1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d",
    }
    for name, digest in published.items():
        require(population_hash(populations[name]) == digest, f"population {name} hash mismatch")
    runs = derive_runs(expected)
    require(len(runs) == 15, f"canonical run count is {len(runs)}, expected 15")
    require(len({row["run_id"] for row in runs}) == 15, "run IDs are not unique")
    require(
        len({row["logical_run_sha256"] for row in runs}) == 15,
        "logical-run hashes are not unique",
    )
    validate_generation_fingerprint_modes(expected, runs)
    return expected, runs


def validate_artifacts(
    root: Path, collection_files: dict[str, bytes], runs: list[dict[str, Any]]
) -> None:
    expected_collection_root = root / "validated-collection"
    for path, data in collection_files.items():
        compare(f"validated-collection/{path}", (expected_collection_root / path).read_bytes(), data)
    expected_populations = []
    for name, ids in derive_populations(fixture_model()).items():
        expected_populations.append(
            {"ids": sorted(ids), "name": name, "sha256": population_hash(ids)}
        )
    expected_population_object = {
        "collection_id": "vectorkit-v3-conformance",
        "collection_version": "1.0.0",
        "foundation_only": True,
        "populations": sorted(expected_populations, key=lambda row: row["name"]),
        "schema_version": 3,
    }
    compare(
        "populations.json",
        (root / "populations.json").read_bytes(),
        canonical_bytes(expected_population_object),
    )
    compare("run-configurations.jsonl", (root / "run-configurations.jsonl").read_bytes(), jsonl_bytes(runs))
    expected_fingerprints = derive_generation_fingerprints(collection_files, runs)
    compare(
        "generation-fingerprints.json",
        (root / "generation-fingerprints.json").read_bytes(),
        canonical_bytes(expected_fingerprints),
    )
    manifest = json.loads((root / "foundation-manifest.json").read_bytes())
    indexed = []
    for path in sorted(
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file() and path.name != "foundation-manifest.json"
    ):
        data = (root / path).read_bytes()
        indexed.append({"bytes": len(data), "path": path, "sha256": sha256(data)})
    require(manifest["files"] == indexed, "foundation manifest file index mismatch")
    require(manifest["run_count"] == 15, "foundation manifest run count mismatch")


def compare(label: str, actual: bytes, expected: bytes) -> None:
    if actual == expected:
        return
    offset = next(
        (index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]),
        min(len(actual), len(expected)),
    )
    raise ValueError(
        f"{label}: canonical bytes differ at offset {offset} "
        f"(actual {len(actual)} bytes, expected {len(expected)} bytes)"
    )


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def write_fixture(root: Path) -> None:
    if root.exists():
        shutil.rmtree(root)
    for path, data in build_collection_files().items():
        destination = root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--collection", type=Path, default=DEFAULT_COLLECTION)
    parser.add_argument("--foundation-artifacts", type=Path)
    parser.add_argument("--write-fixture", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.write_fixture:
        write_fixture(args.collection)
    validate_normative_fixture()
    collection_files, runs = validate_collection(args.collection)
    if args.foundation_artifacts:
        validate_artifacts(args.foundation_artifacts, collection_files, runs)
    populations = reported_populations(fixture_model())
    result = {
        "fixture_bytes": 2135,
        "fixture_sha256": NORMATIVE_AJ_SHA256,
        "population_hashes": {
            name: population_hash(ids) for name, ids in sorted(populations.items())
        },
        "run_count": len(runs),
        "run_ids": [row["run_id"] for row in runs],
        "status": "valid",
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"V3 conformance validation failed: {error}", file=sys.stderr)
        sys.exit(1)
