#!/usr/bin/env python3
"""Verify and stage the pinned ONNX Runtime 1.24.3 native libraries.

This tool never trusts a filename. It verifies the complete source artifact,
the selected native library, and the exact legal files before atomically
publishing a closed output directory.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import stat
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Callable

VERSION = "1.24.3"

MACOS_RUNTIME = "libonnxruntime.1.24.3.dylib"
MACOS_RUNTIME_SIZE = 27_724_968
MACOS_RUNTIME_SHA256 = "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"

ANDROID_AAR = "onnxruntime-android-1.24.3.aar"
ANDROID_AAR_URL = (
    "https://repo.maven.apache.org/maven2/com/microsoft/onnxruntime/"
    f"onnxruntime-android/{VERSION}/{ANDROID_AAR}"
)
ANDROID_AAR_SIZE = 40_948_335
ANDROID_AAR_SHA256 = "67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce"
ANDROID_AAR_SHA1 = "e17cad728482733e3787abaf2a0bbe1b8122ff8a"
ANDROID_RUNTIME_ENTRY = "jni/arm64-v8a/libonnxruntime.so"
ANDROID_RUNTIME = "libonnxruntime.so"
ANDROID_RUNTIME_SIZE = 25_831_632
ANDROID_RUNTIME_SHA256 = "4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad"

LEGAL_FILES = {
    "LICENSE": (
        1_073,
        "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c",
    ),
    "ThirdPartyNotices.txt": (
        325_054,
        "0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2",
    ),
}


class RuntimeError(ValueError):
    """The runtime input is not the immutable artifact that was qualified."""


@dataclass(frozen=True)
class Identity:
    size: int
    sha256: str


def digest(path: Path, algorithm: str = "sha256") -> str:
    value = hashlib.new(algorithm)
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def verify_file(path: Path, identity: Identity, label: str) -> None:
    try:
        size = path.stat().st_size
    except OSError as error:
        raise RuntimeError(f"{label} is unavailable at {path}: {error}") from error
    if not path.is_file():
        raise RuntimeError(f"{label} is not a regular file: {path}")
    if size != identity.size:
        raise RuntimeError(
            f"{label} size mismatch: expected {identity.size}, found {size}"
        )
    actual = digest(path)
    if actual != identity.sha256:
        raise RuntimeError(
            f"{label} SHA-256 mismatch: expected {identity.sha256}, found {actual}"
        )


def verify_legal(license_path: Path, notices_path: Path) -> None:
    for path, name in (
        (license_path, "LICENSE"),
        (notices_path, "ThirdPartyNotices.txt"),
    ):
        size, sha256 = LEGAL_FILES[name]
        verify_file(path, Identity(size, sha256), f"ONNX Runtime {name}")


def _safe_zip_members(archive: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    members: dict[str, zipfile.ZipInfo] = {}
    for member in archive.infolist():
        name = member.filename
        pure = PurePosixPath(name)
        if (
            not name
            or "\\" in name
            or pure.is_absolute()
            or ".." in pure.parts
            or "." in pure.parts
        ):
            raise RuntimeError(f"unsafe Android AAR entry: {name!r}")
        if name in members:
            raise RuntimeError(f"duplicate Android AAR entry: {name!r}")
        mode = member.external_attr >> 16
        if stat.S_ISLNK(mode):
            raise RuntimeError(f"unsafe link in Android AAR: {name!r}")
        members[name] = member
    return members


def extract_android_runtime(aar: Path, destination: Path) -> None:
    verify_file(
        aar,
        Identity(ANDROID_AAR_SIZE, ANDROID_AAR_SHA256),
        "official ONNX Runtime Android AAR",
    )
    if digest(aar, "sha1") != ANDROID_AAR_SHA1:
        raise RuntimeError("official ONNX Runtime Android AAR SHA-1 mismatch")
    try:
        with zipfile.ZipFile(aar) as archive:
            members = _safe_zip_members(archive)
            member = members.get(ANDROID_RUNTIME_ENTRY)
            if member is None or member.is_dir():
                raise RuntimeError(
                    f"Android AAR is missing {ANDROID_RUNTIME_ENTRY}"
                )
            with archive.open(member) as source, destination.open("wb") as output:
                shutil.copyfileobj(source, output)
    except (OSError, zipfile.BadZipFile) as error:
        raise RuntimeError(f"cannot extract verified Android AAR: {error}") from error
    verify_file(
        destination,
        Identity(ANDROID_RUNTIME_SIZE, ANDROID_RUNTIME_SHA256),
        "ONNX Runtime Android arm64-v8a library",
    )


def publish(
    output: Path,
    runtime_name: str,
    runtime_writer: Callable[[Path], None],
    license_path: Path,
    notices_path: Path,
) -> None:
    verify_legal(license_path, notices_path)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(
        tempfile.mkdtemp(prefix=f".{output.name}.partial-", dir=output.parent)
    )
    try:
        runtime_writer(temporary / runtime_name)
        shutil.copyfile(license_path, temporary / "ONNX-Runtime-LICENSE")
        shutil.copyfile(
            notices_path, temporary / "ONNX-Runtime-ThirdPartyNotices.txt"
        )
        marker = temporary / "runtime-identity.txt"
        marker.write_text(
            f"onnxruntime={VERSION}\n"
            f"runtime={runtime_name}\n"
            f"runtime_sha256={digest(temporary / runtime_name)}\n",
            encoding="utf-8",
            newline="\n",
        )
        for path in temporary.iterdir():
            os.chmod(path, 0o755 if path.name == runtime_name else 0o644)
        if output.exists():
            shutil.rmtree(output)
        os.replace(temporary, output)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise


def prepare_macos(args: argparse.Namespace) -> None:
    source = args.runtime.resolve()
    if source.name != MACOS_RUNTIME:
        raise RuntimeError(f"macOS runtime must be named {MACOS_RUNTIME}")
    verify_file(
        source,
        Identity(MACOS_RUNTIME_SIZE, MACOS_RUNTIME_SHA256),
        "ONNX Runtime macOS arm64 library",
    )
    publish(
        args.output.resolve(),
        MACOS_RUNTIME,
        lambda destination: shutil.copyfile(source, destination),
        args.license.resolve(),
        args.notices.resolve(),
    )


def prepare_android(args: argparse.Namespace) -> None:
    source = args.aar.resolve()
    publish(
        args.output.resolve(),
        ANDROID_RUNTIME,
        lambda destination: extract_android_runtime(source, destination),
        args.license.resolve(),
        args.notices.resolve(),
    )


def download_android(args: argparse.Namespace) -> None:
    destination = args.output.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(f".{destination.name}.{os.getpid()}.partial")
    request = urllib.request.Request(
        ANDROID_AAR_URL, headers={"User-Agent": "RetrievalKit-runtime-preparer/1"}
    )
    try:
        with urllib.request.urlopen(request) as response, partial.open("wb") as output:
            if not response.geturl().startswith("https://"):
                raise RuntimeError("Android AAR download redirected away from HTTPS")
            shutil.copyfileobj(response, output)
        verify_file(
            partial,
            Identity(ANDROID_AAR_SIZE, ANDROID_AAR_SHA256),
            "downloaded ONNX Runtime Android AAR",
        )
        if digest(partial, "sha1") != ANDROID_AAR_SHA1:
            raise RuntimeError("downloaded ONNX Runtime Android AAR SHA-1 mismatch")
        os.replace(partial, destination)
    finally:
        partial.unlink(missing_ok=True)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subparsers = result.add_subparsers(dest="command", required=True)

    macos = subparsers.add_parser("macos", help="stage the qualified macOS runtime")
    macos.add_argument("--runtime", type=Path, required=True)
    macos.add_argument("--license", type=Path, required=True)
    macos.add_argument("--notices", type=Path, required=True)
    macos.add_argument("--output", type=Path, required=True)
    macos.set_defaults(function=prepare_macos)

    android = subparsers.add_parser(
        "android", help="stage arm64-v8a from the pinned official Android AAR"
    )
    android.add_argument("--aar", type=Path, required=True)
    android.add_argument("--license", type=Path, required=True)
    android.add_argument("--notices", type=Path, required=True)
    android.add_argument("--output", type=Path, required=True)
    android.set_defaults(function=prepare_android)

    download = subparsers.add_parser(
        "download-android-aar",
        help="download the pinned official Android AAR with atomic publication",
    )
    download.add_argument("--output", type=Path, required=True)
    download.set_defaults(function=download_android)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.function(args)
    except RuntimeError as error:
        parser().error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
