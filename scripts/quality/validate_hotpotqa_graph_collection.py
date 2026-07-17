#!/usr/bin/env python3
"""Independently validate the frozen HotpotQA V3 graph adapter output."""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import math
import re
import struct
import subprocess
import sys
import tarfile
import unicodedata
from collections import Counter
from decimal import Decimal
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence


ROOT = Path(__file__).resolve().parents[2]
EXPECTED_CORPUS_HASH = "a59dd4edc535abde55d27aa8262d64b99d7a25c05754cd0724fef5216c5204c6"
EXPECTED_EMBEDDINGS = {
    "corpus": "0dd2c67f457f8a1b075056410102966b8632d0fcf3ff136face0ce247d7653e7",
    "development": "ad75e5a803158930969c30572cf11857b6f942904f48c867e137f86b2eeb9402",
    "test": "81f7413fb572bbf5e9391d4d32b64a96fe5b6c8b20c3ecd931e0b26a6b55f96c",
}
EXPECTED_SPLITS = {
    "development": {
        "queries": 603,
        "qrels": 1_206,
        "evidence": 603,
        "exclusions": 1_401,
        "global": 1_397,
        "derived": 4,
        "population": "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f",
        "derived_population": "da343545fa764b44c5382f4a16c933dded7bd613ae6e12768b5c2772c6739582",
    },
    "test": {
        "queries": 297,
        "qrels": 594,
        "evidence": 297,
        "exclusions": 704,
        "global": 703,
        "derived": 1,
        "population": "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010",
        "derived_population": "93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8",
    },
}
EXPECTED_RESOLUTIONS = {"ambiguous": 235, "no_match": 2, "resolved": 2_763}
EXPECTED_FILES = {
    "collection.json",
    "records.jsonl",
    "graph-schema.json",
    "queries.jsonl",
    "corpus-embeddings.f32.jsonl",
    "query-embeddings.f32.jsonl",
    "qrels.tsv",
    "evidence-judgments.jsonl",
    "expected-paths.jsonl",
    "exclusions.jsonl",
    "manifests/preprocessing.json",
    "manifests/chunking.json",
    "manifests/embedding.json",
    "manifests/graph-construction.json",
    "manifests/seed-policy.json",
    "manifests/split.json",
}
EXPECTED_ROOT_FILES = {"adapter-manifest.json", "inspection.json", "source-inventory.json"}
MODEL_FILES = {
    "AllMiniLML6V2.mlpackage/Data/com.apple.CoreML/model.mlmodel": "bb7f068c83217c5f4a39b4bad4aa75525847803485b46b7c226454a7d8f5e2fe",
    "AllMiniLML6V2.mlpackage/Data/com.apple.CoreML/weights/weight.bin": "84cbd97f75e18368c9ba9566bb51614f8f7d56f659c171124bf4447cc2145bde",
    "AllMiniLML6V2.mlpackage/Manifest.json": "e016b09b0886f4716add9817fe1ba040a201681e27bae5f317a34bab30c39afa",
    "metadata.json": "31367d7310f9d5adcc727bf8f52bfb0bc6c6b31512fa3d83b7d5224cddf59784",
    "tokenizer/tokenizer.json": "da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0",
    "tokenizer/tokenizer_config.json": "872b6936be955bc3aea75ed599264d865626d68feede7e58b01e378e6332bd74",
}
SOURCE_FILES = {
    "enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2": (
        1_553_565_403,
        "1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729",
    ),
    "hotpotqa-distractor-train-00000-1908d6af.parquet": (
        165_624_177,
        "76d3bb3048a7cc73c1958107c0c5872a00d7e7d00c105b81e92f6769e7822e68",
    ),
    "hotpotqa-distractor-train-00001-1908d6af.parquet": (
        166_162_479,
        "713661628434fbb19fff7392e2e321e4ed107e3c7c7784d0690946e5f722763f",
    ),
    "hotpotqa-distractor-validation-1908d6af.parquet": (
        27_452_575,
        "c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6",
    ),
}
FLOAT_TOKEN = re.compile(r"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:e-?[0-9]+)?$")


