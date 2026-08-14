#!/usr/bin/env python3
"""Export the existing frozen 48-document/42-query provider fixture."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "scripts/embedding/validate-minilm-conformance.py"


def load_comparison_texts():
    spec = importlib.util.spec_from_file_location("minilm_conformance", SOURCE)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SOURCE}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.comparison_texts()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    corpus, queries, diagnostics = load_comparison_texts()
    if (len(corpus), len(queries), len(diagnostics)) != (48, 42, 4):
        raise ValueError("provider conformance population drifted")
    with (output / "corpus.jsonl").open("w", encoding="utf-8") as handle:
        for index, text in enumerate(corpus):
            record = {
                "chunks": [{"chunk_key": "body", "text": text}],
                "metadata": {"fixture": "provider-conformance-v1"},
                "record_id": f"provider-record-{index:03d}",
                "text": text,
            }
            handle.write(json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
    document = {
        "diagnostics": diagnostics,
        "queries": [
            {"id": f"provider-query-{index:03d}", "text": text}
            for index, text in enumerate(queries)
        ],
        "schema_version": 1,
    }
    (output / "queries.json").write_text(
        json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"corpus_count": 48, "query_count": 42, "diagnostic_count": 4}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
