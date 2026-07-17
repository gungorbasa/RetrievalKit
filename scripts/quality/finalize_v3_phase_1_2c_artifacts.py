#!/usr/bin/env python3
"""Write the canonical non-publication hash index for Phase 1.2c artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

if __package__:
    from .validate_v3_phase_1_2a import ValidationError
    from .validate_v3_phase_1_2b import canonical_bytes
else:
    from validate_v3_phase_1_2a import ValidationError
    from validate_v3_phase_1_2b import canonical_bytes


INDEX_NAME = "qualification-hash-index.json"
INDEX_SCHEMA = "phase-1.2c-canonical-hash-index-v1"
EXPECTED_PREINDEX_FILE_COUNT = 56
RUN_IDS = (
    "v3-a-whole-semantic-f32-na-cfg-984e4c3bf991",
    "v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53",
    "v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0",
    "v3-e-graph-semantic-f32-explicit-cfg-d2855327ee28",
    "v3-e-graph-semantic-f32-team-cfg-9d005ed09abd",
    "v3-e-graph-semantic-f32-topic-cfg-dd783bc155d4",
    "v3-f-graph-semantic-i8-explicit-cfg-9199f34e596a",
    "v3-f-graph-semantic-i8-team-cfg-c9fe28bfe8a2",
    "v3-f-graph-semantic-i8-topic-cfg-748772f67f91",
    "v3-g-graph-weighted-i8-explicit-cfg-f5f6dfcae573",
    "v3-g-graph-weighted-i8-team-cfg-0562c721d6e7",
    "v3-g-graph-weighted-i8-topic-cfg-36c6887ab88d",
)
SELECTION_RUN_IDS = (
    "v3-d-selection-none-none-explicit-cfg-13feb2a18ac3",
    "v3-d-selection-none-none-team-cfg-7278e2315c8f",
    "v3-d-selection-none-none-topic-cfg-bf6bed5c72e7",
    *RUN_IDS[3:],
)
TOP_LEVEL_FILES = {
    "graph-generation-fingerprint.json",
    "graph-metrics.json",
    "graph-persistence-validation.json",
    "graph-projection-identities.jsonl",
    "graph-retrieval-generation-fingerprints.json",
    "graph-retrieval-independent-cross-check.json",
    "graph-retrieval-metrics.json",
    "graph-retrieval-paired-comparisons.json",
    "graph-retrieval-persistence-validation.json",
    "graph-retrieval-projection-identities.jsonl",
    "graph-retrieval-rust-results.json",
    "graph-retrieval-selection-path-equality.json",
    "graph-rust-results.json",
    "ir-measures-cross-check.json",
    "metrics.json",
    "qrels.tsv",
    "qualification.json",
    "rust-results.json",
    "seed-resolution-diagnostics.json",
    "timing-samples.jsonl",
}
EXPECTED_FILES = frozenset(
    TOP_LEVEL_FILES
    | {f"runs/{run_id}.trec" for run_id in RUN_IDS}
    | {
        f"{directory}/{run_id}.jsonl"
        for directory in ("graph-selections", "graph-paths")
        for run_id in SELECTION_RUN_IDS
    }
)
EXPECTED_DIRECTORIES = frozenset({"graph-paths", "graph-selections", "runs"})

if len(EXPECTED_FILES) != EXPECTED_PREINDEX_FILE_COUNT:
    raise RuntimeError("Phase 1.2c expected inventory constant is inconsistent")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def validate_inventory(artifacts: Path) -> None:
    if not artifacts.is_dir():
        raise ValidationError(f"Phase 1.2c artifact root is not a directory: '{artifacts}'")
    symlinks = sorted(
        path.relative_to(artifacts).as_posix()
        for path in artifacts.rglob("*")
        if path.is_symlink()
    )
    if symlinks:
        raise ValidationError(f"Phase 1.2c inventory contains symlinks {symlinks}")
    actual_files = {
        path.relative_to(artifacts).as_posix()
        for path in artifacts.rglob("*")
        if path.is_file() and path.relative_to(artifacts).as_posix() != INDEX_NAME
    }
    actual_directories = {
        path.relative_to(artifacts).as_posix()
        for path in artifacts.rglob("*")
        if path.is_dir()
    }
    missing = sorted(EXPECTED_FILES - actual_files)
    if missing:
        raise ValidationError(f"incomplete Phase 1.2c inventory; missing {missing}")
    unexpected = sorted(actual_files - EXPECTED_FILES)
    if unexpected:
        raise ValidationError(f"unexpected Phase 1.2c artifact files {unexpected}")
    unexpected_directories = sorted(actual_directories - EXPECTED_DIRECTORIES)
    if unexpected_directories:
        raise ValidationError(
            f"unexpected Phase 1.2c artifact directories {unexpected_directories}"
        )
    if len(actual_files) != EXPECTED_PREINDEX_FILE_COUNT:
        raise ValidationError(
            "Phase 1.2c pre-index file count mismatch: "
            f"expected {EXPECTED_PREINDEX_FILE_COUNT}, actual {len(actual_files)}"
        )


def build_index(artifacts: Path) -> dict[str, object]:
    validate_inventory(artifacts)
    files = []
    for relative in sorted(EXPECTED_FILES):
        path = artifacts / relative
        data = path.read_bytes()
        files.append(
            {
                "bytes": len(data),
                "path": relative,
                "sha256": sha256(data),
            }
        )
    return {
        "artifact_schema": INDEX_SCHEMA,
        "artifact_set_sha256": sha256(canonical_bytes(files)),
        "file_count": len(files),
        "files": files,
        "partial": True,
        "publication_ready": False,
        "status": "qualification_only_no_final_manifest",
    }


def write_index(artifacts: Path) -> dict[str, object]:
    path = artifacts / INDEX_NAME
    if path.exists():
        raise ValidationError(f"refusing to overwrite canonical hash index '{path}'")
    index = build_index(artifacts)
    path.write_bytes(canonical_bytes(index) + b"\n")
    return index


def check_index(artifacts: Path) -> dict[str, object]:
    index = build_index(artifacts)
    path = artifacts / INDEX_NAME
    if path.exists():
        expected = canonical_bytes(index) + b"\n"
        actual = path.read_bytes()
        if actual != expected:
            raise ValidationError(
                f"stored canonical hash index '{path}' does not match the fresh inventory"
            )
    return index


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--check-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        artifacts = args.artifacts.resolve()
        index = check_index(artifacts) if args.check_only else write_index(artifacts)
        print(json.dumps(index, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
