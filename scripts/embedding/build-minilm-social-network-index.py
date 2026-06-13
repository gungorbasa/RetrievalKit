#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import subprocess
import sys
import time
import types
from pathlib import Path
from typing import Any, Protocol

from vectorkit import Index


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE_JSON = Path("/Users/gungorbasa/Desktop/the_social_network_v.1.32.json")
DEFAULT_MODEL_DIR = ROOT_DIR / "target" / "embedding-models" / "all-MiniLM-L6-v2"
DEFAULT_INDEX_DIR = ROOT_DIR / "target" / "examples" / "social-network-index-minilm"
DEFAULT_QUERIES_PATH = ROOT_DIR / "target" / "examples" / "social-network-minilm-queries.json"
DEFAULT_RUST_EMBEDDER = ROOT_DIR / "target" / "release" / "embeddingkit-coreml-embed"
SOCIAL_NETWORK_SCRIPT = ROOT_DIR / "examples" / "python" / "social_network_search" / "social_network_search.py"
DIMENSION = 384


def main() -> None:
    args = parse_args()
    source_json = Path(args.json)
    model_dir = Path(args.model_dir)
    index_dir = Path(args.index_dir)
    queries_path = Path(args.queries_path)

    social = load_social_network_module()
    metadata = json.loads((model_dir / "metadata.json").read_text())
    sequence_length = int(metadata["sequence_length"])

    records_start = time.perf_counter()
    records = social.build_records(
        source_json,
        chunk_token_limit=args.chunk_token_limit,
        chunk_overlap=args.chunk_overlap,
    )
    if args.max_records > 0:
        records = records[: args.max_records]
    record_prep_ms = elapsed_ms(records_start)

    with make_embedder(args, model_dir, sequence_length) as embedder:
        build_index(
            records=records,
            index_dir=index_dir,
            embedder=embedder,
            batch_size=args.batch_size,
            record_prep_ms=record_prep_ms,
        )

        queries = social.build_benchmark_queries(
            source_path=source_json,
            fallback_query=args.query,
            measured_queries=args.measured_queries,
            warmup_queries=args.warmup_queries,
            chunk_token_limit=args.chunk_token_limit,
            chunk_overlap=args.chunk_overlap,
        )
        write_query_embeddings(
            queries=queries,
            queries_path=queries_path,
            embedder=embedder,
            sequence_length=sequence_length,
            warmup_queries=args.warmup_queries,
            measured_queries=args.measured_queries,
            top_k=args.limit,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build The Social Network VectorKit index with all-MiniLM-L6-v2 Core ML embeddings."
    )
    parser.add_argument("--json", default=str(DEFAULT_SOURCE_JSON), help="source JSON path")
    parser.add_argument("--model-dir", default=str(DEFAULT_MODEL_DIR), help="generated MiniLM model directory")
    parser.add_argument("--index-dir", default=str(DEFAULT_INDEX_DIR), help="output VectorKit index directory")
    parser.add_argument("--queries-path", default=str(DEFAULT_QUERIES_PATH), help="output query embedding JSON")
    parser.add_argument("--query", default="Mark and Erica arguing in a dim bar", help="fallback query")
    parser.add_argument("--limit", type=int, default=5, help="top_k recorded in query fixture")
    parser.add_argument("--batch-size", type=int, default=64, help="indexing batch size")
    parser.add_argument("--chunk-token-limit", type=int, default=500, help="source chunk token limit")
    parser.add_argument("--chunk-overlap", type=int, default=2, help="source chunk overlap")
    parser.add_argument("--max-records", type=int, default=0, help="optional smoke-test record cap")
    parser.add_argument("--warmup-queries", type=int, default=50, help="warmup query count")
    parser.add_argument("--measured-queries", type=int, default=750, help="measured query count")
    parser.add_argument(
        "--embedding-runtime",
        choices=("rust-coreml", "python-coreml"),
        default="rust-coreml",
        help="embedding runtime used to calculate document and query vectors",
    )
    parser.add_argument(
        "--rust-embedder",
        default=str(DEFAULT_RUST_EMBEDDER),
        help="path to the embeddingkit-coreml-embed release binary",
    )
    parser.add_argument(
        "--compute",
        default="cpuAndNeuralEngine",
        help="Core ML compute mode for rust-coreml: all, cpuAndNeuralEngine, cpuAndGPU, or cpu",
    )
    return parser.parse_args()


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


def build_index(
    *,
    records: list[Any],
    index_dir: Path,
    embedder: "TextEmbedder",
    batch_size: int,
    record_prep_ms: float,
) -> None:
    build_start = time.perf_counter()
    if index_dir.exists():
        shutil.rmtree(index_dir)

    index = Index(dimension=DIMENSION, metric="cosine", encoding="i8")
    total_embed_ms = 0.0
    total_add_ms = 0.0

    for batch_number, start in enumerate(range(0, len(records), batch_size), start=1):
        batch = records[start : start + batch_size]
        texts = [record.text for record in batch]

        embed_start = time.perf_counter()
        embeddings = embedder.embed_many(texts)
        total_embed_ms += elapsed_ms(embed_start)

        documents = []
        for record, embedding in zip(batch, embeddings):
            documents.append(
                {
                    "id": record.document_id,
                    "metadata": record.metadata,
                    "chunks": [
                        {
                            "text": record.text,
                            "embedding": embedding,
                        }
                    ],
                }
            )

        add_start = time.perf_counter()
        index.add(documents=documents)
        total_add_ms += elapsed_ms(add_start)
        print(f"Indexed batch {batch_number}: {len(documents)} records")

    save_start = time.perf_counter()
    report = index.save(index_dir)
    save_ms = elapsed_ms(save_start)
    build_ms = elapsed_ms(build_start)

    print(f"Saved MiniLM index to {index_dir}")
    print(f"Persisted size: {report['total_bytes'] / (1024 * 1024):.3f} MiB")
    print("\nBuild timing:")
    print(f"  record_prep_ms: {record_prep_ms:.3f}")
    print(f"  embedding_total_ms: {total_embed_ms:.3f}")
    print(f"  index_add_total_ms: {total_add_ms:.3f}")
    print(f"  save_ms: {save_ms:.3f}")
    print(f"  build_total_ms: {build_ms:.3f}")


