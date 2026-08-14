#!/usr/bin/env python3
"""Run the frozen Mac Apple end-to-end matrix sequentially."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BENCHMARK = ROOT / "benchmarks/apple-end-to-end"
DEFAULT_BINARY = (
    ROOT / "wrappers/swift/RetrievalKitAppleE2EBench/.build/release/retrievalkit-apple-e2e"
)
PROFILES = {
    "coreml-fp32-production-v1": {
        "classification": "production_control",
        "model": "coreml-fp32-production-v1/compiled/all-MiniLM-L6-v2-fp32.mlmodelc",
        "tokenizer": "coreml-fp32-production-v1/extracted/tokenizer/tokenizer.json",
        "slug": "fp32",
    },
    "coreml-weight-only-q8-experimental-v1": {
        "classification": "experimental_candidate",
        "model": "coreml-weight-only-q8-experimental-v1/compiled/all-MiniLM-L6-v2-q8.mlmodelc",
        "tokenizer": "coreml-weight-only-q8-experimental-v1/tokenizer/tokenizer.json",
        "slug": "q8",
    },
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--attempt-id", required=True)
    parser.add_argument("--retrievalkit-revision", required=True)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--assets", type=Path, default=ROOT / "target/apple-end-to-end")
    args = parser.parse_args()
    assets = args.assets.resolve()
    output_root = assets / "results/mac" / args.attempt_id
    output_root.mkdir(parents=True, exist_ok=False)
    quality = json.loads((assets / "quality/q8-vs-fp32-provider-v1.json").read_text())
    if quality.get("passed") is not True:
        raise SystemExit("Q8 provider prerequisite has not passed")
    descriptor = json.loads((BENCHMARK / "workloads-v1.json").read_text())
    workloads = descriptor["workloads"]
    model_root = assets / "models-v1"

    reports: list[Path] = []
    for workload in workloads:
        size = f"{workload['active_chunks'] // 1000}k"
        queries = assets / f"source-{size}-a/queries.json"
        for profile_id, profile in PROFILES.items():
            for mode in ("vector", "weighted_hybrid"):
                for session in range(1, 4):
                    output = output_root / profile["slug"] / size / mode / f"session-{session}.json"
                    session_id = f"mac-{profile['slug']}-{size}-{mode}-{session}"
                    command = [
                        str(args.binary.resolve()), "run",
                        "--queries", str(queries),
                        "--index", str(assets / "indexes" / profile_id / size),
                        "--model", str(model_root / profile["model"]),
                        "--tokenizer", str(model_root / profile["tokenizer"]),
                        "--output", str(output),
                        "--workload-id", workload["id"],
                        "--workload-classification", workload["classification"],
                        "--profile-id", profile_id,
                        "--profile-classification", profile["classification"],
                        "--session-id", session_id,
                        "--mode", mode,
                        "--retrievalkit-revision", args.retrievalkit_revision,
                    ]
                    print(f"running {session_id}", flush=True)
                    subprocess.run(command, check=True)
                    reports.append(output)

    validation = [
        "python3", str(BENCHMARK / "validate_results.py"),
        "--queries", str(assets / "source-10k-a/queries.json"),
        "--require-complete-sessions",
        "--q8-quality", str(assets / "quality/q8-vs-fp32-provider-v1.json"),
        *map(str, reports),
    ]
    subprocess.run(validation, check=True)
    print(f"complete: {output_root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
