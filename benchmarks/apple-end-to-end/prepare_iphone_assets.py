#!/usr/bin/env python3
"""Assemble closed iPhone asset roots from verified Mac preparation outputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROFILES = {
    "fp32": {
        "id": "coreml-fp32-production-v1",
        "model": "coreml-fp32-production-v1/extracted/coreml/all-MiniLM-L6-v2-fp32.mlpackage",
        "tokenizer": "coreml-fp32-production-v1/extracted/tokenizer/tokenizer.json",
    },
    "q8": {
        "id": "coreml-weight-only-q8-experimental-v1",
        "model": "coreml-weight-only-q8-experimental-v1/coreml/all-MiniLM-L6-v2-q8.mlpackage",
        "tokenizer": "coreml-weight-only-q8-experimental-v1/tokenizer/tokenizer.json",
    },
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tree_stats(root: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    total = 0
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        relative = path.relative_to(root).as_posix()
        size = path.stat().st_size
        digest.update(f"{relative}\0{size}\0{sha256_file(path)}\n".encode())
        total += size
    return total, digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--assets", type=Path, default=ROOT / "target/apple-end-to-end")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assets = args.assets.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    manifest: dict[str, object] = {"schema_version": 1, "roots": {}}
    for profile, profile_info in PROFILES.items():
        for size in ("10k", "50k", "100k"):
            root = output / f"{profile}-{size}"
            root.mkdir()
            shutil.copytree(
                assets / "models-v1" / profile_info["model"],
                root / "model.mlpackage",
            )
            shutil.copy2(
                assets / "models-v1" / profile_info["tokenizer"],
                root / "tokenizer.json",
            )
            shutil.copy2(assets / f"source-{size}-a/queries.json", root / "queries.json")
            shutil.copytree(
                assets / "indexes" / profile_info["id"] / size,
                root / "index",
            )
            byte_count, digest = tree_stats(root)
            manifest["roots"][root.name] = {"bytes": byte_count, "sha256": digest}
    (output / "iphone-assets-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(manifest, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
