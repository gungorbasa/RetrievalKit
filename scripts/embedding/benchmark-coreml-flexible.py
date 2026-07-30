#!/usr/bin/env python3
"""Compare fixed-256 and flexible-sequence FP16 Core ML exports."""

from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
FIXED = (
    ROOT
    / "target"
    / "embedding-models"
    / "retrievalkit-minilm"
    / "coreml"
    / "all-MiniLM-L6-v2-fp16.mlpackage"
)
FLEXIBLE = (
    ROOT
    / "target"
    / "embedding-models"
    / "retrievalkit-minilm-flexible-candidate-check"
    / "coreml"
    / "all-MiniLM-L6-v2-flexible-fp16.mlpackage"
)
TOKENIZER = (
    ROOT
    / "target"
    / "embedding-models"
    / "retrievalkit-minilm"
    / "tokenizer"
    / "tokenizer.json"
)
TOKEN_LENGTHS = (16, 32, 64, 128, 256)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixed", type=Path, default=FIXED)
    parser.add_argument("--flexible", type=Path, default=FLEXIBLE)
    parser.add_argument("--tokenizer", type=Path, default=TOKENIZER)
    parser.add_argument("--warmups", type=int, default=50)
    parser.add_argument("--samples", type=int, default=750)
    parser.add_argument(
        "--compute-units",
        choices=("all", "cpu-only", "cpu-and-neural-engine"),
        default="all",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    import coremltools as ct
    import numpy as np
    from tokenizers import Tokenizer

    compute_units = {
        "all": ct.ComputeUnit.ALL,
        "cpu-only": ct.ComputeUnit.CPU_ONLY,
        "cpu-and-neural-engine": ct.ComputeUnit.CPU_AND_NE,
    }[args.compute_units]
    fixed = ct.models.MLModel(str(args.fixed), compute_units=compute_units)
    flexible = ct.models.MLModel(str(args.flexible), compute_units=compute_units)
    tokenizer = Tokenizer.from_file(str(args.tokenizer))
    tokenizer.enable_truncation(max_length=256)

    reports = []
    fixed_all: list[int] = []
    flexible_all: list[int] = []
    cosines: list[float] = []
    for token_length in TOKEN_LENGTHS:
        text = " ".join(["hello"] * (token_length - 2))
        fixed_input = inputs(tokenizer, text, 256, np)
        flexible_input = inputs(tokenizer, text, token_length, np)
        fixed_vector = predict(fixed, fixed_input, np)
        flexible_vector = predict(flexible, flexible_input, np)
        cosines.append(cosine(fixed_vector, flexible_vector, np))
        fixed_times = measure(
            lambda: predict(fixed, fixed_input, np),
            args.warmups,
            args.samples,
        )
        flexible_times = measure(
            lambda: predict(flexible, flexible_input, np),
            args.warmups,
            args.samples,
        )
        fixed_all.extend(fixed_times)
        flexible_all.extend(flexible_times)
        reports.append(
            {
                "token_length": token_length,
                "fixed_p95_ms": percentile_ms(fixed_times, 95),
                "fixed_p99_ms": percentile_ms(fixed_times, 99),
                "flexible_p95_ms": percentile_ms(flexible_times, 95),
                "flexible_p99_ms": percentile_ms(flexible_times, 99),
                "cosine": cosines[-1],
            }
        )

    fixed_p95 = percentile_ms(fixed_all, 95)
    fixed_p99 = percentile_ms(fixed_all, 99)
    flexible_p95 = percentile_ms(flexible_all, 95)
    flexible_p99 = percentile_ms(flexible_all, 99)
    improvement = (fixed_p95 - flexible_p95) / fixed_p95
    conformance_passed = statistics.median(cosines) >= 0.999
    p95_passed = improvement >= 0.10
    p99_passed = flexible_p99 <= fixed_p99 * 1.05
    result = {
        "compute_units": args.compute_units,
        "warmups_per_length": args.warmups,
        "samples_per_length": args.samples,
        "fixed_p95_ms": fixed_p95,
        "fixed_p99_ms": fixed_p99,
        "flexible_p95_ms": flexible_p95,
        "flexible_p99_ms": flexible_p99,
        "flexible_p95_improvement": improvement,
        "median_cosine": statistics.median(cosines),
        "conformance_passed": conformance_passed,
        "p95_gate_passed": p95_passed,
        "p99_gate_passed": p99_passed,
        "adopt_flexible": conformance_passed and p95_passed and p99_passed,
        "length_groups": reports,
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


def inputs(tokenizer: Any, text: str, sequence_length: int, np: Any) -> dict[str, Any]:
    encoding = tokenizer.encode(text)
    count = min(len(encoding.ids), sequence_length)
    padding = sequence_length - count
    return {
        "input_ids": np.asarray(
            [encoding.ids[:count] + [0] * padding], dtype=np.int32
        ),
        "attention_mask": np.asarray(
            [encoding.attention_mask[:count] + [0] * padding], dtype=np.int32
        ),
        "token_type_ids": np.asarray(
            [encoding.type_ids[:count] + [0] * padding], dtype=np.int32
        ),
    }


def predict(model: Any, model_inputs: dict[str, Any], np: Any) -> Any:
    vector = np.asarray(model.predict(model_inputs)["embedding"], dtype=np.float32).reshape(
        384
    )
    if not np.isfinite(vector).all():
        raise ValueError("Core ML returned non-finite output")
    norm = np.linalg.norm(vector)
    if abs(float(norm) - 1.0) > 5e-3:
        raise ValueError(f"Core ML returned non-normalized output: {norm}")
    return vector


def cosine(first: Any, second: Any, np: Any) -> float:
    denominator = float(np.linalg.norm(first) * np.linalg.norm(second))
    if denominator <= 0:
        raise ValueError("cannot compare a zero-length Core ML embedding")
    return float((first @ second) / denominator)


def measure(operation: Any, warmups: int, samples: int) -> list[int]:
    for _ in range(warmups):
        operation()
    durations = []
    for _ in range(samples):
        started = time.perf_counter_ns()
        operation()
        durations.append(time.perf_counter_ns() - started)
    return durations


def percentile_ms(values: list[int], percentile: int) -> float:
    ordered = sorted(values)
    index = (len(ordered) * percentile + 99) // 100 - 1
    return ordered[index] / 1_000_000


if __name__ == "__main__":
    raise SystemExit(main())
