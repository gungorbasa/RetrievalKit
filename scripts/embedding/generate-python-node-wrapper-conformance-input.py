#!/usr/bin/env python3
"""Generate the role-aware frozen input for Python/Node embedding qualification."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
from types import ModuleType


ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).with_name("validate-minilm-conformance.py")


def load_source() -> ModuleType:
    spec = importlib.util.spec_from_file_location("minilm_conformance_source", SOURCE)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load frozen conformance source: {SOURCE}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def document() -> dict[str, object]:
    corpus, queries, diagnostics = load_source().comparison_texts()
    items: list[dict[str, str]] = []
    for role, texts in (
        ("corpus", corpus),
        ("query", queries),
        ("diagnostic", diagnostics),
    ):
        for index, text in enumerate(texts):
            items.append(
                {
                    "id": f"{role}-{index:03d}",
                    "role": role,
                    "text": text,
                }
            )
    return {"schema_version": 1, "items": items}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT
        / "target"
        / "python-node-embedding-qualification"
        / "input.json",
    )
    args = parser.parse_args()
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(document(), indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
