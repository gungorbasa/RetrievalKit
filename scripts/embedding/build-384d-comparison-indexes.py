#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import platform
import shutil
import sys
import time
import types
from pathlib import Path
from typing import Any

import coremltools as ct
import numpy as np
from transformers import AutoTokenizer
from retrievalkit import Index


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE_JSON = Path("/Users/gungorbasa/Desktop/the_social_network_v.1.32.json")
DEFAULT_MODEL_ROOT = ROOT_DIR / "target" / "embedding-models"
DEFAULT_OUTPUT_ROOT = ROOT_DIR / "target" / "examples" / "social-network-48k-384d"
SOCIAL_NETWORK_SCRIPT = (
    ROOT_DIR / "examples" / "python" / "social_network_search" / "social_network_search.py"
)
DIMENSION = 384
DEFAULT_MODEL_SLUGS = [
    "bge-small-en-v1.5",
    "all-MiniLM-L6-v2",
    "e5-small-v2",
    "gte-small",
    "snowflake-arctic-embed-xs",
    "snowflake-arctic-embed-s",
]


def main() -> None:
    args = parse_args()
    source_json = Path(args.json).resolve()
    model_root = Path(args.model_root).resolve()
    output_root = Path(args.output_root).resolve()
    model_slugs = parse_model_slugs(args.models)

    social = load_social_network_module()
    records_start = time.perf_counter()
    records = social.build_records(
        source_json,
        chunk_token_limit=args.chunk_token_limit,
        chunk_overlap=args.chunk_overlap,
    )
    record_prep_ms = elapsed_ms(records_start)
    if len(records) < args.records:
        raise RuntimeError(
            f"source produced {len(records)} records, but --records requested {args.records}"
        )
    records = records[: args.records]

    queries = build_benchmark_queries(
        records=records,
        query_text_from_record=social.query_text_from_record,
        fallback_query=args.query,
        measured_queries=args.measured_queries,
        warmup_queries=args.warmup_queries,
    )

    output_root.mkdir(parents=True, exist_ok=True)
    print("RetrievalKit 384d comparison index build")
    print(f"  source_json: {source_json}")
    print(f"  records: {len(records)}")
    print(f"  chunk_token_limit: {args.chunk_token_limit}")
    print(f"  chunk_overlap: {args.chunk_overlap}")
    print(f"  output_root: {output_root}")
    print(f"  models: {', '.join(model_slugs)}")

    reports = []
    for slug in model_slugs:
        report = build_model_index(
            slug=slug,
            model_dir=model_root / slug,
            output_root=output_root,
            records=records,
            queries=queries,
            record_prep_ms=record_prep_ms,
            batch_size=args.batch_size,
            encoding=args.encoding,
            compute_units=parse_compute_units(args.compute_units),
            top_k=args.limit,
            warmup_queries=args.warmup_queries,
            measured_queries=args.measured_queries,
            rebuild=args.rebuild,
        )
        reports.append(report)

    summary_path = output_root / "summary.json"
    summary_path.write_text(json.dumps({"reports": reports}, indent=2) + "\n", encoding="utf-8")
    print(f"\nWrote summary: {summary_path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build production-style 48K RetrievalKit indexes for 384d Core ML embedding models."
    )
    parser.add_argument("--json", default=str(DEFAULT_SOURCE_JSON), help="source JSON path")
    parser.add_argument("--model-root", default=str(DEFAULT_MODEL_ROOT), help="generated model root")
    parser.add_argument("--output-root", default=str(DEFAULT_OUTPUT_ROOT), help="output root directory")
    parser.add_argument(
        "--models",
        default=",".join(DEFAULT_MODEL_SLUGS),
        help="comma-separated model slugs under --model-root",
    )
    parser.add_argument("--records", type=int, default=48_000, help="number of real records to index")
    parser.add_argument("--batch-size", type=int, default=64, help="embedding/index add batch size")
    parser.add_argument("--chunk-token-limit", type=int, default=250, help="source chunk token limit")
    parser.add_argument("--chunk-overlap", type=int, default=2, help="source chunk overlap")
    parser.add_argument("--encoding", choices=["f32", "f16", "bf16", "i8"], default="i8")
    parser.add_argument(
        "--compute-units",
        choices=["all", "cpu-only", "cpu-and-gpu", "cpu-and-neural-engine"],
        default="all",
        help="Core ML compute units for Python-side embedding",
    )
    parser.add_argument("--query", default="Mark and Erica arguing in a dim bar")
    parser.add_argument("--limit", type=int, default=5, help="top_k recorded in query fixtures")
    parser.add_argument("--warmup-queries", type=int, default=50)
    parser.add_argument("--measured-queries", type=int, default=750)
    parser.add_argument(
        "--rebuild",
        action="store_true",
        help="delete existing per-model output directories before rebuilding",
    )
    args = parser.parse_args()
    if args.records <= 0:
        parser.error("--records must be greater than zero")
    if args.batch_size <= 0:
        parser.error("--batch-size must be greater than zero")
    if args.measured_queries <= 0:
        parser.error("--measured-queries must be greater than zero")
    if args.warmup_queries < 0:
        parser.error("--warmup-queries must be greater than or equal to zero")
    return args


