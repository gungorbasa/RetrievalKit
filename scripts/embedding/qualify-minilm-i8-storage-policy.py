#!/usr/bin/env python3
"""Qualify FP32 embedding parity with RetrievalKit's signed-I8 storage policy."""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import statistics
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping, Sequence


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ARTIFACTS = ROOT / "target" / "embedding-models" / "retrievalkit-minilm"
CONFORMANCE_SCRIPT = ROOT / "scripts" / "embedding" / "validate-minilm-conformance.py"
TOP_K = 10
I8_MAX = 127
GATES = {
    "median_cosine": 0.9999,
    "mean_top10_overlap": 0.99,
    "exact_top10_fraction": 0.90,
    "minimum_top10_overlap": 0.90,
}


@dataclass(frozen=True)
class I8Vectors:
    values: Any
    scales: Any


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Compare ONNX FP32 with direct Core ML FP32, then qualify both "
            "cross-provider database directions using RetrievalKit's exact "
            "per-vector symmetric signed-I8 scoring policy."
        )
    )
    parser.add_argument("--artifacts", type=Path, default=DEFAULT_ARTIFACTS)
    parser.add_argument(
        "--output",
        type=Path,
        help="optional JSON report path; generated reports must remain under target",
    )
    parser.add_argument(
        "--coreml-compute",
        choices=("cpu-only", "all"),
        default="cpu-only",
        help="Core ML compute-unit policy to qualify; default cpu-only",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    artifacts = args.artifacts.resolve()
    output = require_target_path(args.output) if args.output is not None else None

    import coremltools as ct
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer

    conformance = load_conformance_helpers()
    tokenizer = Tokenizer.from_file(str(artifacts / "tokenizer" / "tokenizer.json"))
    tokenizer.enable_truncation(max_length=conformance.MAX_TOKENS)
    corpus, queries, diagnostics = conformance.comparison_texts()
    all_texts = corpus + queries + diagnostics

    onnx_model = artifacts / "onnx" / "all-MiniLM-L6-v2-fp32.onnx"
    onnx_vectors = conformance.embed_onnx(
        ort.InferenceSession(str(onnx_model), providers=["CPUExecutionProvider"]),
        tokenizer,
        all_texts,
        np,
    )

    # coremltools may rewrite Manifest.json formatting while loading a package.
    # Qualify a target-local copy so the locked artifact remains byte-identical.
    coreml_package = artifacts / "coreml" / "all-MiniLM-L6-v2-fp32.mlpackage"
    target = ROOT / "target"
    target.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="minilm-coreml-fp32-", dir=target) as temporary:
        copied_package = Path(temporary) / coreml_package.name
        shutil.copytree(coreml_package, copied_package)
        coreml_model = ct.models.MLModel(
            str(copied_package),
            compute_units=(
                ct.ComputeUnit.CPU_ONLY
                if args.coreml_compute == "cpu-only"
                else ct.ComputeUnit.ALL
            ),
        )
        coreml_vectors = conformance.embed_coreml(
            coreml_model,
            tokenizer,
            all_texts,
            np,
        )

    report = analyze_policy(
        onnx_vectors=onnx_vectors,
        coreml_vectors=coreml_vectors,
        corpus_count=len(corpus),
        query_count=len(queries),
        np=np,
    )
    report.update(
        {
            "schema": "retrievalkit-minilm-i8-storage-qualification-v1",
            "corpus_count": len(corpus),
            "query_count": len(queries),
            "diagnostic_count": len(diagnostics),
            "top_k": TOP_K,
            "coreml_compute_units": args.coreml_compute,
            "i8_contract": {
                "encoding": "per-vector symmetric signed I8",
                "zero_point": 0,
                "scale": "f32 max_abs / 127",
                "rounding": "Rust f32::round (half away from zero)",
                "scoring": "i8 dot * query f32 scale * database f32 scale",
            },
        }
    )
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if output is not None:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered)
    print(rendered, end="")
    return 0 if report["passed"] else 1


def require_target_path(path: Path) -> Path:
    resolved = path.expanduser().resolve()
    target = (ROOT / "target").resolve()
    if resolved == target or target not in resolved.parents:
        raise SystemExit(f"output must be a file inside {target}: {resolved}")
    return resolved


