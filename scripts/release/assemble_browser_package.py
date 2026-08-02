#!/usr/bin/env python3
"""Assemble the approved browser retrieval npm package with both WASM tiers."""

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
PACKAGE_ROOT = REPO_ROOT / "wrappers" / "browser"
APPROVED_NAME = "@gungorbasa/retrievalkit-browser"
EXPECTED_REPOSITORY = {
    "type": "git",
    "url": "git+https://github.com/gungorbasa/RetrievalKit.git",
    "directory": "wrappers/browser",
}
NPM_NAME = re.compile(r"^(?:@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$")
SEMVER = re.compile(
    r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
WASM_TIERS = ("portable", "simd128")
WASM_FILES = (
    "retrievalkit_wasm.js",
    "retrievalkit_wasm.d.ts",
    "retrievalkit_wasm_bg.wasm",
    "retrievalkit_wasm_bg.wasm.d.ts",
)
TYPESCRIPT_MODULES = (
    "adapter",
    "databases",
    "errors",
    "generated-adapter",
    "index",
    "protocol",
    "rpc-client",
    "types",
    "worker",
)
REQUIRED_FILES = {
    "LICENSE",
    "NOTICE",
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "package.json",
    *(
        f"dist/{module}{suffix}"
        for module in TYPESCRIPT_MODULES
        for suffix in (".js", ".js.map", ".d.ts", ".d.ts.map")
    ),
    *(f"dist/wasm/{tier}/{filename}" for tier in WASM_TIERS for filename in WASM_FILES),
}


class AssemblyError(RuntimeError):
    """A browser retrieval package input or output violated the release contract."""


def run(
    command: list[str], *, cwd: Path, capture: bool = False
) -> subprocess.CompletedProcess[str]:
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


def clean_output(output: Path, package_root: Path) -> None:
    resolved = output.resolve()
    if resolved in {Path("/"), REPO_ROOT.resolve(), package_root.resolve()}:
        raise AssemblyError(f"refusing to replace unsafe output directory {resolved}")
    if resolved.exists():
        shutil.rmtree(resolved)
    resolved.mkdir(parents=True)


def copy_declared_files(
    source: Path, destination: Path, metadata: dict[str, Any]
) -> None:
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


def copy_wasm_tiers(generated_root: Path, destination: Path) -> None:
    for tier in WASM_TIERS:
        for filename in WASM_FILES:
            source = generated_root / tier / filename
            if not source.is_file():
                raise AssemblyError(
                    f"generated browser WASM input is missing: {source}"
                )
            target = destination / "dist" / "wasm" / tier / filename
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)


def inspect_tarball(tarball: Path, name: str, version: str) -> list[str]:
    wasm_payloads: dict[str, bytes] = {}
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
        unexpected = set(files).difference(REQUIRED_FILES)
        if unexpected:
            raise AssemblyError(
                f"{tarball.name} contains unexpected files: "
                f"{', '.join(sorted(unexpected))}"
            )
        if any(file.endswith(".node") for file in files):
            raise AssemblyError(
                f"{tarball.name} unexpectedly contains native Node code"
            )
        package_file = archive.extractfile("package/package.json")
        if package_file is None:
            raise AssemblyError(f"{tarball.name} package.json could not be read")
        metadata = json.load(package_file)
        for tier in WASM_TIERS:
            wasm_name = f"package/dist/wasm/{tier}/retrievalkit_wasm_bg.wasm"
            wasm_file = archive.extractfile(wasm_name)
            if wasm_file is None:
                raise AssemblyError(f"{tarball.name} {tier} WASM could not be read")
            wasm_payloads[tier] = wasm_file.read()
    if metadata.get("name") != name or metadata.get("version") != version:
        raise AssemblyError(f"{tarball.name} contains an unexpected package identity")
    if "private" in metadata or metadata.get("license") != "Apache-2.0":
        raise AssemblyError(f"{tarball.name} is not publishable under Apache-2.0")
    if metadata.get("publishConfig") != {
        "access": "public",
        "registry": "https://registry.npmjs.org/",
    }:
        raise AssemblyError(f"{tarball.name} is not pinned to public npm publication")
    if metadata.get("repository") != EXPECTED_REPOSITORY:
        raise AssemblyError(f"{tarball.name} does not identify the RetrievalKit repository")
    exports = metadata.get("exports", {})
    for tier in WASM_TIERS:
        require_export = {
            "types": f"./dist/wasm/{tier}/retrievalkit_wasm.d.ts",
            "import": f"./dist/wasm/{tier}/retrievalkit_wasm.js",
        }
        if exports.get(f"./wasm/{tier}") != require_export:
            raise AssemblyError(f"{tarball.name} lacks the {tier} WASM export")
        if not wasm_payloads[tier].startswith(b"\x00asm\x01\x00\x00\x00"):
            raise AssemblyError(f"{tarball.name} contains invalid {tier} WebAssembly")
    if wasm_payloads["portable"] == wasm_payloads["simd128"]:
        raise AssemblyError(
            f"{tarball.name} contains identical portable and SIMD128 WASM"
        )
    return files


