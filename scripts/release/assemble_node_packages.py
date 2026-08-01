#!/usr/bin/env python3
"""Assemble verified npm tarballs after registry names have been approved."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
TYPESCRIPT_ROOT = REPO_ROOT / "wrappers" / "typescript"
SOURCE_NAMES = {
    "base": "@gungorbasa/retrievalkit",
    "graph": "@gungorbasa/retrievalkit-graph",
    "embedding": "@gungorbasa/retrievalkit-embedding",
}
APPROVED_NAMES = SOURCE_NAMES
PACKAGE_DIRECTORIES = {
    "base": TYPESCRIPT_ROOT / "base",
    "graph": TYPESCRIPT_ROOT / "graph",
    "embedding": TYPESCRIPT_ROOT / "embedding",
}
COMMON_PACKAGE_FILES = {
    "LICENSE",
    "NOTICE",
    "README.md",
    "dist/index.js",
    "dist/index.d.ts",
    "package.json",
}
NATIVE_FILES = {
    "base": "native/retrievalkit.node",
    "graph": "native/retrievalkit.node",
    "embedding": "native/retrievalkit-embedding.node",
}
EMBEDDING_RUNTIME_FILES = {
    "runtime/libonnxruntime.1.24.3.dylib",
    "runtime/ONNX-Runtime-LICENSE",
    "runtime/ONNX-Runtime-ThirdPartyNotices.txt",
}
ONNX_RUNTIME_SIZE = 27_724_968
ONNX_RUNTIME_SHA256 = (
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"
)
NPM_NAME = re.compile(
    r"^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$"
)
SEMVER = re.compile(
    r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


class AssemblyError(RuntimeError):
    """A release input or generated package did not pass closed validation."""


def run(
    command: list[str],
    *,
    cwd: Path = REPO_ROOT,
    capture: bool = False,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=capture,
    )


def validate_npm_name(name: str) -> str:
    if len(name) > 214 or not NPM_NAME.fullmatch(name):
        raise AssemblyError(
            f"invalid npm package name {name!r}; supply an approved lowercase, URL-safe "
            "scoped or unscoped name"
        )
    if name in {"node_modules", "favicon.ico"}:
        raise AssemblyError(f"{name!r} is reserved and cannot be used as an npm package name")
    return name


def validate_version(version: str) -> str:
    if not SEMVER.fullmatch(version):
        raise AssemblyError(
            f"invalid version {version!r}; release assembly requires a SemVer x.y.z value"
        )
    return version


def validate_host() -> None:
    if platform.system() != "Darwin" or platform.machine() not in {"arm64", "aarch64"}:
        raise AssemblyError(
            "Node release assembly currently supports only the authorized macOS arm64 target"
        )


def validate_macho_arm64(path: Path) -> None:
    try:
        header = path.read_bytes()[:8]
        magic, cpu_type = struct.unpack("<II", header)
    except (OSError, struct.error) as error:
        raise AssemblyError(f"could not inspect native addon {path}: {error}") from error
    if magic != 0xFEEDFACF or cpu_type != 0x0100000C:
        raise AssemblyError(f"{path} is not a 64-bit arm64 Mach-O native addon")


def clean_output(output: Path) -> None:
    resolved = output.resolve()
    if resolved in {Path("/"), REPO_ROOT.resolve(), TYPESCRIPT_ROOT.resolve()}:
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


def release_metadata(
    source: dict[str, Any],
    *,
    capability: str,
    name: str,
    version: str,
) -> dict[str, Any]:
    metadata = dict(source)
    metadata.pop("private", None)
    metadata.pop("scripts", None)
    metadata["name"] = name
    metadata["version"] = version
    descriptions = {
        "base": "Local-first exact, BM25, and hybrid retrieval for Node.js on macOS arm64",
        "graph": "Optional local graph and graph-scoped retrieval for Node.js on macOS arm64",
        "embedding": "Optional local FP32 MiniLM embeddings for Node.js on macOS arm64",
    }
    metadata["description"] = descriptions[capability]
    metadata["publishConfig"] = {
        "access": "public",
        "registry": "https://registry.npmjs.org/",
    }
    return metadata


def rewrite_readme(path: Path, names: dict[str, str]) -> None:
    content = path.read_text(encoding="utf-8")
    for capability, source_name in sorted(
        SOURCE_NAMES.items(), key=lambda item: len(item[1]), reverse=True
    ):
        content = content.replace(source_name, names[capability])
    path.write_text(content, encoding="utf-8")


def inspect_tarball(
    tarball: Path,
    *,
    capability: str,
    expected_name: str,
    expected_version: str,
) -> list[str]:
    with tarfile.open(tarball, "r:gz") as archive:
        members = archive.getmembers()
        if any(member.issym() or member.islnk() for member in members):
            raise AssemblyError(f"{tarball.name} contains a symbolic or hard link")
        if any(
            not member.name.startswith("package/")
            or ".." in Path(member.name).parts
            for member in members
        ):
            raise AssemblyError(f"{tarball.name} contains an unsafe archive path")
        files = sorted(
            member.name.removeprefix("package/")
            for member in members
            if member.isfile()
        )
        required = COMMON_PACKAGE_FILES | {NATIVE_FILES[capability]}
        if capability == "embedding":
            required |= EMBEDDING_RUNTIME_FILES
        missing = required.difference(files)
        if missing:
            raise AssemblyError(
                f"{tarball.name} is missing required files: {', '.join(sorted(missing))}"
            )
        package_member = archive.getmember("package/package.json")
        package_file = archive.extractfile(package_member)
        if package_file is None:
            raise AssemblyError(f"{tarball.name} package.json could not be read")
        metadata = json.load(package_file)
        runtime = None
        if capability == "embedding":
            runtime_file = archive.extractfile(
                "package/runtime/libonnxruntime.1.24.3.dylib"
            )
            if runtime_file is None:
                raise AssemblyError(f"{tarball.name} ONNX Runtime could not be read")
            runtime = runtime_file.read()

    if metadata.get("name") != expected_name or metadata.get("version") != expected_version:
        raise AssemblyError(f"{tarball.name} contains unexpected package identity")
    if metadata.get("private") is not None:
        raise AssemblyError(f"{tarball.name} still contains npm's private publication blocker")
    if metadata.get("license") != "Apache-2.0":
        raise AssemblyError(f"{tarball.name} does not declare Apache-2.0")
    if metadata.get("os") != ["darwin"] or metadata.get("cpu") != ["arm64"]:
        raise AssemblyError(f"{tarball.name} does not fail closed to macOS arm64")
    if metadata.get("publishConfig", {}).get("registry") != "https://registry.npmjs.org/":
        raise AssemblyError(f"{tarball.name} is not pinned to the public npm registry")
    if capability == "base" and any(
        "graph" in file_name.lower()
        for file_name in files
        if file_name.startswith(("dist/", "native/"))
    ):
        raise AssemblyError(f"{tarball.name} contains graph executable content")
    if capability == "embedding":
        assert runtime is not None
        if len(runtime) != ONNX_RUNTIME_SIZE or hashlib.sha256(runtime).hexdigest() != ONNX_RUNTIME_SHA256:
            raise AssemblyError(f"{tarball.name} contains an unqualified ONNX Runtime")
    return files


def digest(path: Path, algorithm: str) -> str:
    checksum = hashlib.new(algorithm)
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def assemble(
    *,
    base_name: str,
    graph_name: str,
    embedding_name: str,
    version: str,
    output: Path,
    names_approved: bool = False,
    skip_native_build: bool = False,
    skip_typescript_build: bool = False,
) -> dict[str, Any]:
    if not names_approved:
        raise AssemblyError(
            "npm package-name approval is unresolved; pass --names-approved only after "
            "the owner approves all requested names"
        )
    names = {
        "base": validate_npm_name(base_name),
        "graph": validate_npm_name(graph_name),
        "embedding": validate_npm_name(embedding_name),
    }
    if names != APPROVED_NAMES:
        raise AssemblyError(
            "release names must be exactly base='@gungorbasa/retrievalkit', "
            "graph='@gungorbasa/retrievalkit-graph', and "
            "embedding='@gungorbasa/retrievalkit-embedding'"
        )
    if len(set(names.values())) != len(names):
        raise AssemblyError("Node npm package names must be different")
    validate_version(version)
    validate_host()

    if not skip_native_build:
        run(["npm", "run", "build:native"], cwd=TYPESCRIPT_ROOT)
    if not skip_typescript_build:
        for package_directory in PACKAGE_DIRECTORIES.values():
            run(["npm", "run", "build"], cwd=package_directory)
    run(["node", "./scripts/verify-package-content.mjs"], cwd=TYPESCRIPT_ROOT)
    for capability, package_directory in PACKAGE_DIRECTORIES.items():
        validate_macho_arm64(package_directory / NATIVE_FILES[capability])

    clean_output(output)
    artifacts: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="retrievalkit-node-release-") as temporary:
        staging_root = Path(temporary)
        for capability in ("base", "graph", "embedding"):
            source = PACKAGE_DIRECTORIES[capability]
            staging = staging_root / capability
            staging.mkdir()
            source_metadata = json.loads(
                (source / "package.json").read_text(encoding="utf-8")
            )
            copy_declared_files(source, staging, source_metadata)
            metadata = release_metadata(
                source_metadata,
                capability=capability,
                name=names[capability],
                version=version,
            )
            (staging / "package.json").write_text(
                json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            rewrite_readme(staging / "README.md", names)
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
            files = inspect_tarball(
                tarball,
                capability=capability,
                expected_name=names[capability],
                expected_version=version,
            )
            artifacts.append(
                {
                    "capability": capability,
                    "file": tarball.name,
                    "npmName": names[capability],
                    "version": version,
                    "platform": {"os": "darwin", "cpu": "arm64"},
                    "size": tarball.stat().st_size,
                    "sha256": digest(tarball, "sha256"),
                    "sha512": digest(tarball, "sha512"),
                    "npmIntegrity": report["integrity"],
                    "files": files,
                }
            )

    artifacts.sort(key=lambda artifact: artifact["capability"])
    inventory = {
        "schemaVersion": 1,
        "kind": "retrievalkit-node-release",
        "artifactReady": True,
        "publicationReady": False,
        "registry": "https://registry.npmjs.org/",
        "uploadBlockers": [
            "npm trusted publishing or an owner-controlled publish credential must be configured"
        ],
        "provenanceBlockers": [
            "npm provenance requires trusted publishing from a public repository"
        ],
        "artifacts": artifacts,
    }
    (output / "inventory.json").write_text(
        json.dumps(inventory, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    for algorithm, filename in (
        ("sha256", "SHA256SUMS"),
        ("sha512", "SHA512SUMS"),
    ):
        lines = [
            f"{digest(output / artifact['file'], algorithm)}  {artifact['file']}"
            for artifact in artifacts
        ]
        (output / filename).write_text("\n".join(lines) + "\n", encoding="utf-8")
    return inventory


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build closed macOS arm64 npm tarballs. Public package names are required "
            "because registry ownership must never be inferred."
        )
    )
    parser.add_argument("--base-name", required=True, help="owner-approved base npm name")
    parser.add_argument("--graph-name", required=True, help="owner-approved graph npm name")
    parser.add_argument(
        "--embedding-name",
        required=True,
        help="owner-approved embedding npm name",
    )
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "dist" / "release" / "node",
    )
    parser.add_argument(
        "--names-approved",
        action="store_true",
        help="assert that the owner approved all supplied npm names",
    )
    parser.add_argument("--skip-native-build", action="store_true")
    parser.add_argument("--skip-typescript-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        inventory = assemble(
            base_name=arguments.base_name,
            graph_name=arguments.graph_name,
            embedding_name=arguments.embedding_name,
            version=arguments.version,
            output=arguments.output,
            names_approved=arguments.names_approved,
            skip_native_build=arguments.skip_native_build,
            skip_typescript_build=arguments.skip_typescript_build,
        )
    except (AssemblyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Node package assembly failed: {error}") from error
    print(
        f"Assembled {len(inventory['artifacts'])} verified npm tarballs in "
        f"{arguments.output.resolve()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
