#!/usr/bin/env python3
"""Build and safely validate RetrievalKit's deterministic Core ML FP32 archive.

The archive is deliberately an uncompressed POSIX ustar file so Apple clients
can validate and extract it without a third-party archive dependency. It
contains regular files only; directory creation is implicit during extraction.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import tarfile
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Sequence


ROOT_DIR = Path(__file__).resolve().parents[2]
TARGET_DIR = ROOT_DIR / "target"
MODEL_ID = "sentence-transformers/all-MiniLM-L6-v2"
MODEL_REVISION = "c9745ed1d9f207416be6d2e6f8de32d1f16199bf"
MODEL_DIRECTORY = "coreml/all-MiniLM-L6-v2-fp32.mlpackage"
TOKENIZER_DIRECTORY = "tokenizer"
ARCHIVE_MANIFEST_NAME = "archive-manifest-v1.json"
ARCHIVE_NAME = "all-MiniLM-L6-v2-coreml-fp32-v1.tar"
ARTIFACT_ID = (
    "sentence-transformers--all-MiniLM-L6-v2--"
    f"{MODEL_REVISION}--coreml-fp32-fixed256-v1"
)
SCHEMA_VERSION = 1
CANONICAL_MODEL_TREE_SHA256 = (
    "6de733c8906b816a310c2735712022ad2093edcd1b17566b86553a2c730b9ec7"
)
FIXED_MTIME = 0
REGULAR_MODE = 0o644
COPY_BUFFER_SIZE = 1024 * 1024

TOKENIZER_FILES = (
    "special_tokens_map.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "vocab.txt",
)
ATTRIBUTION_FILES = ("LICENSE", "NOTICE", "README.md")
MODEL_FILES = (
    f"{MODEL_DIRECTORY}/Data/com.apple.CoreML/model.mlmodel",
    f"{MODEL_DIRECTORY}/Data/com.apple.CoreML/weights/weight.bin",
    f"{MODEL_DIRECTORY}/Manifest.json",
)
PAYLOAD_PATHS = tuple(sorted((*ATTRIBUTION_FILES, *MODEL_FILES, *(
    f"{TOKENIZER_DIRECTORY}/{name}" for name in TOKENIZER_FILES
))))


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(COPY_BUFFER_SIZE), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_tree_sha256(files: Iterable[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for item in sorted(files, key=lambda value: value["path"]):
        digest.update(
            f"{item['path']}\0{item['size']}\0{item['sha256']}\n".encode("utf-8")
        )
    return digest.hexdigest()


def package_tree_stats(package: Path) -> tuple[int, str]:
    records: list[dict[str, Any]] = []
    for path in sorted(package.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Core ML package contains a symbolic link: {path}")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError(f"Core ML package contains an unsafe entry: {path}")
        records.append(
            {
                "path": path.relative_to(package).as_posix(),
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    if not records:
        raise ValueError(f"Core ML package is empty: {package}")
    return sum(item["size"] for item in records), canonical_tree_sha256(records)


def validate_source(source_root: Path) -> list[dict[str, Any]]:
    source_root = source_root.resolve()
    package = source_root / MODEL_DIRECTORY
    _, model_digest = package_tree_stats(package)
    if model_digest != CANONICAL_MODEL_TREE_SHA256:
        raise ValueError(
            "Core ML package does not match the qualified canonical tree: "
            f"expected {CANONICAL_MODEL_TREE_SHA256}, got {model_digest}"
        )

    actual_files = {
        path.relative_to(package).as_posix()
        for path in package.rglob("*")
        if path.is_file()
    }
    expected_model_files = {
        Path(path).relative_to(MODEL_DIRECTORY).as_posix() for path in MODEL_FILES
    }
    if actual_files != expected_model_files:
        raise ValueError(
            "Core ML package has an unexpected file set: "
            f"expected {sorted(expected_model_files)}, got {sorted(actual_files)}"
        )

    records: list[dict[str, Any]] = []
    for relative in PAYLOAD_PATHS:
        path = source_root / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"required regular source file is missing: {relative}")
        records.append(
            {
                "path": relative,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return records


def manifest_document(files: Sequence[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "artifactID": ARTIFACT_ID,
        "modelPath": MODEL_DIRECTORY,
        "tokenizerPath": TOKENIZER_DIRECTORY,
        "canonicalTreeSHA256": canonical_tree_sha256(files),
        "files": list(files),
    }


def manifest_bytes(files: Sequence[dict[str, Any]]) -> bytes:
    return (
        json.dumps(
            manifest_document(files),
            ensure_ascii=True,
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode("utf-8")


def tar_info(path: str, size: int) -> tarfile.TarInfo:
    info = tarfile.TarInfo(path)
    info.size = size
    info.mtime = FIXED_MTIME
    info.mode = REGULAR_MODE
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.type = tarfile.REGTYPE
    return info


def build_archive(source_root: Path, output_dir: Path) -> tuple[Path, Path]:
    source_root = source_root.resolve()
    output_dir = require_target_path(output_dir, "output directory")
    output_dir.mkdir(parents=True, exist_ok=True)
    files = validate_source(source_root)
    encoded_manifest = manifest_bytes(files)
    archive_path = output_dir / ARCHIVE_NAME
    manifest_path = output_dir / ARCHIVE_MANIFEST_NAME

    temporary_archive = output_dir / f".{ARCHIVE_NAME}.{os.getpid()}.tmp"
    temporary_manifest = output_dir / f".{ARCHIVE_MANIFEST_NAME}.{os.getpid()}.tmp"
    for temporary in (temporary_archive, temporary_manifest):
        if temporary.exists():
            temporary.unlink()

    try:
        with tarfile.open(
            temporary_archive,
            mode="w",
            format=tarfile.USTAR_FORMAT,
        ) as archive:
            archive.addfile(
                tar_info(ARCHIVE_MANIFEST_NAME, len(encoded_manifest)),
                fileobj=_BytesReader(encoded_manifest),
            )
            for record in files:
                source = source_root / record["path"]
                with source.open("rb") as handle:
                    archive.addfile(
                        tar_info(record["path"], record["size"]),
                        fileobj=handle,
                    )
        temporary_manifest.write_bytes(encoded_manifest)
        os.replace(temporary_archive, archive_path)
        os.replace(temporary_manifest, manifest_path)
    finally:
        for temporary in (temporary_archive, temporary_manifest):
            if temporary.exists():
                temporary.unlink()

    validate_archive(archive_path, manifest_path)
    return archive_path, manifest_path


class _BytesReader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.offset = 0

    def read(self, size: int = -1) -> bytes:
        if size < 0:
            size = len(self.data) - self.offset
        result = self.data[self.offset : self.offset + size]
        self.offset += len(result)
        return result


def parse_manifest(data: bytes) -> dict[str, Any]:
    document = json.loads(data)
    require(isinstance(document, dict), "archive manifest is not an object")
    require(document.get("schemaVersion") == SCHEMA_VERSION, "unexpected schema version")
    require(document.get("artifactID") == ARTIFACT_ID, "unexpected artifact id")
    require(document.get("modelPath") == MODEL_DIRECTORY, "unexpected model path")
    require(
        document.get("tokenizerPath") == TOKENIZER_DIRECTORY,
        "unexpected tokenizer path",
    )
    files = document.get("files")
    require(isinstance(files, list) and files, "archive manifest files are missing")
    paths: set[str] = set()
    for item in files:
        require(isinstance(item, dict), "archive manifest file entry is not an object")
        path = item.get("path")
        require(isinstance(path, str), "archive manifest file path is missing")
        validate_relative_path(path)
        require(path != ARCHIVE_MANIFEST_NAME, "manifest must not list itself")
        require(path not in paths, f"duplicate manifest file path: {path}")
        paths.add(path)
        size = item.get("size")
        digest = item.get("sha256")
        require(isinstance(size, int) and size >= 0, f"invalid size for {path}")
        require(
            isinstance(digest, str)
            and len(digest) == 64
            and all(character in "0123456789abcdef" for character in digest),
            f"invalid SHA-256 for {path}",
        )
    require(paths == set(PAYLOAD_PATHS), "manifest has an unexpected file set")
    require(
        document.get("canonicalTreeSHA256") == canonical_tree_sha256(files),
        "canonical tree SHA-256 does not match",
    )
    return document


def validate_relative_path(path: str) -> None:
    pure = PurePosixPath(path)
    require(path != "", "empty archive path")
    require("\\" not in path, f"archive path contains a backslash: {path}")
    require(not pure.is_absolute(), f"absolute archive path: {path}")
    require(
        all(part not in ("", ".", "..") for part in pure.parts),
        f"unsafe archive path: {path}",
    )
    require(pure.as_posix() == path, f"non-canonical archive path: {path}")


def validate_members(
    archive: tarfile.TarFile,
    expected: dict[str, dict[str, Any]],
) -> tuple[dict[str, tarfile.TarInfo], bytes]:
    observed: dict[str, tarfile.TarInfo] = {}
    embedded_manifest: bytes | None = None
    allowed = {ARCHIVE_MANIFEST_NAME, *expected.keys()}
    for member in archive:
        validate_relative_path(member.name)
        require(member.name not in observed, f"duplicate archive entry: {member.name}")
        require(member.name in allowed, f"unexpected archive entry: {member.name}")
        require(member.isreg(), f"archive entry is not a regular file: {member.name}")
        require(member.mode == REGULAR_MODE, f"unexpected mode for {member.name}")
        require(member.uid == 0 and member.gid == 0, f"unexpected owner for {member.name}")
        require(member.mtime == FIXED_MTIME, f"unexpected timestamp for {member.name}")
        observed[member.name] = member
        handle = archive.extractfile(member)
        require(handle is not None, f"archive entry cannot be read: {member.name}")
        if member.name == ARCHIVE_MANIFEST_NAME:
            embedded_manifest = handle.read()
            continue
        record = expected[member.name]
        require(member.size == record["size"], f"archive size mismatch: {member.name}")
        digest = hashlib.sha256()
        for chunk in iter(lambda: handle.read(COPY_BUFFER_SIZE), b""):
            digest.update(chunk)
        require(
            digest.hexdigest() == record["sha256"],
            f"archive SHA-256 mismatch: {member.name}",
        )
    require(set(observed) == allowed, "archive is missing expected entries")
    require(embedded_manifest is not None, "archive manifest entry is missing")
    return observed, embedded_manifest


def validate_archive(archive_path: Path, manifest_path: Path) -> dict[str, Any]:
    sidecar = manifest_path.read_bytes()
    document = parse_manifest(sidecar)
    expected = {item["path"]: item for item in document["files"]}
    with tarfile.open(archive_path, mode="r:") as archive:
        _, embedded = validate_members(archive, expected)
    require(embedded == sidecar, "embedded and sidecar manifests differ")
    return document


def extract_archive(archive_path: Path, manifest_path: Path, destination: Path) -> Path:
    document = validate_archive(archive_path, manifest_path)
    destination = destination.resolve()
    require(not destination.exists(), f"extraction destination already exists: {destination}")
    destination.mkdir(parents=True)
    expected = {item["path"]: item for item in document["files"]}
    try:
        with tarfile.open(archive_path, mode="r:") as archive:
            for member in archive:
                target = destination / member.name
                target.parent.mkdir(parents=True, exist_ok=True)
                handle = archive.extractfile(member)
                require(handle is not None, f"archive entry cannot be read: {member.name}")
                with target.open("xb") as output:
                    shutil.copyfileobj(handle, output, COPY_BUFFER_SIZE)
                target.chmod(REGULAR_MODE)
        extracted_manifest = destination / ARCHIVE_MANIFEST_NAME
        require(
            extracted_manifest.read_bytes() == manifest_path.read_bytes(),
            "extracted manifest differs from sidecar",
        )
        for path, record in expected.items():
            extracted = destination / path
            require(extracted.stat().st_size == record["size"], f"extracted size mismatch: {path}")
            require(
                sha256_file(extracted) == record["sha256"],
                f"extracted SHA-256 mismatch: {path}",
            )
    except Exception:
        shutil.rmtree(destination, ignore_errors=True)
        raise
    return destination


def compare_source_tree(source_root: Path, extracted_root: Path) -> None:
    source_files = validate_source(source_root)
    expected = {item["path"]: item for item in source_files}
    actual = {
        path.relative_to(extracted_root).as_posix()
        for path in extracted_root.rglob("*")
        if path.is_file() and path.name != ARCHIVE_MANIFEST_NAME
    }
    require(actual == set(expected), "extracted tree has unexpected or missing files")
    for relative, record in expected.items():
        extracted = extracted_root / relative
        require(extracted.stat().st_size == record["size"], f"tree size mismatch: {relative}")
        require(
            sha256_file(extracted) == record["sha256"],
            f"tree SHA-256 mismatch: {relative}",
        )


def verified_https_download(url: str, destination: Path) -> None:
    require(url.startswith("https://"), f"download URL must use HTTPS: {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "RetrievalKit-artifact-validator/1"})
    temporary = destination.with_name(f".{destination.name}.{os.getpid()}.tmp")
    if temporary.exists():
        temporary.unlink()
    try:
        with urllib.request.urlopen(request) as response, temporary.open("xb") as output:
            require(response.geturl().startswith("https://"), "download redirected away from HTTPS")
            shutil.copyfileobj(response, output, COPY_BUFFER_SIZE)
        os.replace(temporary, destination)
    finally:
        if temporary.exists():
            temporary.unlink()


def require_target_path(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    target = TARGET_DIR.resolve()
    if resolved != target and target not in resolved.parents:
        raise SystemExit(f"{label} must be inside {target}: {resolved}")
    return resolved


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    build = subparsers.add_parser("build", help="build and validate the deterministic archive")
    build.add_argument("--source-root", type=Path, required=True)
    build.add_argument("--output-dir", type=Path, required=True)

    validate = subparsers.add_parser("validate", help="validate and optionally extract an archive")
    validate.add_argument("--archive", type=Path, required=True)
    validate.add_argument("--manifest", type=Path, required=True)
    validate.add_argument("--extract-dir", type=Path)
    validate.add_argument("--compare-source-root", type=Path)

    download = subparsers.add_parser(
        "download-validate",
        help="download immutable HTTPS artifacts and validate them in a clean directory",
    )
    download.add_argument("--archive-url", required=True)
    download.add_argument("--manifest-url", required=True)
    download.add_argument("--output-dir", type=Path, required=True)
    download.add_argument("--compare-source-root", type=Path)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    if args.command == "build":
        archive, manifest = build_archive(args.source_root, args.output_dir)
        print(
            json.dumps(
                {
                    "archive": str(archive),
                    "archiveBytes": archive.stat().st_size,
                    "archiveSHA256": sha256_file(archive),
                    "manifest": str(manifest),
                    "manifestBytes": manifest.stat().st_size,
                    "manifestSHA256": sha256_file(manifest),
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if args.command == "validate":
        validate_archive(args.archive, args.manifest)
        if args.extract_dir:
            extracted = extract_archive(args.archive, args.manifest, args.extract_dir)
            if args.compare_source_root:
                compare_source_tree(args.compare_source_root, extracted)
        elif args.compare_source_root:
            raise SystemExit("--compare-source-root requires --extract-dir")
        print(f"Validated archive: {args.archive}")
        return 0

    output_dir = require_target_path(args.output_dir, "output directory")
    output_dir.mkdir(parents=True, exist_ok=True)
    archive = output_dir / ARCHIVE_NAME
    manifest = output_dir / ARCHIVE_MANIFEST_NAME
    verified_https_download(args.archive_url, archive)
    verified_https_download(args.manifest_url, manifest)
    extracted = output_dir / "extracted"
    extract_archive(archive, manifest, extracted)
    if args.compare_source_root:
        compare_source_tree(args.compare_source_root, extracted)
    print(f"Downloaded and validated archive: {archive}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
