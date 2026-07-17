#!/usr/bin/env python3
"""Bootstrap the pinned official NIST trec_eval used by the V3 publication gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[2]
UPSTREAM_URL = "https://github.com/usnistgov/trec_eval"
UPSTREAM_COMMIT = "f4253652c8efd0d86ddffd0d163cc0a0f813111a"
UPSTREAM_VERSION = "10.0-rc3"
ARCHIVE_URL = f"https://codeload.github.com/usnistgov/trec_eval/tar.gz/{UPSTREAM_COMMIT}"
ARCHIVE_SHA256 = "3cc2618656038df53b6783aef44de24d72854a4877064ce1d12b2205fcd63165"
SOURCE_TREE_SHA256 = "b68cb9ad8d407c6e1e4d1bce9d867a7525a841d4a1b98b19478a984dde445e28"
DEFAULT_TOOL_ROOT = ROOT / "target/benchmarks/tools/trec_eval-f4253652-v3"
IDENTITY_NAME = "identity.json"
PRECISION_PATCH = {
    "description": (
        "output-only patch replacing the official four-decimal measure format with "
        "17-significant-digit output; metric implementations are unchanged"
    ),
    "files": ["meas_print_final.c", "meas_print_single.c"],
    "from": "%6.4f",
    "to": "%.17g",
}


class BootstrapError(RuntimeError):
    """Raised when the pinned tool cannot be reproduced exactly."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, allow_nan=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")


def source_tree_identity(root: Path) -> tuple[str, list[dict[str, object]]]:
    files: list[dict[str, object]] = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or (not path.is_file() and not path.is_dir()):
            raise BootstrapError(f"source tree contains unsupported entry '{path}'")
        if path.is_file():
            relative = path.relative_to(root).as_posix()
            data = path.read_bytes()
            files.append({"bytes": len(data), "path": relative, "sha256": sha256(data)})
    return sha256(canonical_bytes(files)), files


def verify_archive(path: Path) -> bytes:
    data = path.read_bytes()
    actual = sha256(data)
    if actual != ARCHIVE_SHA256:
        raise BootstrapError(
            f"official trec_eval archive checksum mismatch: expected {ARCHIVE_SHA256}, "
            f"actual {actual}"
        )
    return data


def download_archive(path: Path) -> bytes:
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(ARCHIVE_URL) as response:  # noqa: S310 - pinned HTTPS URL
            data = response.read()
        actual = sha256(data)
        if actual != ARCHIVE_SHA256:
            raise BootstrapError(
                f"downloaded official trec_eval archive checksum mismatch: expected "
                f"{ARCHIVE_SHA256}, actual {actual}"
            )
        temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
        temporary.write_bytes(data)
        os.replace(temporary, path)
    return verify_archive(path)


