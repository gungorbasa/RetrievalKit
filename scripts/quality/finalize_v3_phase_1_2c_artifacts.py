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
REQUIRED_FILES = {
    "graph-retrieval-independent-cross-check.json",
    "graph-retrieval-metrics.json",
    "graph-retrieval-paired-comparisons.json",
    "graph-retrieval-persistence-validation.json",
    "graph-retrieval-rust-results.json",
    "ir-measures-cross-check.json",
    "qualification.json",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_index(artifacts: Path) -> dict[str, object]:
    missing = sorted(name for name in REQUIRED_FILES if not (artifacts / name).is_file())
    if missing:
        raise ValidationError(f"cannot hash incomplete Phase 1.2c artifacts; missing {missing}")
    files = []
    for path in sorted(artifacts.rglob("*")):
        if not path.is_file() or path.name == INDEX_NAME:
            continue
        data = path.read_bytes()
        files.append(
            {
                "bytes": len(data),
                "path": path.relative_to(artifacts).as_posix(),
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--check-only", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        artifacts = args.artifacts.resolve()
        index = build_index(artifacts) if args.check_only else write_index(artifacts)
        print(json.dumps(index, indent=2, sort_keys=True))
        return 0
    except (OSError, ValidationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