def load_conformance_helpers() -> Any:
    spec = importlib.util.spec_from_file_location(
        "retrievalkit_minilm_conformance",
        CONFORMANCE_SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load conformance helpers: {CONFORMANCE_SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def rust_round_f32(values: Any, np: Any) -> Any:
    """Match Rust f32::round: nearest integer, with half ties away from zero."""
    values = np.asarray(values, dtype=np.float32)
    magnitude = np.floor(
        np.add(np.abs(values), np.float32(0.5), dtype=np.float32)
    )
    return np.copysign(magnitude, values).astype(np.float32, copy=False)


def encode_i8_vectors(vectors: Any, np: Any) -> I8Vectors:
    """Match retrievalkit-core's encode_i8_scalar_quantized row by row."""
    vectors = require_f32_matrix(vectors, "vectors", np)
    max_abs = np.max(np.abs(vectors), axis=1).astype(np.float32, copy=False)
    scales = np.divide(max_abs, np.float32(I8_MAX), dtype=np.float32)
    inverse_scales = np.zeros_like(scales, dtype=np.float32)
    nonzero = max_abs != np.float32(0.0)
    np.divide(
        np.float32(1.0),
        scales,
        out=inverse_scales,
        where=nonzero,
    )
    scaled = np.multiply(
        vectors,
        inverse_scales[:, None],
        dtype=np.float32,
    )
    rounded = rust_round_f32(scaled, np)
    values = np.clip(rounded, -128, I8_MAX).astype(np.int8)
    values[~nonzero] = 0
    return I8Vectors(values=values, scales=scales)


def score_i8(database: I8Vectors, queries: I8Vectors, np: Any) -> Any:
    if database.values.ndim != 2 or queries.values.ndim != 2:
        raise ValueError("I8 database and query values must be matrices")
    if database.values.shape[1] != queries.values.shape[1]:
        raise ValueError("I8 database and query dimensions do not match")
    integer_scores = (
        queries.values.astype(np.int32)
        @ database.values.astype(np.int32).T
    )
    scores = integer_scores.astype(np.float32)
    scores = np.multiply(scores, queries.scales[:, None], dtype=np.float32)
    return np.multiply(scores, database.scales[None, :], dtype=np.float32)


def analyze_policy(
    *,
    onnx_vectors: Any,
    coreml_vectors: Any,
    corpus_count: int,
    query_count: int,
    np: Any,
    top_k: int = TOP_K,
    gates: Mapping[str, float] = GATES,
) -> dict[str, Any]:
    onnx = require_f32_matrix(onnx_vectors, "ONNX vectors", np)
    coreml = require_f32_matrix(coreml_vectors, "Core ML vectors", np)
    if onnx.shape != coreml.shape:
        raise ValueError(
            f"provider vector shapes differ: ONNX {onnx.shape}, Core ML {coreml.shape}"
        )
    if corpus_count < top_k:
        raise ValueError(f"corpus count {corpus_count} is smaller than top_k {top_k}")
    if query_count <= 0 or corpus_count + query_count > onnx.shape[0]:
        raise ValueError("corpus/query counts do not fit the provider vectors")
    require_gate_keys(gates)

    onnx_corpus = onnx[:corpus_count]
    coreml_corpus = coreml[:corpus_count]
    query_slice = slice(corpus_count, corpus_count + query_count)
    onnx_queries = onnx[query_slice]
    coreml_queries = coreml[query_slice]
    median_cosine = corresponding_median_cosine(onnx, coreml, np)

    onnx_reference_scores = onnx_queries @ onnx_corpus.T
    coreml_reference_scores = coreml_queries @ coreml_corpus.T
    direct_coreml_scores = coreml_queries @ coreml_corpus.T

    encoded_onnx_corpus = encode_i8_vectors(onnx_corpus, np)
    encoded_coreml_corpus = encode_i8_vectors(coreml_corpus, np)
    encoded_onnx_queries = encode_i8_vectors(onnx_queries, np)
    encoded_coreml_queries = encode_i8_vectors(coreml_queries, np)

    direct = qualify_scores(
        median_cosine=median_cosine,
        reference_scores=onnx_reference_scores,
        candidate_scores=direct_coreml_scores,
        top_k=top_k,
        gates=gates,
        np=np,
    )
    direct.update(
        {
            "name": "direct_fp32_onnx_vs_coreml",
            "reference": {
                "database": "onnx_fp32",
                "queries": "onnx_fp32",
                "scoring": "f32_dot",
            },
            "candidate": {
                "database": "coreml_fp32",
                "queries": "coreml_fp32",
                "scoring": "f32_dot",
            },
        }
    )

    onnx_database = qualify_scores(
        median_cosine=median_cosine,
        reference_scores=onnx_reference_scores,
        candidate_scores=score_i8(
            encoded_onnx_corpus,
            encoded_coreml_queries,
            np,
        ),
        top_k=top_k,
        gates=gates,
        np=np,
    )
    onnx_database.update(
        {
            "name": "onnx_database_coreml_queries",
            "reference": {
                "database": "onnx_fp32",
                "queries": "onnx_fp32",
                "scoring": "f32_dot",
            },
            "candidate": {
                "database": "onnx_fp32_encoded_retrievalkit_i8",
                "queries": "coreml_fp32_encoded_retrievalkit_i8",
                "scoring": "i8_dot_rescaled_f32",
            },
        }
    )

    coreml_database = qualify_scores(
        median_cosine=median_cosine,
        reference_scores=coreml_reference_scores,
        candidate_scores=score_i8(
            encoded_coreml_corpus,
            encoded_onnx_queries,
            np,
        ),
        top_k=top_k,
        gates=gates,
        np=np,
    )
    coreml_database.update(
        {
            "name": "coreml_database_onnx_queries",
            "reference": {
                "database": "coreml_fp32",
                "queries": "coreml_fp32",
                "scoring": "f32_dot",
            },
            "candidate": {
                "database": "coreml_fp32_encoded_retrievalkit_i8",
                "queries": "onnx_fp32_encoded_retrievalkit_i8",
                "scoring": "i8_dot_rescaled_f32",
            },
        }
    )

    directions = [onnx_database, coreml_database]
    return {
        "gates": dict(gates),
        "direct_fp32_comparison": direct,
        "database_directions": directions,
        "passed": direct["passed"] and all(item["passed"] for item in directions),
    }


def corresponding_median_cosine(left: Any, right: Any, np: Any) -> float:
    left_norms = np.linalg.norm(left, axis=1)
    right_norms = np.linalg.norm(right, axis=1)
    if np.any(left_norms == 0) or np.any(right_norms == 0):
        raise ValueError("provider vectors must be nonzero")
    cosines = np.sum(left * right, axis=1) / (left_norms * right_norms)
    return float(statistics.median(float(value) for value in cosines))


def qualify_scores(
    *,
    median_cosine: float,
    reference_scores: Any,
    candidate_scores: Any,
    top_k: int,
    gates: Mapping[str, float],
    np: Any,
) -> dict[str, Any]:
    ranking = ranking_overlap_metrics(
        reference_scores,
        candidate_scores,
        top_k=top_k,
        np=np,
    )
    metrics = {
        "median_cosine": float(median_cosine),
        **ranking,
    }
    gate_results = {
        name: metrics[name] >= threshold for name, threshold in gates.items()
    }
    return {
        **metrics,
        "gate_results": gate_results,
        "passed": all(gate_results.values()),
    }


def ranking_overlap_metrics(
    reference_scores: Any,
    candidate_scores: Any,
    *,
    top_k: int,
    np: Any,
) -> dict[str, float]:
    reference = require_f32_matrix(reference_scores, "reference scores", np)
    candidate = require_f32_matrix(candidate_scores, "candidate scores", np)
    if reference.shape != candidate.shape:
        raise ValueError(
            f"ranking score shapes differ: {reference.shape} vs {candidate.shape}"
        )
    if top_k <= 0 or reference.shape[1] < top_k:
        raise ValueError("top_k must fit the score matrix")

    overlaps: list[float] = []
    exact = 0
    for expected_scores, actual_scores in zip(reference, candidate, strict=True):
        expected = set(stable_top_k(expected_scores, top_k))
        actual = set(stable_top_k(actual_scores, top_k))
        overlap = len(expected & actual) / top_k
        overlaps.append(overlap)
        exact += expected == actual
    return {
        "mean_top10_overlap": float(statistics.mean(overlaps)),
        "exact_top10_fraction": exact / len(overlaps),
        "minimum_top10_overlap": min(overlaps),
    }


def stable_top_k(scores: Any, count: int) -> list[int]:
    return sorted(
        range(len(scores)),
        key=lambda index: (-float(scores[index]), index),
    )[:count]


def require_f32_matrix(values: Any, label: str, np: Any) -> Any:
    matrix = np.asarray(values, dtype=np.float32)
    if matrix.ndim != 2:
        raise ValueError(f"{label} must be a matrix, got {matrix.shape}")
    if not np.isfinite(matrix).all():
        raise ValueError(f"{label} contain non-finite values")
    return matrix


def require_gate_keys(gates: Mapping[str, float]) -> None:
    if set(gates) != set(GATES):
        raise ValueError(f"gates must contain exactly {sorted(GATES)}")


if __name__ == "__main__":
    raise SystemExit(main())
