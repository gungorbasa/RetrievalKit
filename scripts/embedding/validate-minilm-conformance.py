#!/usr/bin/env python3
"""Validate cross-profile MiniLM embedding and ranking conformance."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACTS = ROOT / "target" / "embedding-models" / "retrievalkit-minilm"
DIMENSION = 384
MAX_TOKENS = 256
COSINE_GATES = {"fp32": 0.9999, "fp16": 0.999, "q8": 0.995}
TOP10_GATES = {"fp32": 0.99, "fp16": 0.98, "q8": 0.95}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, default=DEFAULT_ARTIFACTS)
    parser.add_argument("--skip-coreml", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    artifacts = args.artifacts.resolve()
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer

    tokenizer = Tokenizer.from_file(str(artifacts / "tokenizer" / "tokenizer.json"))
    tokenizer.enable_truncation(max_length=MAX_TOKENS)
    corpus, queries, diagnostics = comparison_texts()
    all_texts = corpus + queries + diagnostics

    onnx_vectors: dict[str, Any] = {}
    for profile in ("fp32", "fp16", "q8"):
        session = ort.InferenceSession(
            str(artifacts / "onnx" / f"all-MiniLM-L6-v2-{profile}.onnx"),
            providers=["CPUExecutionProvider"],
        )
        onnx_vectors[profile] = embed_onnx(session, tokenizer, all_texts, np)

    reference = onnx_vectors["fp32"]
    reports = [
        qualify(
            runtime="onnx-cpu",
            profile=profile,
            vectors=vectors,
            reference=reference,
            corpus_count=len(corpus),
            query_count=len(queries),
            np=np,
        )
        for profile, vectors in onnx_vectors.items()
    ]

    if not args.skip_coreml and sys.platform == "darwin":
        import coremltools as ct

        for profile in ("fp32", "fp16", "q8"):
            model = ct.models.MLModel(
                str(
                    artifacts
                    / "coreml"
                    / f"all-MiniLM-L6-v2-{profile}.mlpackage"
                ),
                compute_units=ct.ComputeUnit.CPU_ONLY,
            )
            vectors = embed_coreml(model, tokenizer, all_texts, np)
            reports.append(
                qualify(
                    runtime="coreml-cpu",
                    profile=profile,
                    vectors=vectors,
                    reference=reference,
                    corpus_count=len(corpus),
                    query_count=len(queries),
                    np=np,
                )
            )

    result = {
        "dimension": DIMENSION,
        "maximum_tokens": MAX_TOKENS,
        "corpus_count": len(corpus),
        "query_count": len(queries),
        "reports": reports,
        "passed": all(report["passed"] for report in reports),
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["passed"] else 1


def comparison_texts() -> tuple[list[str], list[str], list[str]]:
    v1 = json.loads(
        (ROOT / "benchmarks" / "retrieval-quality" / "v1" / "source.json").read_text()
    )
    v2 = json.loads(
        (ROOT / "benchmarks" / "retrieval-quality" / "v2" / "source.json").read_text()
    )
    deleted = set(v1["deletions"])
    replacements = {
        item["document_id"]: item["replacement_text"] for item in v1["replacements"]
    }
    corpus = [
        replacements.get(item["id"], item["text"])
        for item in v1["documents"]
        if item["id"] not in deleted
    ]
    corpus.extend(item["text"] for item in v2["additional_documents"])
    queries = [item["text"] for item in v1["queries"]]
    queries.extend(item["text"] for item in v2["additional_queries"])
    diagnostics = [
        "hello",
        "İstanbul'da çevrimdışı arama",
        "東京でローカル検索",
        "retrieval " * 300,
    ]
    return corpus, queries, diagnostics


def tokenize(
    tokenizer: Any,
    texts: list[str],
    *,
    fixed_length: bool,
    np: Any,
) -> dict[str, Any]:
    encodings = tokenizer.encode_batch(texts)
    length = MAX_TOKENS if fixed_length else max(len(item.ids) for item in encodings)
    values: dict[str, list[list[int]]] = {
        "input_ids": [],
        "attention_mask": [],
        "token_type_ids": [],
    }
    for encoding in encodings:
        count = min(len(encoding.ids), MAX_TOKENS)
        padding = length - count
        values["input_ids"].append(encoding.ids[:count] + [0] * padding)
        values["attention_mask"].append(encoding.attention_mask[:count] + [0] * padding)
        values["token_type_ids"].append(encoding.type_ids[:count] + [0] * padding)
    dtype = np.int32 if fixed_length else np.int64
    return {name: np.asarray(rows, dtype=dtype) for name, rows in values.items()}


def embed_onnx(session: Any, tokenizer: Any, texts: list[str], np: Any) -> Any:
    batches = []
    for start in range(0, len(texts), 32):
        inputs = tokenize(tokenizer, texts[start : start + 32], fixed_length=False, np=np)
        batches.append(session.run(["embedding"], inputs)[0])
    return validated(np.concatenate(batches, axis=0), np)


def embed_coreml(model: Any, tokenizer: Any, texts: list[str], np: Any) -> Any:
    rows = []
    for text in texts:
        inputs = tokenize(tokenizer, [text], fixed_length=True, np=np)
        output = model.predict(inputs)["embedding"]
        rows.append(np.asarray(output, dtype=np.float32).reshape(DIMENSION))
    return validated(np.stack(rows), np)


def validated(vectors: Any, np: Any) -> Any:
    vectors = np.asarray(vectors, dtype=np.float32)
    if vectors.ndim != 2 or vectors.shape[1] != DIMENSION:
        raise ValueError(f"unexpected embedding shape: {vectors.shape}")
    if not np.isfinite(vectors).all():
        raise ValueError("embedding contains non-finite values")
    norms = np.linalg.norm(vectors, axis=1)
    if not np.isfinite(norms).all() or np.any(norms <= 0):
        raise ValueError(
            f"embedding norms are invalid: {norms.min()}...{norms.max()}"
        )
    normalized = vectors / norms[:, None]
    normalized_norms = np.linalg.norm(normalized, axis=1)
    if not np.allclose(normalized_norms, 1.0, atol=1e-4):
        raise ValueError("provider normalization did not produce unit vectors")
    return normalized


def qualify(
    *,
    runtime: str,
    profile: str,
    vectors: Any,
    reference: Any,
    corpus_count: int,
    query_count: int,
    np: Any,
) -> dict[str, Any]:
    cosines = np.sum(vectors * reference, axis=1)
    median_cosine = float(statistics.median(float(value) for value in cosines))
    reference_corpus = reference[:corpus_count]
    reference_queries = reference[corpus_count : corpus_count + query_count]
    candidate_corpus = vectors[:corpus_count]
    candidate_queries = vectors[corpus_count : corpus_count + query_count]
    overlaps = []
    for query_reference, query_candidate in zip(
        reference_queries, candidate_queries, strict=True
    ):
        expected = set(top_k(reference_corpus @ query_reference, 10, np))
        actual = set(top_k(candidate_corpus @ query_candidate, 10, np))
        overlaps.append(len(expected & actual) / 10)
    top10_overlap = float(statistics.mean(overlaps))
    cosine_gate = COSINE_GATES[profile]
    top10_gate = TOP10_GATES[profile]
    return {
        "runtime": runtime,
        "profile": profile,
        "median_cosine_vs_onnx_fp32": median_cosine,
        "median_cosine_gate": cosine_gate,
        "top10_overlap": top10_overlap,
        "top10_overlap_gate": top10_gate,
        "passed": median_cosine >= cosine_gate and top10_overlap >= top10_gate,
    }


def top_k(scores: Any, count: int, np: Any) -> list[int]:
    indices = np.argpartition(-scores, count - 1)[:count]
    return sorted((int(index) for index in indices), key=lambda index: (-scores[index], index))


if __name__ == "__main__":
    raise SystemExit(main())