def parse_model_slugs(value: str) -> list[str]:
    slugs = [slug.strip() for slug in value.split(",") if slug.strip()]
    if not slugs:
        raise ValueError("--models must include at least one model slug")
    return slugs


def parse_compute_units(value: str) -> ct.ComputeUnit:
    if value == "all":
        return ct.ComputeUnit.ALL
    if value == "cpu-only":
        return ct.ComputeUnit.CPU_ONLY
    if value == "cpu-and-gpu":
        return ct.ComputeUnit.CPU_AND_GPU
    if value == "cpu-and-neural-engine":
        return ct.ComputeUnit.CPU_AND_NE
    raise ValueError(f"unsupported compute units: {value}")


def build_benchmark_queries(
    *,
    records: list[Any],
    query_text_from_record: Any,
    fallback_query: str,
    measured_queries: int,
    warmup_queries: int,
) -> list[str]:
    requested = measured_queries + warmup_queries
    unique_queries = []
    seen = set()
    for record in records:
        query = query_text_from_record(record.text)
        if query in seen:
            continue
        seen.add(query)
        unique_queries.append(query)
    if not unique_queries:
        unique_queries = [fallback_query]
    return [unique_queries[index % len(unique_queries)] for index in range(requested)]


def load_social_network_module() -> Any:
    fastembed_module = types.ModuleType("fastembed")
    fastembed_module.TextEmbedding = object
    sys.modules.setdefault("fastembed", fastembed_module)

    spec = importlib.util.spec_from_file_location("social_network_search", SOCIAL_NETWORK_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {SOCIAL_NETWORK_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def build_model_index(
    *,
    slug: str,
    model_dir: Path,
    output_root: Path,
    records: list[Any],
    queries: list[str],
    record_prep_ms: float,
    batch_size: int,
    encoding: str,
    compute_units: ct.ComputeUnit,
    top_k: int,
    warmup_queries: int,
    measured_queries: int,
    rebuild: bool,
) -> dict[str, Any]:
    output_dir = output_root / slug
    index_dir = output_dir / "index"
    queries_path = output_dir / "queries.json"
    report_path = output_dir / "build-report.json"

    if output_dir.exists():
        if not rebuild:
            raise RuntimeError(f"{output_dir} already exists; pass --rebuild to replace it")
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)

    metadata = load_model_metadata(model_dir)
    tokenizer = AutoTokenizer.from_pretrained(model_dir / "tokenizer", local_files_only=True)
    model_path = resolve_root_path(metadata["package_path"])

    print(f"\n[{slug}] loading Core ML model: {model_path}")
    model_load_start = time.perf_counter()
    model = ct.models.MLModel(str(model_path), compute_units=compute_units)
    model_load_ms = elapsed_ms(model_load_start)

    build_start = time.perf_counter()
    index = Index(dimension=DIMENSION, metric="cosine", encoding=encoding)
    total_embed_ms = 0.0
    total_add_ms = 0.0
    passage_prefix = str(metadata.get("passage_prefix", ""))

    for batch_number, start in enumerate(range(0, len(records), batch_size), start=1):
        batch = records[start : start + batch_size]
        embed_start = time.perf_counter()
        embeddings = [
            embed_one(
                model=model,
                tokenizer=tokenizer,
                text=passage_prefix + record.text,
                sequence_length=int(metadata["sequence_length"]),
            )
            for record in batch
        ]
        total_embed_ms += elapsed_ms(embed_start)

        documents = [
            {
                "id": record.document_id,
                "metadata": {
                    **record.metadata,
                    "embedding_model_slug": slug,
                    "embedding_model": metadata["model"],
                },
                "chunks": [{"text": record.text, "embedding": embedding}],
            }
            for record, embedding in zip(batch, embeddings)
        ]

        add_start = time.perf_counter()
        index.add(documents=documents)
        total_add_ms += elapsed_ms(add_start)
        print(f"[{slug}] indexed batch {batch_number}: {start + len(batch)}/{len(records)}")

    save_start = time.perf_counter()
    size_report = index.save(index_dir)
    save_ms = elapsed_ms(save_start)
    build_total_ms = elapsed_ms(build_start)

    loaded = Index.load(index_dir)
    if loaded.active_chunk_count != len(records):
        raise RuntimeError(
            f"{slug} active chunk count {loaded.active_chunk_count} != expected {len(records)}"
        )

    query_embed_ms = write_query_embeddings(
        queries=queries,
        queries_path=queries_path,
        model=model,
        tokenizer=tokenizer,
        metadata=metadata,
        slug=slug,
        top_k=top_k,
        warmup_queries=warmup_queries,
        measured_queries=measured_queries,
    )

    validation_query = json.loads(queries_path.read_text(encoding="utf-8"))["queries"][0]
    validation_hits = loaded.search(validation_query["embedding"], limit=top_k)
    if not validation_hits:
        raise RuntimeError(f"{slug} validation search returned no hits")

    report = {
        "model_slug": slug,
        "model": metadata["model"],
        "dimension": DIMENSION,
        "sequence_length": int(metadata["sequence_length"]),
        "encoding": encoding,
        "records": len(records),
        "index_dir": str(index_dir.relative_to(ROOT_DIR)),
        "queries_path": str(queries_path.relative_to(ROOT_DIR)),
        "report_path": str(report_path.relative_to(ROOT_DIR)),
        "size_bytes": size_report["total_bytes"],
        "size_mib": size_report["total_bytes"] / (1024 * 1024),
        "timing_ms": {
            "record_prep": record_prep_ms,
            "model_load": model_load_ms,
            "document_embedding": total_embed_ms,
            "index_add": total_add_ms,
            "save": save_ms,
            "query_embedding": query_embed_ms,
            "build_total": build_total_ms,
        },
        "validation": {
            "query": validation_query["query"],
            "top_document_id": validation_hits[0]["document_id"],
            "top_score": validation_hits[0]["score"],
        },
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"[{slug}] saved index: {index_dir}")
    print(f"[{slug}] size_mib: {report['size_mib']:.3f}")
    print(f"[{slug}] report: {report_path}")
    return report


def load_model_metadata(model_dir: Path) -> dict[str, Any]:
    metadata_path = model_dir / "metadata.json"
    if not metadata_path.exists():
        raise FileNotFoundError(metadata_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if int(metadata["dimension"]) != DIMENSION:
        raise RuntimeError(
            f"{model_dir} dimension {metadata['dimension']} != expected {DIMENSION}"
        )
    return metadata


def resolve_root_path(value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = ROOT_DIR / path
    return path


def write_query_embeddings(
    *,
    queries: list[str],
    queries_path: Path,
    model: Any,
    tokenizer: Any,
    metadata: dict[str, Any],
    slug: str,
    top_k: int,
    warmup_queries: int,
    measured_queries: int,
) -> float:
    sequence_length = int(metadata["sequence_length"])
    query_prefix = str(metadata.get("query_prefix", ""))
    embed_start = time.perf_counter()
    query_records = [
        {
            "query": query,
            "embedding": embed_one(
                model=model,
                tokenizer=tokenizer,
                text=query_prefix + query,
                sequence_length=sequence_length,
            ),
        }
        for query in queries
    ]
    embed_ms = elapsed_ms(embed_start)

    queries_path.write_text(
        json.dumps(
            {
                "model_slug": slug,
                "model": metadata["model"],
                "sequence_length": sequence_length,
                "dimension": DIMENSION,
                "top_k": top_k,
                "warmup_queries": warmup_queries,
                "measured_queries": measured_queries,
                "query_prefix": query_prefix,
                "queries": query_records,
                "query_embedding_total_ms": embed_ms,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return embed_ms


def embed_one(
    *,
    model: Any,
    tokenizer: Any,
    text: str,
    sequence_length: int,
) -> list[float]:
    encoded = tokenizer(
        text,
        padding="max_length",
        truncation=True,
        max_length=sequence_length,
        return_tensors="np",
    )
    token_type_ids = encoded.get("token_type_ids")
    if token_type_ids is None:
        token_type_ids = np.zeros_like(encoded["input_ids"])
    prediction = model.predict(
        {
            "input_ids": encoded["input_ids"].astype(np.int32),
            "attention_mask": encoded["attention_mask"].astype(np.int32),
            "token_type_ids": token_type_ids.astype(np.int32),
        }
    )
    embedding = np.asarray(prediction["embedding"], dtype=np.float32).reshape(-1)
    if embedding.shape[0] != DIMENSION:
        raise RuntimeError(f"embedding dimension {embedding.shape[0]} != {DIMENSION}")
    return embedding.tolist()


def elapsed_ms(start: float) -> float:
    return (time.perf_counter() - start) * 1000.0


if __name__ == "__main__":
    main()