def smoke_install(tarball: Path, name: str) -> None:
    with tempfile.TemporaryDirectory(
        prefix="retrievalkit-browser-consumer-"
    ) as temporary:
        consumer = Path(temporary)
        metadata = {
            "name": "retrievalkit-browser-release-smoke",
            "private": True,
            "type": "module",
            "dependencies": {name: f"file:{tarball.resolve()}"},
        }
        (consumer / "package.json").write_text(
            json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
        )
        (consumer / "smoke.mjs").write_text(
            "\n".join(
                (
                    f'import * as browser from "{name}";',
                    f'import * as worker from "{name}/worker";',
                    f'import * as adapter from "{name}/adapter";',
                    f'import portable from "{name}/wasm/portable";',
                    f'import simd128 from "{name}/wasm/simd128";',
                    'if (typeof browser.RetrievalKitBrowser !== "function") throw new Error("browser export missing");',
                    'if (typeof worker.installRetrievalKitWorker !== "function") throw new Error("worker export missing");',
                    'if (typeof adapter.createAdaptiveGeneratedWasmAdapter !== "function") throw new Error("adapter export missing");',
                    'if (typeof portable !== "function" || typeof simd128 !== "function") throw new Error("WASM initializer missing");',
                    "",
                )
            ),
            encoding="utf-8",
        )
        run(
            [
                "npm",
                "install",
                "--ignore-scripts",
                "--offline",
                "--no-audit",
                "--no-fund",
            ],
            cwd=consumer,
        )
        run(["node", "smoke.mjs"], cwd=consumer)


def assemble(
    *,
    name: str,
    version: str,
    generated_root: Path,
    output: Path,
    name_approved: bool = False,
    skip_build: bool = False,
    skip_smoke: bool = False,
    package_root: Path = PACKAGE_ROOT,
) -> dict[str, Any]:
    if not name_approved:
        raise AssemblyError(
            "browser retrieval package-name approval is unresolved; pass "
            "--name-approved only after the owner approves the requested name"
        )
    validate_name(name)
    if name != APPROVED_NAME:
        raise AssemblyError(f"release name must be exactly {APPROVED_NAME!r}")
    validate_version(version)
    if not skip_build:
        run(["npm", "run", "check"], cwd=package_root)
        run(["npm", "run", "build"], cwd=package_root)

    output = output.resolve()
    clean_output(output, package_root)
    with tempfile.TemporaryDirectory(
        prefix="retrievalkit-browser-release-"
    ) as temporary:
        staging = Path(temporary)
        source_metadata = json.loads(
            (package_root / "package.json").read_text(encoding="utf-8")
        )
        if source_metadata.get("private") is not True:
            raise AssemblyError("browser retrieval source package must remain private")
        for legal_name in ("LICENSE", "NOTICE"):
            if (package_root / legal_name).read_bytes() != (
                REPO_ROOT / legal_name
            ).read_bytes():
                raise AssemblyError(
                    f"browser retrieval {legal_name} differs from the repository"
                )
        copy_declared_files(package_root, staging, source_metadata)
        copy_wasm_tiers(generated_root, staging)
        shutil.copy2(REPO_ROOT / "THIRD_PARTY_NOTICES.md", staging)
        metadata = dict(source_metadata)
        metadata.pop("private", None)
        metadata.pop("scripts", None)
        metadata.pop("devDependencies", None)
        metadata["name"] = name
        metadata["version"] = version
        metadata["description"] = (
            "Worker-owned exact, BM25, hybrid, and graph retrieval for browsers"
        )
        metadata["files"] = sorted({*metadata["files"], "THIRD_PARTY_NOTICES.md"})
        metadata["exports"] = {
            **metadata["exports"],
            "./wasm/portable": {
                "types": "./dist/wasm/portable/retrievalkit_wasm.d.ts",
                "import": "./dist/wasm/portable/retrievalkit_wasm.js",
            },
            "./wasm/simd128": {
                "types": "./dist/wasm/simd128/retrievalkit_wasm.d.ts",
                "import": "./dist/wasm/simd128/retrievalkit_wasm.js",
            },
        }
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

    if not skip_smoke:
        smoke_install(tarball, name)
    artifact = {
        "capability": "browser-retrieval-graph",
        "file": tarball.name,
        "npmName": name,
        "version": version,
        "runtime": "browser-worker-wasm",
        "wasmTiers": list(WASM_TIERS),
        "size": tarball.stat().st_size,
        "sha256": digest(tarball, "sha256"),
        "sha512": digest(tarball, "sha512"),
        "npmIntegrity": report["integrity"],
        "files": files,
    }
    inventory = {
        "schemaVersion": 1,
        "kind": "retrievalkit-browser-release",
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
    parser.add_argument("--generated-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--name-approved", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    try:
        assemble(
            name=args.name,
            version=args.version,
            generated_root=args.generated_root.resolve(),
            output=args.output,
            name_approved=args.name_approved,
            skip_build=args.skip_build,
        )
    except (AssemblyError, subprocess.CalledProcessError) as error:
        raise SystemExit(
            f"Browser retrieval package assembly failed: {error}"
        ) from error
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
