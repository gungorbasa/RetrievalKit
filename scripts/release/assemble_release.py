#!/usr/bin/env python3
"""Assemble deterministic release metadata around prebuilt package artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


def write_json(path: Path, value: Any) -> None:
    path.write_bytes(canonical(value))


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_date() -> str:
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    return datetime.fromtimestamp(epoch, tz=timezone.utc).isoformat().replace("+00:00", "Z")


def cargo_packages(repo: Path) -> list[dict[str, Any]]:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    return [
        {
            "SPDXID": f"SPDXRef-Cargo-{package['name']}-{package['version']}",
            "name": package["name"],
            "versionInfo": package["version"],
            "downloadLocation": "NOASSERTION",
            "licenseConcluded": package.get("license") or "NOASSERTION",
            "licenseDeclared": package.get("license") or "NOASSERTION",
            "filesAnalyzed": False,
        }
        for package in sorted(metadata["packages"], key=lambda row: (row["name"], row["version"]))
    ]


def artifact_files(staging: Path) -> list[Path]:
    return sorted(
        [*staging.glob("*.zip"), *staging.glob("*.whl")],
        key=lambda path: path.name,
    )


def assemble(repo: Path, staging: Path, output: Path, revision: str) -> None:
    config = json.loads((repo / "release/release-v0.1.0.json").read_text())
    files = artifact_files(staging)
    if not files:
        raise ValueError("staging contains no release artifacts")
    output.mkdir(parents=True, exist_ok=False)
    artifacts = output / "artifacts"
    artifacts.mkdir()
    for source in files:
        shutil.copy2(source, artifacts / source.name)

    subjects = [{"name": path.name, "digest": {"sha256": digest(path)}} for path in sorted(artifacts.iterdir())]
    sbom = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"VectorKit-{config['version']}",
        "documentNamespace": f"https://github.com/gungorbasa/VectorKit/releases/tag/{config['tag']}#sbom",
        "creationInfo": {"created": source_date(), "creators": ["Tool: VectorKit-release-assembler-v1"]},
        "packages": cargo_packages(repo),
    }
    write_json(output / "sbom.spdx.json", sbom)
    provenance = {
        "_type": "https://in-toto.io/Statement/v1",
        "subject": subjects,
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "buildDefinition": {
                "buildType": "https://github.com/gungorbasa/VectorKit/release/v1",
                "externalParameters": {"version": config["version"], "platform": "arm64-apple"},
                "internalParameters": {},
                "resolvedDependencies": [{"uri": "git+https://github.com/gungorbasa/VectorKit", "digest": {"gitCommit": revision}}],
            },
            "runDetails": {"builder": {"id": "https://github.com/gungorbasa/VectorKit/actions"}, "metadata": {"invocationId": os.environ.get("GITHUB_RUN_ID", "local-dry-run")}},
        },
    }
    write_json(output / "provenance.intoto.json", provenance)
    payloads = sorted([*artifacts.iterdir(), output / "sbom.spdx.json", output / "provenance.intoto.json"])
    inventory = {"schema_version": 1, "files": {path.relative_to(output).as_posix(): digest(path) for path in payloads}}
    write_json(output / "inventory.json", inventory)
    manifest = {
        "schema_version": 1,
        "version": config["version"],
        "tag": config["tag"],
        "source_revision": revision,
        "platform": "arm64-apple",
        "artifact_count": len(files),
        "inventory_sha256": digest(output / "inventory.json"),
        "publication_ready": False,
        "publication_blockers": config["publication_blockers"],
    }
    write_json(output / "release-manifest.json", manifest)
    checksummed = [*payloads, output / "inventory.json", output / "release-manifest.json"]
    lines = [f"{digest(path)}  {path.relative_to(output).as_posix()}" for path in sorted(checksummed)]
    (output / "checksums.sha256").write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--staging", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args()
    assemble(args.repo.resolve(), args.staging.resolve(), args.output.resolve(), args.source_revision)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
