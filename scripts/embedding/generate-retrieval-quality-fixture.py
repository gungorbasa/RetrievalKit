#!/usr/bin/env python3
"""Generate checked-in MiniLM embeddings for the retrieval-quality fixture."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import coremltools as ct
import numpy as np
from transformers import AutoTokenizer


ROOT = Path(__file__).resolve().parents[2]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source",
        type=Path,
        default=ROOT / "benchmarks/retrieval-quality/v1/source.json",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "benchmarks/retrieval-quality/v1/fixture.json",
    )
    parser.add_argument(
        "--model-dir",
        type=Path,
        default=ROOT / "target/embedding-models/all-MiniLM-L6-v2",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    source = json.loads(args.source.read_text())
    model_info = source["model"]
    tokenizer = AutoTokenizer.from_pretrained(
        args.model_dir / "tokenizer", local_files_only=True
    )
    model_packages = sorted(args.model_dir.glob("*.mlpackage"))
    if len(model_packages) != 1:
        raise SystemExit(
            f"expected one .mlpackage in {args.model_dir}, found {len(model_packages)}"
        )
    model = ct.models.MLModel(str(model_packages[0]), compute_units=ct.ComputeUnit.ALL)
    sequence_length = int(model_info["sequence_length"])

    fixture = {key: value for key, value in source.items() if key != "distractor_count"}
    documents = source["documents"] + generate_distractors(
        int(source.get("distractor_count", 0))
    )
    fixture["documents"] = [
        with_embedding(item, item["text"], model, tokenizer, sequence_length)
        for item in documents
    ]
    fixture["replacements"] = [
        {
            **item,
            "initial_embedding": embed(
                item["initial_text"], model, tokenizer, sequence_length
            ),
            "replacement_embedding": embed(
                item["replacement_text"], model, tokenizer, sequence_length
            ),
        }
        for item in source["replacements"]
    ]
    fixture["queries"] = [
        with_embedding(item, item["text"], model, tokenizer, sequence_length)
        for item in source["queries"]
    ]
    fixture["embedding_provenance"] = {
        "generator": "scripts/embedding/generate-retrieval-quality-fixture.py",
        "model": model_info["id"],
        "sequence_length": sequence_length,
        "normalized": True,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")
    print(f"Wrote {args.output}")


def generate_distractors(count: int) -> list[dict[str, Any]]:
    topics = [
        "warehouse inventory",
        "conference catering",
        "museum membership",
        "bicycle maintenance",
        "language study",
        "home insurance",
        "podcast editing",
        "volunteer scheduling",
        "cloud cost allocation",
        "accessibility testing",
        "customer onboarding",
        "office furniture",
        "market research",
        "video color correction",
        "community events",
        "data export",
    ]
    actions = [
        "review the monthly checklist and assign an owner",
        "compare the current report with the previous quarter",
        "collect feedback before approving the next revision",
        "verify the schedule and notify everyone affected",
        "document open questions and decide them next week",
        "archive completed work after the final audit",
        "measure the result and record follow-up actions",
        "prepare a short summary for the next planning meeting",
    ]
    kinds = ["note", "checklist", "meeting", "plan"]
    departments = ["operations", "community", "marketing", "facilities"]
    return [
        {
            "id": f"distractor-{offset:03d}",
            "text": (
                f"{topics[offset % len(topics)].title()} {kinds[offset % len(kinds)]} "
                f"number {offset + 1}: {actions[(offset // len(topics)) % len(actions)]}. "
                f"The owner is team {chr(65 + offset % 8)} and the review month is "
                f"{1 + offset % 12}."
            ),
            "metadata": {
                "kind": kinds[offset % len(kinds)],
                "department": departments[offset % len(departments)],
                "fixture_role": "distractor",
            },
        }
        for offset in range(count)
    ]


def with_embedding(
    item: dict[str, Any],
    text: str,
    model: ct.models.MLModel,
    tokenizer: Any,
    sequence_length: int,
) -> dict[str, Any]:
    return {**item, "embedding": embed(text, model, tokenizer, sequence_length)}


def embed(
    text: str,
    model: ct.models.MLModel,
    tokenizer: Any,
    sequence_length: int,
) -> list[float]:
    encoded = tokenizer(
        text,
        max_length=sequence_length,
        padding="max_length",
        truncation=True,
        return_tensors="np",
    )
    inputs = {
        "input_ids": encoded["input_ids"].astype(np.int32),
        "attention_mask": encoded["attention_mask"].astype(np.int32),
    }
    if "token_type_ids" in encoded:
        inputs["token_type_ids"] = encoded["token_type_ids"].astype(np.int32)
    prediction = model.predict(inputs)
    vector = np.asarray(prediction["embedding"], dtype=np.float32).reshape(-1)
    norm = float(np.linalg.norm(vector))
    if norm > 0:
        vector /= norm
    return [round(float(value), 8) for value in vector]


if __name__ == "__main__":
    main()