def safe_extract(archive: Path, destination: Path) -> None:
    with tarfile.open(archive, mode="r:gz") as bundle:
        members = bundle.getmembers()
        if not members:
            raise BootstrapError("official trec_eval source archive is empty")
        roots = set()
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not path.parts:
                raise BootstrapError(f"unsafe source archive member '{member.name}'")
            if len(path.parts) == 1 and not member.isdir():
                raise BootstrapError(f"unsafe source archive member '{member.name}'")
            if member.issym() or member.islnk() or not (member.isfile() or member.isdir()):
                raise BootstrapError(f"unsupported source archive member '{member.name}'")
            roots.add(path.parts[0])
        if len(roots) != 1:
            raise BootstrapError("official trec_eval archive must have one top-level directory")
        top = next(iter(roots))
        for member in members:
            relative = PurePosixPath(*PurePosixPath(member.name).parts[1:])
            if not relative.parts:
                continue
            target = destination.joinpath(*relative.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            source = bundle.extractfile(member)
            if source is None:
                raise BootstrapError(f"failed to extract source member '{top}/{relative}'")
            target.write_bytes(source.read())
            target.chmod(member.mode & 0o777)


def apply_precision_patch(source: Path) -> None:
    for relative in PRECISION_PATCH["files"]:
        path = source / str(relative)
        data = path.read_bytes()
        old = str(PRECISION_PATCH["from"]).encode("ascii")
        new = str(PRECISION_PATCH["to"]).encode("ascii")
        count = data.count(old)
        if count == 0:
            raise BootstrapError(f"precision patch preimage missing from '{relative}'")
        path.write_bytes(data.replace(old, new))


def compiler_identity(compiler: str) -> dict[str, str]:
    resolved = shutil.which(compiler)
    if resolved is None:
        raise BootstrapError(f"C compiler '{compiler}' was not found")
    result = subprocess.run(
        [resolved, "--version"], capture_output=True, check=False, text=True
    )
    if result.returncode != 0:
        raise BootstrapError(f"failed to identify C compiler '{resolved}'")
    version = (result.stdout or result.stderr).strip()
    if not version:
        raise BootstrapError(f"C compiler '{resolved}' returned an empty identity")
    return {"executable": str(Path(resolved).resolve()), "version": version}


def verify_executable(binary: Path) -> str:
    if not binary.is_file() or binary.is_symlink():
        raise BootstrapError(f"trec_eval executable is not a regular file: '{binary}'")
    result = subprocess.run([str(binary), "-v"], capture_output=True, check=False, text=True)
    identity = (result.stderr + result.stdout).strip()
    expected = f"trec_eval version {UPSTREAM_VERSION}"
    if result.returncode != 0 or identity != expected:
        raise BootstrapError(
            f"trec_eval executable identity mismatch: expected '{expected}', actual '{identity}'"
        )
    return sha256(binary.read_bytes())


def build(tool_root: Path, compiler: str) -> dict[str, object]:
    archive = tool_root / "source.tar.gz"
    download_archive(archive)
    compiler_info = compiler_identity(compiler)
    tool_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".source-staging-", dir=tool_root) as directory:
        staged_source = Path(directory) / "source"
        staged_source.mkdir()
        safe_extract(archive, staged_source)
        apply_precision_patch(staged_source)
        tree_sha256, tree_files = source_tree_identity(staged_source)
        if tree_sha256 != SOURCE_TREE_SHA256:
            raise BootstrapError(
                "patched official trec_eval source-tree checksum mismatch: "
                f"expected {SOURCE_TREE_SHA256}, actual {tree_sha256}"
            )

        source = tool_root / "source"
        if source.exists():
            existing_sha256, _ = source_tree_identity(source)
            if existing_sha256 != tree_sha256:
                raise BootstrapError(
                    f"existing trec_eval source tree checksum mismatch: expected {tree_sha256}, "
                    f"actual {existing_sha256}"
                )
        else:
            os.replace(staged_source, source)

    with tempfile.TemporaryDirectory(prefix=".build-staging-", dir=tool_root) as directory:
        build_root = Path(directory) / "source"
        shutil.copytree(tool_root / "source", build_root)
        result = subprocess.run(
            ["make", f"CC={compiler_info['executable']}"],
            cwd=build_root,
            capture_output=True,
            check=False,
            text=True,
        )
        if result.returncode != 0:
            raise BootstrapError(
                "official trec_eval build failed:\n"
                + (result.stderr.strip() or result.stdout.strip())
            )
        built = build_root / "trec_eval"
        executable_sha256 = verify_executable(built)
        bin_root = tool_root / "bin"
        bin_root.mkdir(exist_ok=True)
        binary = bin_root / "trec_eval"
        if binary.exists():
            existing_sha256 = verify_executable(binary)
            if existing_sha256 != executable_sha256:
                raise BootstrapError(
                    "existing trec_eval executable differs from a fresh pinned build: "
                    f"expected {executable_sha256}, actual {existing_sha256}"
                )
        else:
            temporary = bin_root / f".trec_eval.{os.getpid()}.tmp"
            shutil.copy2(built, temporary)
            os.replace(temporary, binary)

    identity: dict[str, object] = {
        "archive_sha256": ARCHIVE_SHA256,
        "archive_url": ARCHIVE_URL,
        "compiler": compiler_info,
        "executable": str((tool_root / "bin/trec_eval").resolve()),
        "executable_sha256": executable_sha256,
        "precision_patch": PRECISION_PATCH,
        "source_file_count": len(tree_files),
        "source_tree_sha256": tree_sha256,
        "upstream_commit": UPSTREAM_COMMIT,
        "upstream_url": UPSTREAM_URL,
        "version": UPSTREAM_VERSION,
    }
    identity_path = tool_root / IDENTITY_NAME
    identity_path.write_bytes(canonical_bytes(identity) + b"\n")
    return identity


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tool-root", type=Path, default=DEFAULT_TOOL_ROOT)
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        identity = build(args.tool_root.resolve(), args.cc)
        print(json.dumps(identity, indent=2, sort_keys=True))
        return 0
    except (BootstrapError, OSError, subprocess.SubprocessError, tarfile.TarError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
