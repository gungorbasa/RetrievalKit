#!/usr/bin/env python3
"""Isolated worker for one Phase 5 system/workload pair."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import sqlite3
import time
import traceback
from pathlib import Path
from typing import Any, Callable

import numpy as np

from phase5_common import (
    CHUNKS_PER_RECORD,
    TOP_K,
    WorkloadData,
    canonical_file,
    chunk_text,
    directory_sizes,
    distribution,
    generate_workload,
    normalized_peak_rss_bytes,
    oracle_results,
    result_identity,
    stable_chunk_id,
    stable_record_id,
)

QueryFunction = Callable[[int], tuple[list[str], list[float]]]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def measured_operation(
    data: WorkloadData,
    operation_id: str,
    query: QueryFunction,
    *,
    warmups: int,
    sample_count: int,
    replay: QueryFunction | None = None,
) -> dict[str, Any]:
    expected: dict[str, list[str]] = {}
    results = []
    for index, spec in enumerate(data.query_specs):
        ids, scores = query(index)
        expected[spec.query_id] = ids
        results.append(
            {
                "operation_id": operation_id,
                "query_id": spec.query_id,
                "result_identity_sha256": result_identity(ids),
                "result_ids": ids,
                "scores": scores,
            }
        )
    for index in range(warmups):
        query(index % len(data.query_specs))
    samples = []
    durations = []
    for sample_index in range(sample_count):
        query_index = sample_index % len(data.query_specs)
        query_id = data.query_specs[query_index].query_id
        started = time.perf_counter_ns()
        ids, _scores = query(query_index)
        duration = time.perf_counter_ns() - started
        if ids != expected[query_id]:
            raise RuntimeError(
                f"{operation_id} result changed for {query_id} at sample {sample_index}"
            )
        durations.append(duration)
        samples.append(
            {
                "duration_ns": duration,
                "operation_id": operation_id,
                "query_id": query_id,
                "result_identity_sha256": result_identity(ids),
                "sample_index": sample_index,
                "stage": "retrieval",
            }
        )
    replay_results = []
    replay_query = replay or query
    for index, spec in enumerate(data.query_specs):
        ids, _scores = replay_query(index)
        replay_results.append(
            {
                "operation_id": operation_id,
                "query_id": spec.query_id,
                "result_identity_sha256": result_identity(ids),
                "result_ids": ids,
            }
        )
    return {
        "distribution": distribution(durations),
        "operation_id": operation_id,
        "replay_results": replay_results,
        "results": results,
        "samples": samples,
        "timed": True,
    }


def base_result(system_id: str, data: WorkloadData) -> dict[str, Any]:
    return {
        "artifact_type": "phase5_adapter_result",
        "build_ns": None,
        "failure": None,
        "input_manifest": data.input_manifest,
        "load_ns": None,
        "operations": [],
        "peak_rss_bytes": None,
        "persistence": {"components": [], "total_bytes": 0},
        "save_ns": None,
        "schema_version": 1,
        "status": "success",
        "system_id": system_id,
        "system_version": None,
        "workload_id": data.spec["workload_id"],
    }


def run_numpy_oracle(data: WorkloadData, _request: dict[str, Any]) -> dict[str, Any]:
    result = base_result("numpy_f32_oracle", data)
    result["system_version"] = np.__version__
    for filtered in [False, True]:
        rows = oracle_results(data, filtered=filtered)
        result["operations"].append(
            {
                "distribution": None,
                "operation_id": rows[0]["operation_id"],
                "replay_results": [],
                "results": [
                    {
                        **row,
                        "result_identity_sha256": result_identity(row["result_ids"]),
                    }
                    for row in rows
                ],
                "samples": [],
                "timed": False,
            }
        )
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


def run_vectorkit_exact(data: WorkloadData, request: dict[str, Any]) -> dict[str, Any]:
    from vectorkit import Index

    result = base_result("vectorkit_f32_exact", data)
    result["system_version"] = request["source_revision"]
    persistence_root = Path(request["scratch_root"]) / "vectorkit-exact-index"
    started = time.perf_counter_ns()
    index = Index(dimension=data.dimension, metric="cosine", encoding="f32")
    batch_size = 256
    for start in range(0, data.total_chunks, batch_size):
        stop = min(start + batch_size, data.total_chunks)
        index.add(
            [
                {
                    "id": stable_chunk_id(ordinal),
                    "chunks": [
                        {
                            "embedding": data.vectors[ordinal].tolist(),
                            "metadata": {
                                "external_id": stable_chunk_id(ordinal),
                                "tenant": f"tenant-{ordinal % 10}",
                            },
                            "text": chunk_text(ordinal),
                        }
                    ],
                }
                for ordinal in range(start, stop)
            ]
        )
    for ordinal in range(data.active_chunks, data.total_chunks):
        index.delete_document(stable_chunk_id(ordinal))
    result["build_ns"] = time.perf_counter_ns() - started

    query_vectors = [value.tolist() for value in data.queries]

    def search(query_index: int, *, filtered: bool) -> tuple[list[str], list[float]]:
        spec = data.query_specs[query_index]
        hits = index.search(
            query_vectors[query_index],
            limit=TOP_K,
            where={"tenant": spec.tenant} if filtered else None,
        )
        return [str(hit["document_id"]) for hit in hits], [
            float(hit["score"]) for hit in hits
        ]

    save_started = time.perf_counter_ns()
    index.save(persistence_root)
    result["save_ns"] = time.perf_counter_ns() - save_started
    total_bytes, components = directory_sizes(persistence_root)
    result["persistence"] = {"components": components, "total_bytes": total_bytes}

    load_started = time.perf_counter_ns()
    loaded = Index.load(persistence_root)
    result["load_ns"] = time.perf_counter_ns() - load_started

    def replay_search(
        query_index: int, *, filtered: bool
    ) -> tuple[list[str], list[float]]:
        spec = data.query_specs[query_index]
        hits = loaded.search(
            query_vectors[query_index],
            limit=TOP_K,
            where={"tenant": spec.tenant} if filtered else None,
        )
        return [str(hit["document_id"]) for hit in hits], [
            float(hit["score"]) for hit in hits
        ]

    measurement = request["measurement"]
    for filtered in [False, True]:
        operation = "exact_filtered" if filtered else "exact_unfiltered"
        result["operations"].append(
            measured_operation(
                data,
                operation,
                lambda value, selected=filtered: search(value, filtered=selected),
                warmups=int(measurement["warmups"]),
                sample_count=int(measurement["samples"]),
                replay=lambda value, selected=filtered: replay_search(
                    value, filtered=selected
                ),
            )
        )
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


def sqlite_connection(path: Path) -> tuple[sqlite3.Connection, str]:
    import sqlite_vec

    connection = sqlite3.connect(path)
    connection.enable_load_extension(True)
    sqlite_vec.load(connection)
    connection.enable_load_extension(False)
    version = str(connection.execute("select vec_version()").fetchone()[0])
    return connection, version


def run_sqlite_vec_exact(data: WorkloadData, request: dict[str, Any]) -> dict[str, Any]:
    result = base_result("sqlite_vec_exact", data)
    database_path = Path(request["scratch_root"]) / "sqlite-vec-exact.sqlite3"
    started = time.perf_counter_ns()
    connection, version = sqlite_connection(database_path)
    result["system_version"] = version
    connection.execute("pragma journal_mode=wal")
    connection.execute("pragma synchronous=full")
    connection.execute(
        "create virtual table vec_chunks using vec0("
        f"embedding float[{data.dimension}] distance_metric=cosine, tenant text)"
    )
    connection.executemany(
        "insert into vec_chunks(rowid, embedding, tenant) values (?, ?, ?)",
        (
            (ordinal + 1, data.vectors[ordinal].tobytes(), f"tenant-{ordinal % 10}")
            for ordinal in range(data.total_chunks)
        ),
    )
    connection.executemany(
        "delete from vec_chunks where rowid = ?",
        ((ordinal + 1,) for ordinal in range(data.active_chunks, data.total_chunks)),
    )
    connection.commit()
    result["build_ns"] = time.perf_counter_ns() - started

    def query_on(
        database: sqlite3.Connection, query_index: int, *, filtered: bool
    ) -> tuple[list[str], list[float]]:
        spec = data.query_specs[query_index]
        if filtered:
            rows = database.execute(
                "select rowid, distance from vec_chunks "
                "where embedding match ? and k = ? and tenant = ? "
                "order by distance, rowid",
                (data.queries[query_index].tobytes(), TOP_K, spec.tenant),
            ).fetchall()
        else:
            rows = database.execute(
                "select rowid, distance from vec_chunks "
                "where embedding match ? and k = ? order by distance, rowid",
                (data.queries[query_index].tobytes(), TOP_K),
            ).fetchall()
        return [stable_chunk_id(int(row[0]) - 1) for row in rows], [
            1.0 - float(row[1]) for row in rows
        ]

    save_started = time.perf_counter_ns()
    connection.execute("pragma wal_checkpoint(truncate)")
    connection.commit()
    result["save_ns"] = time.perf_counter_ns() - save_started
    total_bytes, components = directory_sizes(database_path.parent)
    components = [
        value for value in components if value["path"].startswith(database_path.name)
    ]
    total_bytes = sum(int(value["bytes"]) for value in components)
    result["persistence"] = {"components": components, "total_bytes": total_bytes}
    connection.close()
    load_started = time.perf_counter_ns()
    loaded, loaded_version = sqlite_connection(database_path)
    result["load_ns"] = time.perf_counter_ns() - load_started
    if loaded_version != version:
        raise RuntimeError("sqlite-vec runtime version changed after reload")
    measurement = request["measurement"]
    for filtered in [False, True]:
        operation = "exact_filtered" if filtered else "exact_unfiltered"
        result["operations"].append(
            measured_operation(
                data,
                operation,
                lambda value, selected=filtered: query_on(
                    loaded, value, filtered=selected
                ),
                warmups=int(measurement["warmups"]),
                sample_count=int(measurement["samples"]),
            )
        )
    loaded.close()
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


def run_usearch(data: WorkloadData, request: dict[str, Any]) -> dict[str, Any]:
    from usearch.index import Index

    result = base_result("usearch_hnsw", data)
    result["system_version"] = importlib.metadata.version("usearch")
    ann = request["contract"]["ann"]
    index_path = Path(request["scratch_root"]) / "ann.usearch"
    started = time.perf_counter_ns()
    index = Index(
        ndim=data.dimension,
        metric="cos",
        dtype=str(ann["dtype"]),
        connectivity=int(ann["connectivity"]),
        expansion_add=int(ann["expansion_add"]),
        expansion_search=int(ann["expansion_search"]),
    )
    keys = np.arange(data.total_chunks, dtype=np.uint64)
    index.add(keys, data.vectors, threads=int(ann["threads"]))
    index.remove(keys[data.active_chunks :])
    result["build_ns"] = time.perf_counter_ns() - started

    def query_on(engine: Any, query_index: int) -> tuple[list[str], list[float]]:
        matches = engine.search(
            data.queries[query_index], TOP_K, threads=int(ann["threads"])
        )
        ids = [stable_chunk_id(int(value)) for value in matches.keys.tolist()]
        scores = [1.0 - float(value) for value in matches.distances.tolist()]
        return ids, scores

    save_started = time.perf_counter_ns()
    index.save(index_path)
    result["save_ns"] = time.perf_counter_ns() - save_started
    result["persistence"] = {
        "components": [{"bytes": index_path.stat().st_size, "path": index_path.name}],
        "total_bytes": index_path.stat().st_size,
    }
    load_started = time.perf_counter_ns()
    loaded = Index.restore(index_path, view=False)
    result["load_ns"] = time.perf_counter_ns() - load_started
    measurement = request["measurement"]
    result["operations"].append(
        measured_operation(
            data,
            "ann_unfiltered",
            lambda value: query_on(index, value),
            warmups=int(measurement["warmups"]),
            sample_count=int(measurement["samples"]),
            replay=lambda value: query_on(loaded, value),
        )
    )
    result["operations"].append(
        {
            "distribution": None,
            "operation_id": "ann_filtered",
            "replay_results": [],
            "results": [],
            "samples": [],
            "timed": False,
            "unsupported_reason": (
                "USearch 2.26.0 Python binding exposes no predicate filtering; "
                "post-filtering is not substituted"
            ),
        }
    )
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


def create_application_schema(connection: sqlite3.Connection, dimension: int) -> None:
    connection.executescript(
        f"""
        pragma journal_mode=wal;
        pragma synchronous=full;
        create table records(
          id text primary key,
          tenant text not null,
          category integer not null
        );
        create table chunks(
          ordinal integer primary key,
          external_id text not null unique,
          record_id text not null references records(id) on delete cascade,
          tenant text not null,
          text text not null,
          embedding blob not null check(vec_length(embedding) = {dimension})
        );
        create table edges(
          source_id text not null,
          target_id text not null,
          relationship text not null,
          primary key(source_id, target_id, relationship)
        );
        create index edges_source on edges(source_id, relationship, target_id);
        create index chunks_record_tenant on chunks(record_id, tenant, external_id);
        create virtual table chunks_fts using fts5(external_id unindexed, text);
        """
    )


def populate_application(connection: sqlite3.Connection, data: WorkloadData) -> None:
    total_records = data.total_chunks // CHUNKS_PER_RECORD
    active_records = data.active_records
    with connection:
        connection.executemany(
            "insert into records(id, tenant, category) values (?, ?, ?)",
            (
                (stable_record_id(value), f"tenant-{value % 10}", value % 7)
                for value in range(total_records)
            ),
        )
        connection.executemany(
            "insert into chunks(ordinal, external_id, record_id, tenant, text, embedding) "
            "values (?, ?, ?, ?, ?, ?)",
            (
                (
                    ordinal,
                    stable_chunk_id(ordinal),
                    stable_record_id(ordinal // CHUNKS_PER_RECORD),
                    f"tenant-{ordinal % 10}",
                    chunk_text(ordinal),
                    data.vectors[ordinal].tobytes(),
                )
                for ordinal in range(data.total_chunks)
            ),
        )
        connection.executemany(
            "insert into chunks_fts(external_id, text) values (?, ?)",
            (
                (stable_chunk_id(ordinal), chunk_text(ordinal))
                for ordinal in range(data.total_chunks)
            ),
        )
        connection.executemany(
            "insert into edges(source_id, target_id, relationship) values (?, ?, ?)",
            (
                edge
                for value in range(active_records)
                for edge in [
                    (
                        stable_record_id(value),
                        stable_record_id((value + 1) % active_records),
                        "next",
                    ),
                    (
                        stable_record_id(value),
                        stable_record_id((value + 7) % active_records),
                        "linked",
                    ),
                ]
            ),
        )
        for record in range(active_records, total_records):
            record_id = stable_record_id(record)
            chunk_ids = [
                stable_chunk_id(record * CHUNKS_PER_RECORD + offset)
                for offset in range(CHUNKS_PER_RECORD)
            ]
            connection.executemany(
                "delete from chunks_fts where external_id = ?",
                ((value,) for value in chunk_ids),
            )
            connection.execute("delete from chunks where record_id = ?", (record_id,))
            connection.execute(
                "delete from edges where source_id = ? or target_id = ?",
                (record_id, record_id),
            )
            connection.execute("delete from records where id = ?", (record_id,))


def application_query(
    connection: sqlite3.Connection, data: WorkloadData, query_index: int
) -> dict[str, Any]:
    spec = data.query_specs[query_index]
    started_total = time.perf_counter_ns()
    started = time.perf_counter_ns()
    selected = [
        str(row[0])
        for row in connection.execute(
            "select target_id from edges where source_id = ? and relationship = 'next' "
            "order by target_id",
            (stable_record_id(spec.seed_record_ordinal),),
        ).fetchall()
    ]
    graph_ns = time.perf_counter_ns() - started
    if not selected:
        raise RuntimeError(f"empty graph selection for {spec.query_id}")

    placeholders = ",".join("?" for _ in selected)
    started = time.perf_counter_ns()
    candidates = connection.execute(
        f"select external_id, text, embedding from chunks "
        f"where record_id in ({placeholders}) and tenant = ? order by external_id",
        (*selected, spec.tenant),
    ).fetchall()
    filter_ns = time.perf_counter_ns() - started

    started = time.perf_counter_ns()
    vector_rows = sorted(
        (
            (
                str(row[0]),
                1.0
                - float(
                    connection.execute(
                        "select vec_distance_cosine(?, ?)",
                        (row[2], data.queries[query_index].tobytes()),
                    ).fetchone()[0]
                ),
            )
            for row in candidates
        ),
        key=lambda value: (-value[1], value[0]),
    )[:TOP_K]
    vector_ns = time.perf_counter_ns() - started

    candidate_ids = {str(value[0]) for value in candidates}
    started = time.perf_counter_ns()
    lexical_rows = [
        (str(row[0]), -float(row[1]))
        for row in connection.execute(
            "select external_id, bm25(chunks_fts) from chunks_fts "
            "where chunks_fts match ? order by bm25(chunks_fts), external_id",
            (spec.query_text,),
        ).fetchall()
        if str(row[0]) in candidate_ids
    ][:TOP_K]
    lexical_ns = time.perf_counter_ns() - started

    started = time.perf_counter_ns()
    vector_scores = dict(vector_rows)
    lexical_scores = dict(lexical_rows)

    def normalize(values: dict[str, float]) -> dict[str, float]:
        if not values:
            return {}
        low = min(values.values())
        high = max(values.values())
        if high == low:
            return {key: 1.0 for key in values}
        return {key: (value - low) / (high - low) for key, value in values.items()}

    vector_normalized = normalize(vector_scores)
    lexical_normalized = normalize(lexical_scores)
    hybrid = sorted(
        (
            (
                value,
                0.6 * vector_normalized.get(value, 0.0)
                + 0.4 * lexical_normalized.get(value, 0.0),
            )
            for value in set(vector_normalized).union(lexical_normalized)
        ),
        key=lambda value: (-value[1], value[0]),
    )[:TOP_K]
    fusion_ns = time.perf_counter_ns() - started

    started = time.perf_counter_ns()
    hydrate_ids = [value[0] for value in hybrid]
    hydrated = []
    if hydrate_ids:
        hydrate_placeholders = ",".join("?" for _ in hydrate_ids)
        rows = connection.execute(
            f"select external_id, text from chunks where external_id in "
            f"({hydrate_placeholders}) order by external_id",
            hydrate_ids,
        ).fetchall()
        hydrated = [str(value[0]) for value in rows]
    hydration_ns = time.perf_counter_ns() - started
    total_ns = time.perf_counter_ns() - started_total
    return {
        "exact_ids": [value[0] for value in vector_rows],
        "hybrid_ids": [value[0] for value in hybrid],
        "hydrated_ids": hydrated,
        "path_identity_sha256": result_identity(
            [stable_record_id(spec.seed_record_ordinal), *selected]
        ),
        "selection_ids": selected,
        "stages": {
            "candidate_filter_intersection": filter_ns,
            "end_to_end_total": total_ns,
            "fusion": fusion_ns,
            "graph_selection": graph_ns,
            "hydration": hydration_ns,
            "lexical_ranking": lexical_ns,
            "vector_ranking": vector_ns,
        },
    }


def run_sqlite_application(
    data: WorkloadData, request: dict[str, Any]
) -> dict[str, Any]:
    result = base_result("sqlite_custom_graph_app", data)
    database_path = Path(request["scratch_root"]) / "sqlite-graph-app.sqlite3"
    started = time.perf_counter_ns()
    connection, vec_version = sqlite_connection(database_path)
    create_application_schema(connection, data.dimension)
    populate_application(connection, data)
    result["build_ns"] = time.perf_counter_ns() - started
    result["system_version"] = (
        f"sqlite-{sqlite3.sqlite_version}+sqlite-vec-{vec_version}"
    )
    save_started = time.perf_counter_ns()
    connection.execute("pragma wal_checkpoint(truncate)")
    connection.commit()
    result["save_ns"] = time.perf_counter_ns() - save_started
    files = [
        value
        for value in directory_sizes(database_path.parent)[1]
        if value["path"].startswith(database_path.name)
    ]
    result["persistence"] = {
        "components": files,
        "total_bytes": sum(int(value["bytes"]) for value in files),
    }
    connection.close()
    load_started = time.perf_counter_ns()
    loaded, loaded_vec_version = sqlite_connection(database_path)
    result["load_ns"] = time.perf_counter_ns() - load_started
    if loaded_vec_version != vec_version:
        raise RuntimeError("sqlite-vec version changed after application reload")
    expected = [
        application_query(loaded, data, value) for value in range(len(data.query_specs))
    ]
    measurement = request["measurement"]
    for value in range(int(measurement["warmups"])):
        application_query(loaded, data, value % len(data.query_specs))
    samples = []
    stage_values: dict[str, list[int]] = {stage: [] for stage in expected[0]["stages"]}
    for sample_index in range(int(measurement["samples"])):
        query_index = sample_index % len(data.query_specs)
        measured = application_query(loaded, data, query_index)
        if measured["exact_ids"] != expected[query_index]["exact_ids"]:
            raise RuntimeError("custom application exact result changed")
        if measured["hybrid_ids"] != expected[query_index]["hybrid_ids"]:
            raise RuntimeError("custom application hybrid result changed")
        for stage, duration in measured["stages"].items():
            stage_values[stage].append(int(duration))
            samples.append(
                {
                    "duration_ns": int(duration),
                    "operation_id": "graph_scoped_application",
                    "query_id": data.query_specs[query_index].query_id,
                    "result_identity_sha256": result_identity(measured["hybrid_ids"]),
                    "sample_index": sample_index,
                    "stage": stage,
                }
            )
    results = []
    replay_results = []
    for index, spec in enumerate(data.query_specs):
        measured = expected[index]
        for operation, ids in [
            ("graph_scoped_exact", measured["exact_ids"]),
            ("graph_scoped_hybrid", measured["hybrid_ids"]),
        ]:
            results.append(
                {
                    "operation_id": operation,
                    "path_identity_sha256": measured["path_identity_sha256"],
                    "query_id": spec.query_id,
                    "result_identity_sha256": result_identity(ids),
                    "result_ids": ids,
                    "scores": [],
                    "selection_ids": measured["selection_ids"],
                }
            )
            replay_results.append(
                {
                    "operation_id": operation,
                    "query_id": spec.query_id,
                    "result_identity_sha256": result_identity(ids),
                    "result_ids": ids,
                }
            )
    result["operations"].append(
        {
            "distribution": {
                stage: distribution(values) for stage, values in stage_values.items()
            },
            "operation_id": "graph_scoped_application",
            "replay_results": replay_results,
            "results": results,
            "samples": samples,
            "timed": True,
        }
    )
    loaded.close()
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


def graph_schema() -> Any:
    from vectorkit_graph import GraphRecordNode, GraphRelationship, GraphSchema

    return GraphSchema(
        record_nodes=[GraphRecordNode("Item", "Item", ["ordinal"])],
        relationships=[
            GraphRelationship(
                "next", "Item", "Item", "next_id", "one", allow_self_edge=False
            ),
            GraphRelationship(
                "linked",
                "Item",
                "Item",
                "linked_ids",
                "many",
                duplicate_references="deduplicate",
            ),
        ],
    )


def populate_vectorkit_graph(data: WorkloadData, builder: Any) -> None:
    active_records = data.active_records
    batch_size = 64
    for start in range(0, active_records, batch_size):
        stop = min(start + batch_size, active_records)
        records = []
        embeddings: dict[str, dict[str, list[float]]] = {}
        for record_ordinal in range(start, stop):
            record_id = stable_record_id(record_ordinal)
            chunks = []
            record_embeddings = {}
            for offset in range(CHUNKS_PER_RECORD):
                ordinal = record_ordinal * CHUNKS_PER_RECORD + offset
                key = f"chunk-{offset}"
                external_id = stable_chunk_id(ordinal)
                chunks.append(
                    {
                        "key": key,
                        "metadata": {
                            "external_id": external_id,
                            "tenant": f"tenant-{ordinal % 10}",
                        },
                        "text": chunk_text(ordinal),
                    }
                )
                record_embeddings[key] = data.vectors[ordinal].tolist()
            records.append(
                {
                    "chunks": chunks,
                    "record": {
                        "fields": {
                            "linked_ids": [
                                stable_record_id((record_ordinal + 7) % active_records)
                            ],
                            "next_id": stable_record_id(
                                (record_ordinal + 1) % active_records
                            ),
                            "ordinal": record_ordinal,
                        },
                        "id": record_id,
                        "record_type": "Item",
                    },
                }
            )
            embeddings[record_id] = record_embeddings
        builder.add(records, embeddings=embeddings)


def vectorkit_graph_query(
    database: Any, data: WorkloadData, query_index: int
) -> dict[str, Any]:
    from vectorkit_graph import GraphNode, GraphTraversal

    spec = data.query_specs[query_index]
    started_total = time.perf_counter_ns()
    started = time.perf_counter_ns()
    selection = database.graph.query(
        seeds=[GraphNode("Item", stable_record_id(spec.seed_record_ordinal))],
        traversals=[GraphTraversal("next")],
    )
    graph_ns = time.perf_counter_ns() - started
    started = time.perf_counter_ns()
    exact_hits = database.retrieval.semantic_search(
        data.queries[query_index].tolist(),
        limit=TOP_K,
        where={"tenant": spec.tenant},
        within=selection,
    )
    exact_ns = time.perf_counter_ns() - started
    started = time.perf_counter_ns()
    hybrid_hits = database.retrieval.hybrid_search(
        spec.query_text,
        data.queries[query_index].tolist(),
        limit=TOP_K,
        where={"tenant": spec.tenant},
        within=selection,
        vector_candidates=100,
        keyword_candidates=100,
        alpha=0.6,
    )
    hybrid_ns = time.perf_counter_ns() - started
    started = time.perf_counter_ns()
    exact_ids = [str(hit["metadata"]["external_id"]) for hit in exact_hits]
    hybrid_ids = [str(hit["metadata"]["external_id"]) for hit in hybrid_hits]
    hydration_ns = time.perf_counter_ns() - started
    matches = selection.matches
    selection_ids = [str(value["node"]["record_id"]) for value in matches]
    path_identity = result_identity(
        [
            json.dumps(value["path"], sort_keys=True, separators=(",", ":"))
            for value in matches
        ]
    )
    selection.close()
    total_ns = time.perf_counter_ns() - started_total
    return {
        "exact_ids": exact_ids,
        "hybrid_ids": hybrid_ids,
        "path_identity_sha256": path_identity,
        "selection_ids": selection_ids,
        "stages": {
            "end_to_end_total": total_ns,
            "graph_selection": graph_ns,
            "hydration": hydration_ns,
            "scoped_exact_ranking": exact_ns,
            "scoped_hybrid_ranking": hybrid_ns,
        },
    }


def run_vectorkit_graph_app(
    data: WorkloadData, request: dict[str, Any]
) -> dict[str, Any]:
    from vectorkit_graph import (
        GraphRetrievalDatabase,
        GraphRetrievalDatabaseBuilder,
        RetrievalConfiguration,
        VectorIndexConfiguration,
    )

    result = base_result("vectorkit_graph_app", data)
    result["system_version"] = request["source_revision"]
    persistence_root = Path(request["scratch_root"]) / "vectorkit-graph-app"
    started = time.perf_counter_ns()
    builder = GraphRetrievalDatabaseBuilder(
        corpus_id=f"phase5-{data.spec['workload_id']}",
        graph=graph_schema(),
        retrieval=RetrievalConfiguration(
            semantic=VectorIndexConfiguration(
                dimension=data.dimension, metric="cosine", encoding="f32"
            )
        ),
    )
    populate_vectorkit_graph(data, builder)
    database = builder.build()
    result["build_ns"] = time.perf_counter_ns() - started
    save_started = time.perf_counter_ns()
    database.save(persistence_root)
    result["save_ns"] = time.perf_counter_ns() - save_started
    total_bytes, components = directory_sizes(persistence_root)
    result["persistence"] = {"components": components, "total_bytes": total_bytes}
    load_started = time.perf_counter_ns()
    loaded = GraphRetrievalDatabase.load(persistence_root)
    result["load_ns"] = time.perf_counter_ns() - load_started
    expected = [
        vectorkit_graph_query(database, data, value)
        for value in range(len(data.query_specs))
    ]
    measurement = request["measurement"]
    for value in range(int(measurement["warmups"])):
        vectorkit_graph_query(database, data, value % len(data.query_specs))
    samples = []
    stage_values: dict[str, list[int]] = {stage: [] for stage in expected[0]["stages"]}
    for sample_index in range(int(measurement["samples"])):
        query_index = sample_index % len(data.query_specs)
        measured = vectorkit_graph_query(database, data, query_index)
        if measured["exact_ids"] != expected[query_index]["exact_ids"]:
            raise RuntimeError("VectorKit graph exact result changed")
        if measured["hybrid_ids"] != expected[query_index]["hybrid_ids"]:
            raise RuntimeError("VectorKit graph hybrid result changed")
        for stage, duration in measured["stages"].items():
            stage_values[stage].append(int(duration))
            samples.append(
                {
                    "duration_ns": int(duration),
                    "operation_id": "graph_scoped_application",
                    "query_id": data.query_specs[query_index].query_id,
                    "result_identity_sha256": result_identity(measured["hybrid_ids"]),
                    "sample_index": sample_index,
                    "stage": stage,
                }
            )
    results = []
    replay_results = []
    for index, spec in enumerate(data.query_specs):
        measured = expected[index]
        replay = vectorkit_graph_query(loaded, data, index)
        for operation, ids, replay_ids in [
            ("graph_scoped_exact", measured["exact_ids"], replay["exact_ids"]),
            ("graph_scoped_hybrid", measured["hybrid_ids"], replay["hybrid_ids"]),
        ]:
            results.append(
                {
                    "operation_id": operation,
                    "path_identity_sha256": measured["path_identity_sha256"],
                    "query_id": spec.query_id,
                    "result_identity_sha256": result_identity(ids),
                    "result_ids": ids,
                    "scores": [],
                    "selection_ids": measured["selection_ids"],
                }
            )
            replay_results.append(
                {
                    "operation_id": operation,
                    "query_id": spec.query_id,
                    "result_identity_sha256": result_identity(replay_ids),
                    "result_ids": replay_ids,
                }
            )
    result["operations"].append(
        {
            "distribution": {
                stage: distribution(values) for stage, values in stage_values.items()
            },
            "operation_id": "graph_scoped_application",
            "replay_results": replay_results,
            "results": results,
            "samples": samples,
            "timed": True,
        }
    )
    result["operations"].append(
        {
            "distribution": None,
            "operation_id": "graph_incremental_deletion",
            "replay_results": [],
            "results": [],
            "samples": [],
            "timed": False,
            "unsupported_reason": "the V1 graph capability is immutable after build",
        }
    )
    result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    return result


RUNNERS: dict[str, Callable[[WorkloadData, dict[str, Any]], dict[str, Any]]] = {
    "numpy_f32_oracle": run_numpy_oracle,
    "sqlite_custom_graph_app": run_sqlite_application,
    "sqlite_vec_exact": run_sqlite_vec_exact,
    "usearch_hnsw": run_usearch,
    "vectorkit_f32_exact": run_vectorkit_exact,
    "vectorkit_graph_app": run_vectorkit_graph_app,
}


def main() -> int:
    arguments = parse_arguments()
    request = json.loads(arguments.request.read_text())
    system_id = str(request["system_id"])
    data = generate_workload(request["workload"])
    Path(request["scratch_root"]).mkdir(parents=True, exist_ok=True)
    try:
        runner = RUNNERS[system_id]
        result = runner(data, request)
    except Exception as error:  # noqa: BLE001 - failures are benchmark artifacts
        result = base_result(system_id, data)
        result["status"] = "failure"
        result["failure"] = {
            "exception_type": type(error).__name__,
            "message": str(error),
            "stage": "adapter_execution",
            "traceback": traceback.format_exc(),
        }
        result["peak_rss_bytes"] = normalized_peak_rss_bytes()
    canonical_file(arguments.output, result)
    return 0 if result["status"] == "success" else 1


if __name__ == "__main__":
    raise SystemExit(main())
