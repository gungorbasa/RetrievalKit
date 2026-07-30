#!/usr/bin/env python3
"""Fail-closed inspection for generated Kotlin embedding JARs and AARs."""

from __future__ import annotations

import argparse
import hashlib
import io
import stat
import struct
import zipfile
from pathlib import Path, PurePosixPath

from importlib.machinery import SourceFileLoader

PREPARER = SourceFileLoader(
    "prepare_embedding_runtime",
    str(Path(__file__).with_name("prepare-embedding-runtime.py")),
).load_module()


class PackageError(ValueError):
    """A generated embedding package violated its closed native inventory."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_inventory(archive: zipfile.ZipFile) -> dict[str, zipfile.ZipInfo]:
    result: dict[str, zipfile.ZipInfo] = {}
    for entry in archive.infolist():
        name = entry.filename
        pure = PurePosixPath(name)
        if (
            not name
            or "\\" in name
            or pure.is_absolute()
            or ".." in pure.parts
            or "." in pure.parts
        ):
            raise PackageError(f"unsafe package entry: {name!r}")
        if name in result:
            raise PackageError(f"duplicate package entry: {name!r}")
        if stat.S_ISLNK(entry.external_attr >> 16):
            raise PackageError(f"unsafe package link: {name!r}")
        result[name] = entry
    return result


def require_bytes(
    archive: zipfile.ZipFile,
    inventory: dict[str, zipfile.ZipInfo],
    name: str,
) -> bytes:
    entry = inventory.get(name)
    if entry is None or entry.is_dir():
        raise PackageError(f"package is missing required file {name}")
    return archive.read(entry)


def verify_identity(data: bytes, size: int, digest: str, label: str) -> None:
    if len(data) != size:
        raise PackageError(f"{label} size mismatch: expected {size}, found {len(data)}")
    actual = sha256(data)
    if actual != digest:
        raise PackageError(f"{label} SHA-256 mismatch: expected {digest}, found {actual}")


def verify_macho_arm64(data: bytes, label: str) -> None:
    if len(data) < 12:
        raise PackageError(f"{label} is too short to be Mach-O")
    magic, cpu_type = struct.unpack_from("<II", data)
    if magic != 0xFEEDFACF or cpu_type != 0x0100000C:
        raise PackageError(f"{label} is not a thin 64-bit arm64 Mach-O library")


def verify_elf_arm64(data: bytes, label: str) -> None:
    if (
        len(data) < 20
        or data[:4] != b"\x7fELF"
        or data[4] != 2
        or data[5] != 1
        or struct.unpack_from("<H", data, 18)[0] != 183
    ):
        raise PackageError(f"{label} is not a 64-bit little-endian arm64 ELF library")


def verify_common_legal(
    archive: zipfile.ZipFile,
    inventory: dict[str, zipfile.ZipInfo],
    project_license: Path,
    project_notice: Path,
) -> None:
    expected = {
        "LICENSE": project_license.read_bytes(),
        "NOTICE": project_notice.read_bytes(),
    }
    for name, data in expected.items():
        if require_bytes(archive, inventory, name) != data:
            raise PackageError(f"package {name} differs from the repository file")
    for packaged, source in (
        ("ONNX-Runtime-LICENSE", "LICENSE"),
        ("ONNX-Runtime-ThirdPartyNotices.txt", "ThirdPartyNotices.txt"),
    ):
        size, digest_value = PREPARER.LEGAL_FILES[source]
        verify_identity(
            require_bytes(archive, inventory, packaged),
            size,
            digest_value,
            f"packaged {packaged}",
        )


def verify_jvm(args: argparse.Namespace) -> None:
    try:
        with zipfile.ZipFile(args.archive) as archive:
            inventory = safe_inventory(archive)
            expected_native = {
                "native/macos-aarch64/libretrievalkit_embedding_jni.dylib",
                f"native/macos-aarch64/{PREPARER.MACOS_RUNTIME}",
            }
            actual_native = {
                name
                for name, entry in inventory.items()
                if name.startswith("native/") and not entry.is_dir()
            }
            if actual_native != expected_native:
                raise PackageError(
                    f"JVM native inventory mismatch: expected {sorted(expected_native)}, "
                    f"found {sorted(actual_native)}"
                )
            jni = require_bytes(
                archive,
                inventory,
                "native/macos-aarch64/libretrievalkit_embedding_jni.dylib",
            )
            runtime = require_bytes(
                archive,
                inventory,
                f"native/macos-aarch64/{PREPARER.MACOS_RUNTIME}",
            )
            verify_macho_arm64(jni, "packaged Kotlin embedding JNI library")
            verify_macho_arm64(runtime, "packaged ONNX Runtime library")
            verify_identity(
                runtime,
                PREPARER.MACOS_RUNTIME_SIZE,
                PREPARER.MACOS_RUNTIME_SHA256,
                "packaged ONNX Runtime macOS library",
            )
            verify_common_legal(
                archive, inventory, args.project_license, args.project_notice
            )
    except (OSError, zipfile.BadZipFile) as error:
        raise PackageError(f"cannot inspect JVM package: {error}") from error


def verify_android(args: argparse.Namespace) -> None:
    try:
        with zipfile.ZipFile(args.archive) as archive:
            inventory = safe_inventory(archive)
            expected_native = {
                "jni/arm64-v8a/libretrievalkit_embedding_jni.so",
                "jni/arm64-v8a/libonnxruntime.so",
            }
            actual_native = {
                name
                for name, entry in inventory.items()
                if name.startswith("jni/") and not entry.is_dir()
            }
            if actual_native != expected_native:
                raise PackageError(
                    f"Android native inventory mismatch: expected {sorted(expected_native)}, "
                    f"found {sorted(actual_native)}"
                )
            jni = require_bytes(
                archive,
                inventory,
                "jni/arm64-v8a/libretrievalkit_embedding_jni.so",
            )
            runtime = require_bytes(
                archive, inventory, "jni/arm64-v8a/libonnxruntime.so"
            )
            verify_elf_arm64(jni, "packaged Kotlin embedding JNI library")
            verify_elf_arm64(runtime, "packaged ONNX Runtime library")
            verify_identity(
                runtime,
                PREPARER.ANDROID_RUNTIME_SIZE,
                PREPARER.ANDROID_RUNTIME_SHA256,
                "packaged ONNX Runtime Android arm64-v8a library",
            )
            classes = require_bytes(archive, inventory, "classes.jar")
            try:
                with zipfile.ZipFile(io.BytesIO(classes)) as classes_archive:
                    classes_inventory = safe_inventory(classes_archive)
                    verify_common_legal(
                        classes_archive,
                        classes_inventory,
                        args.project_license,
                        args.project_notice,
                    )
            except zipfile.BadZipFile as error:
                raise PackageError(f"AAR classes.jar is invalid: {error}") from error
    except (OSError, zipfile.BadZipFile) as error:
        raise PackageError(f"cannot inspect Android package: {error}") from error


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    for name, function in (("jvm", verify_jvm), ("android", verify_android)):
        command = commands.add_parser(name)
        command.add_argument("--archive", type=Path, required=True)
        command.add_argument("--project-license", type=Path, required=True)
        command.add_argument("--project-notice", type=Path, required=True)
        command.set_defaults(function=function)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.function(args)
    except PackageError as error:
        parser().error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