class ValidationError(RuntimeError):
    """An independently checked adapter invariant failed."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical(value: Any, *, final_lf: bool = False) -> bytes:
    data = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    return data + (b"\n" if final_lf else b"")


def lexical(value: str) -> bytes:
    return value.encode("utf-8")


def population_hash(ids: Iterable[str]) -> str:
    return sha256(b"".join(f"{value}\n".encode() for value in sorted(ids, key=lexical)))


def normalize(value: str) -> str:
    return unicodedata.normalize(
        "NFC", " ".join(unicodedata.normalize("NFC", value).casefold().split())
    )


def require_version(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        raise ValidationError(f"unknown {label}: expected {expected!r}, found {actual!r}")


def verify_source_file(path: Path, expected_bytes: int, expected_sha256: str) -> None:
    if not path.is_file():
        raise ValidationError(f"missing pinned source: {path}")
    if path.stat().st_size != expected_bytes:
        raise ValidationError(f"source checksum mismatch (byte count): {path}")
    if digest_file(path) != expected_sha256:
        raise ValidationError(f"source checksum mismatch (SHA-256): {path}")


def collect_files(root: Path) -> set[str]:
    result: set[str] = set()
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValidationError(f"symlink is forbidden in adapter output: {path}")
        if path.is_file():
            result.add(path.relative_to(root).as_posix())
    return result


def require_inventory(root: Path, expected: set[str]) -> None:
    actual = collect_files(root)
    if actual != expected:
        raise ValidationError(
            f"missing or extra file: expected {sorted(expected)}, found {sorted(actual)}"
        )


def read_canonical_json(path: Path) -> Any:
    data = path.read_bytes()
    if not data.endswith(b"\n") or data.endswith(b"\n\n") or data.startswith(b"\xef\xbb\xbf"):
        raise ValidationError(f"noncanonical serialization: {path}")
    try:
        value = json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid canonical JSON: {path}") from error
    if data != canonical(value, final_lf=True):
        raise ValidationError(f"noncanonical serialization: {path}")
    return value


def read_canonical_jsonl(path: Path) -> list[dict[str, Any]]:
    data = path.read_bytes()
    if not data:
        return []
    if not data.endswith(b"\n") or data.endswith(b"\n\n") or b"\r" in data:
        raise ValidationError(f"noncanonical serialization: {path}")
    rows: list[dict[str, Any]] = []
    for offset, line in enumerate(data.splitlines(), 1):
        try:
            row = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValidationError(f"invalid JSONL at {path}:{offset}") from error
        if line + b"\n" != canonical(row, final_lf=True):
            raise ValidationError(f"noncanonical serialization at {path}:{offset}")
        rows.append(row)
    return rows


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def normalize_exponent(value: str) -> str:
    if "e" not in value.lower():
        return value
    significand, exponent = value.lower().split("e")
    return f"{significand}e{int(exponent)}"


def independent_canonical_f32(value: float) -> str:
    value = f32(value)
    if not math.isfinite(value):
        raise ValidationError("nonfinite embedding value")
    if value == 0.0:
        return "0"
    candidate = None
    for precision in range(1, 10):
        probe = normalize_exponent(format(value, f".{precision}g"))
        if f32(float(probe)) == value:
            candidate = probe
            break
    if candidate is None:
        raise ValidationError(f"could not derive shortest F32 for {value}")
    exponent = (
        int(candidate.lower().split("e")[1])
        if "e" in candidate.lower()
        else Decimal(candidate).adjusted()
    )
    if -6 <= exponent <= 20:
        fixed = format(Decimal(candidate), "f")
        if "." in fixed:
            fixed = fixed.rstrip("0").rstrip(".")
        return fixed
    if "e" not in candidate:
        candidate = format(Decimal(candidate), "e")
    significand, exp = candidate.lower().split("e")
    significand = significand.rstrip("0").rstrip(".")
    return f"{significand}e{int(exp)}"


def validate_embedding_file(
    path: Path, expected_identities: Sequence[tuple[str, ...]], expected_hash: str
) -> None:
    if digest_file(path) != expected_hash:
        raise ValidationError(f"embedding checksum mismatch: {path}")
    with path.open("rb") as source:
        for offset, (line, identity) in enumerate(
            zip(source, expected_identities, strict=True), 1
        ):
            if not line.endswith(b"\n") or b" " in line:
                raise ValidationError(f"noncanonical embedding serialization at row {offset}")
            row = json.loads(line)
            actual = (
                (row.get("record_id"), row.get("chunk_key"))
                if len(identity) == 2
                else (row.get("query_id"),)
            )
            if actual != identity:
                raise ValidationError(f"embedding order mismatch at row {offset}")
            values = row.get("values")
            if not isinstance(values, list) or len(values) != 384:
                raise ValidationError(f"embedding dimension mismatch at row {offset}")
            if not all(isinstance(value, (int, float)) and math.isfinite(value) for value in values):
                raise ValidationError(f"embedding contains nonfinite value at row {offset}")
            norm = math.sqrt(sum(float(value) * float(value) for value in values))
            if norm != 0.0 and abs(norm - 1.0) > 2e-5:
                raise ValidationError(f"embedding norm mismatch at row {offset}")
            text = line.decode("utf-8")
            value_text = text[text.index("[", text.index("values")) + 1 : text.rindex("]")]
            tokens = value_text.split(",")
            if len(tokens) != 384:
                raise ValidationError(f"embedding token count mismatch at row {offset}")
            for token, value in zip(tokens, values, strict=True):
                if not FLOAT_TOKEN.fullmatch(token) or token != independent_canonical_f32(value):
                    raise ValidationError(f"noncanonical F32 at row {offset}: {token}")
        if source.read(1):
            raise ValidationError(f"unexpected embedding rows: {path}")


def verify_manifest_hashes(root: Path, collection: Mapping[str, Any]) -> None:
    indexed = {row["path"]: row for row in collection["files"]}
    if set(indexed) != EXPECTED_FILES - {"collection.json"}:
        raise ValidationError("manifest closure has missing or unexpected file")
    for relative, entry in indexed.items():
        data = (root / relative).read_bytes()
        if len(data) != entry["bytes"] or sha256(data) != entry["sha256"]:
            raise ValidationError(f"manifest hash mismatch: {relative}")
    for name in (
        "preprocessing",
        "chunking",
        "embedding",
        "graph-construction",
        "seed-policy",
        "split",
    ):
        manifest = read_canonical_json(root / f"manifests/{name}.json")
        if set(manifest) != {
            "inputs",
            "outputs",
            "parameters",
            "policy_id",
            "policy_version",
            "schema_version",
            "tool",
        }:
            raise ValidationError(f"closed manifest schema mismatch: {name}")
        require_version(manifest["schema_version"], 1, f"manifest version {name}")
        for output in manifest["outputs"]:
            if sha256((root / output["path"]).read_bytes()) != output["sha256"]:
                raise ValidationError(f"manifest hash mismatch: {name}/{output['path']}")


def validate_graph_and_records(
    records: Sequence[Mapping[str, Any]], schema: Mapping[str, Any], inspection: Mapping[str, Any]
) -> tuple[list[str], int]:
    expected_schema = {
        "chunk_nodes": {
            "inverse_relationship": "PartOf",
            "node_type": "Chunk",
            "owns_relationship": "ContainsChunk",
        },
        "record_nodes": [
            {
                "node_type": "Article",
                "queryable_fields": [["title"]],
                "record_type": "WikipediaArticle",
            }
        ],
        "relationships": [
            {
                "allow_self_edge": False,
                "cardinality": "many",
                "duplicate_references": "deduplicate",
                "inverse_relationship": None,
                "missing_target": "omit_edge",
                "relationship_type": "LinksTo",
                "source_field": ["outgoing_record_ids"],
                "source_node_type": "Article",
                "target_node_type": "Article",
            }
        ],
        "version": 1,
    }
    if schema != expected_schema:
        raise ValidationError("graph schema or graph provenance mismatch")
    if len(records) != 12_670:
        raise ValidationError("corpus count/hash mismatch: record count")
    ids = [row["record_id"] for row in records]
    if ids != sorted(ids, key=lexical) or len(set(ids)) != len(ids):
        raise ValidationError("record identities are not strict lexical order")
    known = set(ids)
    edge_count = 0
    preimage_records = []
    forbidden = {"answer", "context", "supporting_facts", "qrels", "evidence", "paths"}
    for row in records:
        if set(row) != {"chunks", "content", "fields", "metadata", "record_id", "record_type"}:
            raise ValidationError("record closed schema mismatch")
        if row["record_type"] != "WikipediaArticle" or len(row["chunks"]) != 1:
            raise ValidationError("record/chunk identity mismatch")
        chunk = row["chunks"][0]
        if chunk["chunk_key"] != "abstract" or chunk["metadata"] != {}:
            raise ValidationError("record/chunk identity mismatch")
        title = row["fields"]["title"]["value"]
        source_id = row["fields"]["upstream_page_id"]["value"]
        if row["record_id"] != f"hotpotqa:wiki:{source_id}" or not source_id.isdecimal():
            raise ValidationError("stable record identity mismatch")
        if chunk["text"] != f"{title}\n\n{row['content']}":
            raise ValidationError("embedding text construction mismatch")
        outgoing = tuple(
            value["value"] for value in row["fields"]["outgoing_record_ids"]["value"]
        )
        if outgoing != tuple(sorted(set(outgoing), key=lexical)):
            raise ValidationError("link deduplication/order mismatch")
        if any(target not in known or target == row["record_id"] for target in outgoing):
            raise ValidationError("missing target or self graph edge")
        edge_count += len(outgoing)
        preimage_records.append(
            {
                "outgoing_record_ids": list(outgoing),
                "record_id": row["record_id"],
                "source_id": source_id,
                "text": row["content"],
                "title": title,
            }
        )
        if forbidden & set(row["fields"]):
            raise ValidationError("graph edge derived from a judgment")
    if edge_count != 43_737:
        raise ValidationError(f"graph edge count mismatch: {edge_count}")
    corpus_info = inspection["corpus"]
    preimage = {
        "conflict_policy": "lowest numeric Wikipedia page ID",
        "neighbor_limit": 15,
        "records": preimage_records,
        "records_seen": corpus_info["source_records"],
        "sample_salt": "vectorkit-hotpotqa-linked-abstracts-v1",
        "selected_conflicting_titles": corpus_info["selected_conflicting_titles"],
        "selected_missing_titles": corpus_info["selected_missing_titles"],
        "source_conflicting_titles": corpus_info["source_conflicting_title_count"],
    }
    if sha256(canonical(preimage)) != EXPECTED_CORPUS_HASH:
        raise ValidationError("corpus count/hash mismatch: preimage SHA-256")
    if (
        corpus_info["source_records"] != 5_233_329
        or corpus_info["source_unique_titles"] != 5_230_693
        or corpus_info["source_conflicting_title_count"] != 2_619
        or len(corpus_info["selected_missing_titles"]) != 1_776
        or len(corpus_info["selected_conflicting_titles"]) != 59
    ):
        raise ValidationError("corpus count/hash mismatch: diagnostics")
    return ids, edge_count


def validate_split(root: Path, split: str, inspection: Mapping[str, Any]) -> dict[str, Any]:
    require_inventory(root, EXPECTED_FILES)
    collection = read_canonical_json(root / "collection.json")
    require_version(collection["schema_version"], 3, "collection version")
    require_version(collection["collection_version"], "1", "collection identity version")
    if collection["split"] != split:
        raise ValidationError("collection split mismatch")
    verify_manifest_hashes(root, collection)
    records = read_canonical_jsonl(root / "records.jsonl")
    schema = read_canonical_json(root / "graph-schema.json")
    record_ids, edges = validate_graph_and_records(records, schema, inspection)
    queries = read_canonical_jsonl(root / "queries.jsonl")
    evidence = read_canonical_jsonl(root / "evidence-judgments.jsonl")
    exclusions = read_canonical_jsonl(root / "exclusions.jsonl")
    if (root / "expected-paths.jsonl").read_bytes() != b"":
        raise ValidationError("nonzero expected-path file")
    qrel_rows = []
    previous = None
    for line in (root / "qrels.tsv").read_text(encoding="utf-8").splitlines():
        fields = line.split(" ")
        if len(fields) != 4 or fields[1] != "0" or fields[3] != "1":
            raise ValidationError("noncanonical qrels row")
        key = (fields[0], fields[2])
        if previous is not None and key <= previous:
            raise ValidationError("noncanonical qrel ordering")
        previous = key
        qrel_rows.append(key)
    expected = EXPECTED_SPLITS[split]
    actual_counts = (len(queries), len(qrel_rows), len(evidence), len(exclusions))
    if actual_counts != (
        expected["queries"],
        expected["qrels"],
        expected["evidence"],
        expected["exclusions"],
    ):
        raise ValidationError(f"split count mismatch: {actual_counts}")
    query_ids = [row["query_id"] for row in queries]
    if query_ids != sorted(query_ids, key=lexical) or population_hash(query_ids) != expected["population"]:
        raise ValidationError("population mismatch")
    by_query_qrels = Counter(query_id for query_id, _ in qrel_rows)
    evidence_by_id = {row["query_id"]: row for row in evidence}
    if len(evidence_by_id) != len(evidence):
        raise ValidationError("missing evidence document")
    for query in queries:
        if (
            set(query) != {
                "category",
                "derived_seed_policy_id",
                "explicit_seed",
                "metadata_filter",
                "query_id",
                "split",
                "tasks",
                "text",
                "traversal",
            }
            or query["tasks"] != ["evidence", "retrieval"]
            or query["explicit_seed"] is not None
            or query["metadata_filter"] is not None
            or query["derived_seed_policy_id"] != "hotpotqa-exact-title-v1"
        ):
            raise ValidationError("illegal gold seed or query schema mismatch")
        query_id = query["query_id"]
        if by_query_qrels[query_id] != 2 or query_id not in evidence_by_id:
            raise ValidationError("missing evidence document")
        evidence_set = evidence_by_id[query_id]["evidence_sets"]
        if len(evidence_set) != 1 or len(evidence_set[0]) != 2:
            raise ValidationError("missing evidence document")
        if any(record_id not in record_ids for record_id in evidence_set[0]):
            raise ValidationError("missing evidence document")
    global_count = sum(row["lane"] == "global" for row in exclusions)
    derived_rows = [row for row in exclusions if row["lane"] != "global"]
    if global_count != expected["global"] or len(derived_rows) != expected["derived"]:
        raise ValidationError("exclusion count/reason mismatch")
    if any(row["reason"] != "not_in_frozen_corpus" for row in exclusions if row["lane"] == "global"):
        raise ValidationError("exclusion count/reason mismatch")
    if any(row["reason"] != "derived_seed_ambiguous" for row in derived_rows):
        raise ValidationError("exclusion count/reason mismatch")
    successful = set(query_ids) - {row["query_id"] for row in derived_rows}
    if population_hash(successful) != expected["derived_population"]:
        raise ValidationError("derived population mismatch")
    seed_manifest = read_canonical_json(root / "manifests/seed-policy.json")
    policy = seed_manifest["parameters"]["derived_policies"][0]
    aliases = policy["aliases"]
    if len(aliases) != 12_670 or sha256(canonical(aliases)) != policy["alias_table_sha256"]:
        raise ValidationError("alias resolution provenance mismatch")
    if seed_manifest["parameters"]["explicit_policy"]["provenance"] != []:
        raise ValidationError("illegal gold seed")
    validate_embedding_file(
        root / "corpus-embeddings.f32.jsonl",
        [(record_id, "abstract") for record_id in record_ids],
        EXPECTED_EMBEDDINGS["corpus"],
    )
    validate_embedding_file(
        root / "query-embeddings.f32.jsonl",
        [(query_id,) for query_id in query_ids],
        EXPECTED_EMBEDDINGS[split],
    )
    return {"collection": collection, "edges": edges, "query_ids": query_ids}


def verify_builder_isolation(builder: Path) -> None:
    tree = ast.parse(builder.read_text(encoding="utf-8"))
    source_fields = None
    freeze_args = None
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == "SourceQuery":
            source_fields = [
                child.target.id
                for child in node.body
                if isinstance(child, ast.AnnAssign) and isinstance(child.target, ast.Name)
            ]
        if isinstance(node, ast.FunctionDef) and node.name == "freeze_source_corpus":
            freeze_args = [argument.arg for argument in node.args.args]
    if source_fields != ["upstream_id", "split", "question_text", "query_type", "level"]:
        raise ValidationError("gold-label access during corpus construction")
    forbidden = {"answer", "context", "supporting_facts", "qrels", "evidence", "paths"}
    if freeze_args is None or forbidden & set(freeze_args):
        raise ValidationError("gold-label access during corpus construction")


def validate_sources(cache_dir: Path, model_dir: Path, source_inventory: Mapping[str, Any]) -> None:
    downloads = cache_dir / "downloads"
    for filename, (size, digest) in SOURCE_FILES.items():
        verify_source_file(downloads / filename, size, digest)
    if list(downloads.glob("*.download")):
        raise ValidationError("partial source download is present")
    archive = downloads / "enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2"
    with tarfile.open(archive, "r:bz2") as bundle:
        members = bundle.getmembers()
    inventory = [
        {
            "name": member.name,
            "size": member.size,
            "type": "dir" if member.isdir() else "file",
        }
        for member in members
    ]
    if len(members) != 15_674 or sum(member.isfile() for member in members) != 15_517:
        raise ValidationError("unknown source version: archive inventory")
    if sha256(canonical(inventory)) != "e2c7b289c1ed0c7e11faabd9ef1b37bceeea1a997e3673657bdfee053c6450cf":
        raise ValidationError("unknown source version: archive inventory hash")
    for relative, expected in MODEL_FILES.items():
        if digest_file(model_dir / relative) != expected:
            raise ValidationError(f"model/tokenizer checksum mismatch: {relative}")
    require_version(source_inventory["schema_version"], 1, "source inventory version")
    acceptance = source_inventory["license_acceptance"]
    if not acceptance["required_before_download"] or acceptance["license_id"] != "CC-BY-SA-4.0":
        raise ValidationError("license and attribution material mismatch")


def validate_adapter(
    root: Path,
    cache_dir: Path,
    model_dir: Path,
    production_cli: Path | None,
) -> dict[str, Any]:
    if not root.is_dir():
        raise ValidationError(f"adapter root is missing: {root}")
    root_entries = {path.name for path in root.iterdir()}
    if root_entries != EXPECTED_ROOT_FILES | {"development", "test"}:
        raise ValidationError("stale output file or partial atomic publication")
    require_inventory(root / "development", EXPECTED_FILES)
    require_inventory(root / "test", EXPECTED_FILES)
    source_inventory = read_canonical_json(root / "source-inventory.json")
    inspection = read_canonical_json(root / "inspection.json")
    adapter_manifest = read_canonical_json(root / "adapter-manifest.json")
    validate_sources(cache_dir, model_dir, source_inventory)
    verify_builder_isolation(ROOT / "scripts/quality/build_hotpotqa_graph_collection.py")
    require_version(adapter_manifest["schema_version"], 1, "adapter manifest version")
    development = validate_split(root / "development", "development", inspection)
    test = validate_split(root / "test", "test", inspection)
    if set(development["query_ids"]) & set(test["query_ids"]):
        raise ValidationError("development/test query populations overlap")
    for shared in (
        "records.jsonl",
        "graph-schema.json",
        "corpus-embeddings.f32.jsonl",
    ):
        if (root / "development" / shared).read_bytes() != (root / "test" / shared).read_bytes():
            raise ValidationError(f"shared corpus file is not byte-identical: {shared}")
    resolutions = inspection["seed_resolutions"]
    if dict(sorted(Counter(row["status"] for row in resolutions).items())) != EXPECTED_RESOLUTIONS:
        raise ValidationError("alias resolution provenance mismatch")
    indexed = {row["path"]: row for row in adapter_manifest["files"]}
    expected_indexed = {
        "inspection.json",
        "source-inventory.json",
        *{f"development/{path}" for path in EXPECTED_FILES},
        *{f"test/{path}" for path in EXPECTED_FILES},
    }
    if set(indexed) != expected_indexed:
        raise ValidationError("adapter-manifest closure mismatch")
    for relative, entry in indexed.items():
        data = (root / relative).read_bytes()
        if len(data) != entry["bytes"] or sha256(data) != entry["sha256"]:
            raise ValidationError(f"adapter manifest hash mismatch: {relative}")
    production = []
    if production_cli is not None:
        for split in ("development", "test"):
            command = [
                str(production_cli),
                "bench",
                "quality-v3",
                "--collection",
                str(root / split),
                "--production-ingestion",
            ]
            result = subprocess.run(command, text=True, capture_output=True, check=False)
            if result.returncode != 0:
                raise ValidationError(
                    f"production-backed ingestion failed for {split}: {result.stderr.strip()}"
                )
            production.append(json.loads(result.stdout))
    return {
        "adapter_manifest_sha256": digest_file(root / "adapter-manifest.json"),
        "corpus_preimage_sha256": EXPECTED_CORPUS_HASH,
        "development_collection_sha256": digest_file(root / "development/collection.json"),
        "production_ingestion": production,
        "status": "valid",
        "test_collection_sha256": digest_file(root / "test/collection.json"),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--cache-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--production-cli", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    result = validate_adapter(
        args.root, args.cache_dir, args.model_dir, args.production_cli
    )
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except ValidationError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
