"""Opt-in conformance and latency qualification for retrievalkit-embedding."""

from __future__ import annotations

import argparse
import json
import math
import time
from pathlib import Path
from typing import Any

from retrievalkit_embedding import BUILD_MODE, ModelInfo, OnnxEmbedder

WARMUPS = 50
MEASURED = 750
BENCHMARK_TEXT = " ".join(f"token{index}" for index in range(32))


def model_metadata(info: ModelInfo) -> dict[str, Any]:
    return {
        "identifier": info.identifier,
        "revision": info.revision,
        "profile": info.profile,
        "dimension": info.dimension,
        "max_input_tokens": info.max_input_tokens,
        "produces_normalized_embeddings": info.produces_normalized_embeddings,
    }


def conformance_model(info: ModelInfo) -> dict[str, Any]:
    return {
        "identifier": info.identifier,
        "revision": info.revision,
        "profile": "fp32",
        "dtype": "float32",
        "dimension": info.dimension,
        "max_input_tokens": info.max_input_tokens,
        "normalized": True,
    }


def conformance_inputs(raw: Any) -> tuple[list[str], list[str]]:
    if isinstance(raw, list) and raw and all(isinstance(text, str) for text in raw):
        return [str(index) for index in range(len(raw))], raw
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise SystemExit(
            "input must be a non-empty JSON string array or schema_version 1 object"
        )
    items = raw.get("items")
    if not isinstance(items, list) or not items:
        raise SystemExit("versioned input items must be a non-empty array")
    identifiers: list[str] = []
    texts: list[str] = []
    for item in items:
        if not isinstance(item, dict):
            raise SystemExit("each versioned input item must be an object")
        identifier = item.get("id")
        text = item.get("text")
        role = item.get("role")
        if (
            not isinstance(identifier, str)
            or not isinstance(text, str)
            or not isinstance(role, str)
        ):
            raise SystemExit("each item requires string id, text, and role fields")
        identifiers.append(identifier)
        texts.append(text)
    if len(set(identifiers)) != len(identifiers):
        raise SystemExit("versioned input item IDs must be unique")
    return identifiers, texts


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    rank = max(0, math.ceil(fraction * len(ordered)) - 1)
    return ordered[rank]


def load_embedder(args: argparse.Namespace) -> tuple[OnnxEmbedder, float]:
    started = time.perf_counter_ns()
    embedder = OnnxEmbedder.load(
        cache_directory=args.cache_directory,
        runtime_library_path=args.runtime_library_path,
        local_only=args.local_only,
    )
    load_ms = (time.perf_counter_ns() - started) / 1_000_000
    return embedder, load_ms


def conformance(args: argparse.Namespace) -> None:
    raw = json.loads(args.input.read_text(encoding="utf-8"))
    identifiers, texts = conformance_inputs(raw)
    embedder, _load_ms = load_embedder(args)
    vectors = embedder.embed_batch(texts)
    payload = {
        "schema_version": 1,
        "model": conformance_model(embedder.model_info),
        "items": [
            {"id": identifier, "embedding": vector}
            for identifier, vector in zip(identifiers, vectors, strict=True)
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def benchmark(args: argparse.Namespace) -> None:
    embedder, load_ms = load_embedder(args)
    started = time.perf_counter_ns()
    first = embedder.embed(BENCHMARK_TEXT)
    first_ms = (time.perf_counter_ns() - started) / 1_000_000
    if len(first) != 384:
        raise RuntimeError("first inference violated the 384-value contract")

    for _ in range(WARMUPS):
        embedder.embed(BENCHMARK_TEXT)

    measured_ms: list[float] = []
    for _ in range(MEASURED):
        started = time.perf_counter_ns()
        embedder.embed(BENCHMARK_TEXT)
        measured_ms.append((time.perf_counter_ns() - started) / 1_000_000)

    payload = {
        "provider": "python-onnx-fp32",
        "build_mode": BUILD_MODE,
        "model": model_metadata(embedder.model_info),
        "token_length": 32,
        "warmups": WARMUPS,
        "measured": MEASURED,
        "load_ms": load_ms,
        "first_inference_ms": first_ms,
        "warm_embedding_ms": {
            "p50": percentile(measured_ms, 0.50),
            "p95": percentile(measured_ms, 0.95),
            "p99": percentile(measured_ms, 0.99),
            "min": min(measured_ms),
            "max": max(measured_ms),
        },
    }
    rendered = json.dumps(payload, ensure_ascii=False, indent=2) + "\n"
    if args.output is None:
        print(rendered, end="")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")


def add_runtime_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--cache-directory", type=Path)
    parser.add_argument("--runtime-library-path", type=Path)
    parser.add_argument("--local-only", action="store_true")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)

    conformance_parser = commands.add_parser("conformance")
    conformance_parser.add_argument("--input", type=Path, required=True)
    conformance_parser.add_argument("--output", type=Path, required=True)
    add_runtime_options(conformance_parser)
    conformance_parser.set_defaults(run=conformance)

    benchmark_parser = commands.add_parser("benchmark")
    benchmark_parser.add_argument("--output", type=Path)
    add_runtime_options(benchmark_parser)
    benchmark_parser.set_defaults(run=benchmark)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
