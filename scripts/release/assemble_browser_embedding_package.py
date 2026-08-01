#!/usr/bin/env python3
"""Assemble the approved browser embedding npm package with a closed inventory."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
PACKAGE_ROOT = REPO_ROOT / "wrappers" / "browser-embedding"
APPROVED_NAME = "@gungorbasa/retrievalkit-browser-embedding"
NPM_NAME = re.compile(r"^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$")
SEMVER = re.compile(
    r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
REQUIRED_FILES = {
    "LICENSE",
    "NOTICE",
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "package.json",
    "dist/index.js",
    "dist/index.d.ts",
    "dist/worker.js",
    "dist/worker.d.ts",
    "dist/runtime/ort-wasm-simd-threaded.mjs",
    "dist/runtime/ort-wasm-simd-threaded.wasm",
    "dist/runtime/ort-wasm-simd-threaded.asyncify.mjs",
    "dist/runtime/ort-wasm-simd-threaded.asyncify.wasm",
    "dist/runtime/ONNXRUNTIME-LICENSE",
    "dist/runtime/ONNXRUNTIME-ThirdPartyNotices.txt",
    "dist/runtime/HUGGINGFACE-TOKENIZERS-LICENSE",
}


class AssemblyError(RuntimeError):
    """A browser package input or output violated the release contract."""


def run(command: list[str], *, cwd: Path, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=capture,
    )


def digest(path: Path, algorithm: str) -> str:
    value = hashlib.new(algorithm)
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def validate_name(name: str) -> str:
    if len(name) > 214 or not NPM_NAME.fullmatch(name):
        raise AssemblyError(f"invalid npm package name {name!r}")
    return name


def validate_version(version: str) -> str:
    if not SEMVER.fullmatch(version):
        raise AssemblyError(f"invalid release version {version!r}")
    return version


def clean_output(output: Path) -> None:
    resolved = output.resolve()
    if resolved in {Path("/"), REPO_ROOT.resolve(), PACKAGE_ROOT.resolve()}:
        raise AssemblyError(f"refusing to replace unsafe output directory {resolved}")
    if resolved.exists():
        shutil.rmtree(resolved)
    resolved.mkdir(parents=True)


def copy_declared_files(source: Path, destination: Path, metadata: dict[str, Any]) -> None:
    for relative in metadata["files"]:
        source_path = source / relative
        destination_path = destination / relative
        if not source_path.exists():
            raise AssemblyError(f"declared package input is missing: {source_path}")
        if source_path.is_dir():
            shutil.copytree(source_path, destination_path)
        else:
            destination_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source_path, destination_path)


def inspect_tarball(tarball: Path, name: str, version: str) -> list[str]:
    with tarfile.open(tarball, "r:gz") as archive:
        members = archive.getmembers()
        if any(member.issym() or member.islnk() for member in members):
            raise AssemblyError(f"{tarball.name} contains a symbolic or hard link")
        if any(
            not member.name.startswith("package/") or ".." in Path(member.name).parts
            for member in members
        ):
            raise AssemblyError(f"{tarball.name} contains an unsafe archive path")
        files = sorted(
            member.name.removeprefix("package/")
            for member in members
            if member.isfile()
        )
        missing = REQUIRED_FILES.difference(files)
        if missing:
            raise AssemblyError(
                f"{tarball.name} is missing required files: {', '.join(sorted(missing))}"
            )
        if any(file.endswith(".node") for file in files):
            raise AssemblyError(f"{tarball.name} unexpectedly contains native Node code")
        package_file = archive.extractfile("package/package.json")
        if package_file is None:
            raise AssemblyError(f"{tarball.name} package.json could not be read")
        metadata = json.load(package_file)
    if metadata.get("name") != name or metadata.get("version") != version:
        raise AssemblyError(f"{tarball.name} contains an unexpected package identity")
    if "private" in metadata or metadata.get("license") != "Apache-2.0":
        raise AssemblyError(f"{tarball.name} is not publishable under Apache-2.0")
    if metadata.get("publishConfig", {}).get("registry") != "https://registry.npmjs.org/":
        raise AssemblyError(f"{tarball.name} is not pinned to the public npm registry")
    return files


def assemble(
    *,
    name: str,
    version: str,
    output: Path,
    name_approved: bool = False,
    skip_build: bool = False,
) -> dict[str, Any]:
    if not name_approved:
        raise AssemblyError(
            "browser embedding package-name approval is unresolved; pass "
            "--name-approved only after the owner approves the requested name"
        )
    validate_name(name)
    if name != APPROVED_NAME:
        raise AssemblyError(f"release name must be exactly {APPROVED_NAME!r}")
    validate_version(version)
    if not skip_build:
        run(["npm", "run", "check:package"], cwd=PACKAGE_ROOT)

    output = output.resolve()
    clean_output(output)
    with tempfile.TemporaryDirectory(prefix="retrievalkit-browser-embedding-release-") as temporary:
        staging = Path(temporary)
        source_metadata = json.loads(
            (PACKAGE_ROOT / "package.json").read_text(encoding="utf-8")
        )
        copy_declared_files(PACKAGE_ROOT, staging, source_metadata)
        metadata = dict(source_metadata)
        metadata.pop("private", None)
        metadata.pop("scripts", None)
        metadata["name"] = name
        metadata["version"] = version
        metadata["description"] = (
            "Local FP32 MiniLM embeddings for dedicated browser Workers"
        )
        metadata["publishConfig"] = {
            "access": "public",
            "registry": "https://registry.npmjs.org/",
        }
        (staging / "package.json").write_text(
            json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        result = run(
            [
                "npm",
                "pack",
                "--ignore-scripts",
                "--json",
                "--pack-destination",
                str(output),
                ".",
            ],
            cwd=staging,
            capture=True,
        )
        report = json.loads(result.stdout)[0]
        tarball = output / report["filename"]
        files = inspect_tarball(tarball, name, version)

    artifact = {
        "capability": "browser-embedding",
        "file": tarball.name,
        "npmName": name,
        "version": version,
        "runtime": "browser-worker",
        "size": tarball.stat().st_size,
        "sha256": digest(tarball, "sha256"),
        "sha512": digest(tarball, "sha512"),
        "npmIntegrity": report["integrity"],
        "files": files,
    }
    inventory = {
        "schemaVersion": 1,
        "kind": "retrievalkit-browser-embedding-release",
        "artifactReady": True,
        "publicationReady": False,
        "registry": "https://registry.npmjs.org/",
        "artifacts": [artifact],
    }
    (output / "inventory.json").write_text(
        json.dumps(inventory, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    for algorithm, filename in (("sha256", "SHA256SUMS"), ("sha512", "SHA512SUMS")):
        (output / filename).write_text(
            f"{digest(tarball, algorithm)}  {tarball.name}\n",
            encoding="ascii",
        )
    return inventory


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", required=True)
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--name-approved", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    try:
        assemble(
            name=args.name,
            version=args.version,
            output=args.output,
            name_approved=args.name_approved,
            skip_build=args.skip_build,
        )
    except (AssemblyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Browser embedding package assembly failed: {error}") from error
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