def write_query_embeddings(
    *,
    queries: list[str],
    queries_path: Path,
    embedder: "TextEmbedder",
    sequence_length: int,
    warmup_queries: int,
    measured_queries: int,
    top_k: int,
) -> None:
    embed_start = time.perf_counter()
    query_records = [
        {"query": query, "embedding": embedding}
        for query, embedding in zip(queries, embedder.embed_many(queries))
    ]
    embed_ms = elapsed_ms(embed_start)

    queries_path.parent.mkdir(parents=True, exist_ok=True)
    queries_path.write_text(
        json.dumps(
            {
                "model": "sentence-transformers/all-MiniLM-L6-v2",
                "sequence_length": sequence_length,
                "dimension": DIMENSION,
                "embedding_runtime": embedder.name,
                "top_k": top_k,
                "warmup_queries": warmup_queries,
                "measured_queries": measured_queries,
                "queries": query_records,
                "query_embedding_total_ms": embed_ms,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"Wrote query embeddings to {queries_path}")
    print(f"Query embedding total ms: {embed_ms:.3f}")


class TextEmbedder(Protocol):
    name: str

    def __enter__(self) -> "TextEmbedder":
        ...

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        ...

    def embed_many(self, texts: list[str]) -> list[list[float]]:
        ...


def make_embedder(args: argparse.Namespace, model_dir: Path, sequence_length: int) -> TextEmbedder:
    if args.embedding_runtime == "rust-coreml":
        return RustCoreMLEmbedder(
            executable=Path(args.rust_embedder),
            model_dir=model_dir,
            compute=args.compute,
        )
    return PythonCoreMLEmbedder(model_dir=model_dir, sequence_length=sequence_length)


class RustCoreMLEmbedder:
    name = "rust-coreml"

    def __init__(self, *, executable: Path, model_dir: Path, compute: str) -> None:
        self.executable = executable
        self.model_dir = model_dir
        self.compute = compute
        self.process: subprocess.Popen[str] | None = None

    def __enter__(self) -> "RustCoreMLEmbedder":
        if not self.executable.exists():
            raise RuntimeError(
                f"Rust embedder binary not found at {self.executable}. "
                "Build it with: cargo build -p embeddingkit-coreml --release "
                "--bin embeddingkit-coreml-embed"
            )
        self.process = subprocess.Popen(
            [
                str(self.executable),
                "--model-dir",
                str(self.model_dir),
                "--compute",
                self.compute,
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        if self.process is None:
            return
        if self.process.stdin is not None:
            self.process.stdin.close()
        exit_code = self.process.wait()
        self.process = None
        if exc_type is None and exit_code != 0:
            raise RuntimeError(f"Rust embedder exited with status {exit_code}")

    def embed_many(self, texts: list[str]) -> list[list[float]]:
        if not texts:
            return []
        if self.process is None or self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("Rust embedder process is not running")

        for text in texts:
            self.process.stdin.write(json.dumps({"text": text}, separators=(",", ":")) + "\n")
        self.process.stdin.flush()

        embeddings = []
        for _ in texts:
            line = self.process.stdout.readline()
            if line == "":
                exit_code = self.process.poll()
                raise RuntimeError(f"Rust embedder stopped before returning an embedding; status={exit_code}")
            response = json.loads(line)
            embedding = response["embedding"]
            if len(embedding) != DIMENSION:
                raise RuntimeError(f"embedding dimension {len(embedding)} != {DIMENSION}")
            embeddings.append(embedding)
        return embeddings


class PythonCoreMLEmbedder:
    name = "python-coreml"

    def __init__(self, *, model_dir: Path, sequence_length: int) -> None:
        import coremltools as ct
        from transformers import AutoTokenizer

        self.sequence_length = sequence_length
        self.tokenizer = AutoTokenizer.from_pretrained(model_dir / "tokenizer")
        model_package = next(model_dir.glob("*.mlpackage"))
        self.model = ct.models.MLModel(str(model_package), compute_units=ct.ComputeUnit.ALL)

    def __enter__(self) -> "PythonCoreMLEmbedder":
        return self

    def __exit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        return None

    def embed_many(self, texts: list[str]) -> list[list[float]]:
        return [self.embed_one(text) for text in texts]

    def embed_one(self, text: str) -> list[float]:
        import numpy as np

        encoded = self.tokenizer(
            text,
            padding="max_length",
            truncation=True,
            max_length=self.sequence_length,
            return_tensors="np",
        )
        token_type_ids = encoded.get("token_type_ids")
        if token_type_ids is None:
            token_type_ids = np.zeros_like(encoded["input_ids"])
        prediction = self.model.predict(
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
