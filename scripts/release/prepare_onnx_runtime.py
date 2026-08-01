#!/usr/bin/env python3
"""Download and verify the exact ONNX Runtime inputs used by release packages."""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import tempfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


VERSION = "1.24.3"
MACOS_WHEEL_URL = (
    "https://files.pythonhosted.org/packages/15/41/"
    "3253db975a90c3ce1d475e2a230773a21cd7998537f0657947df6fb79861/"
    f"onnxruntime-{VERSION}-cp311-cp311-macosx_14_0_arm64.whl"
)
MACOS_WHEEL_SIZE = 17_332_766
MACOS_WHEEL_SHA256 = (
    "3e6456801c66b095c5cd68e690ca25db970ea5202bd0c5b84a2c3ef7731c5a3c"
)
MACOS_RUNTIME = f"libonnxruntime.{VERSION}.dylib"
MACOS_RUNTIME_SIZE = 27_724_968
MACOS_RUNTIME_SHA256 = (
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"
)
ANDROID_AAR_URL = (
    "https://repo.maven.apache.org/maven2/com/microsoft/onnxruntime/"
    f"onnxruntime-android/{VERSION}/onnxruntime-android-{VERSION}.aar"
)
ANDROID_AAR_SIZE = 40_948_335
ANDROID_AAR_SHA256 = (
    "67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce"
)
LEGAL_IDENTITIES = {
    "LICENSE": (
        1_073,
        "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c",
    ),
    "ThirdPartyNotices.txt": (
        325_054,
        "0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2",
    ),
}
MAX_DOWNLOAD_BYTES = 200 * 1024 * 1024


class RuntimePreparationError(RuntimeError):
    """A remote runtime input did not match the qualified identity."""


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def verify(path: Path, *, size: int, sha256: str, label: str) -> None:
    if not path.is_file():
        raise RuntimePreparationError(f"{label} is missing: {path}")
    if path.stat().st_size != size or digest(path) != sha256:
        raise RuntimePreparationError(f"{label} exact-size or SHA-256 check failed")


def download(url: str, destination: Path) -> None:
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "RetrievalKit-release-runtime-preparer/1"},
    )
    with urllib.request.urlopen(request) as response, destination.open("wb") as output:
        if not response.geturl().startswith("https://"):
            raise RuntimePreparationError("runtime download redirected away from HTTPS")
        total = 0
        while chunk := response.read(1024 * 1024):
            total += len(chunk)
            if total > MAX_DOWNLOAD_BYTES:
                raise RuntimePreparationError("runtime download exceeded the size limit")
            output.write(chunk)


def safe_member(name: str) -> bool:
    pure = PurePosixPath(name)
    return (
        bool(name)
        and "\\" not in name
        and not pure.is_absolute()
        and ".." not in pure.parts
    )


def extract_member(
    archive: zipfile.ZipFile,
    members: list[str],
    suffix: str,
    destination: Path,
) -> None:
    matches = [member for member in members if member.endswith(suffix)]
    if len(matches) != 1:
        raise RuntimePreparationError(
            f"macOS runtime wheel expected one {suffix!r}, found {len(matches)}"
        )
    with archive.open(matches[0]) as source, destination.open("wb") as output:
        shutil.copyfileobj(source, output)


def prepare_macos(output: Path) -> None:
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="retrievalkit-onnx-runtime-", dir=output.parent
    ) as temporary:
        staging = Path(temporary)
        archive_path = staging / "runtime.whl"
        download(MACOS_WHEEL_URL, archive_path)
        verify(
            archive_path,
            size=MACOS_WHEEL_SIZE,
            sha256=MACOS_WHEEL_SHA256,
            label="official ONNX Runtime macOS arm64 wheel",
        )
        try:
            with zipfile.ZipFile(archive_path) as archive:
                members = archive.namelist()
                if not members or not all(safe_member(member) for member in members):
                    raise RuntimePreparationError("macOS runtime wheel has an unsafe inventory")
                extract_member(
                    archive,
                    members,
                    f"/capi/{MACOS_RUNTIME}",
                    staging / MACOS_RUNTIME,
                )
                for name in LEGAL_IDENTITIES:
                    extract_member(archive, members, f"/{name}", staging / name)
        except zipfile.BadZipFile as error:
            raise RuntimePreparationError(f"cannot inspect macOS runtime wheel: {error}") from error

        verify(
            staging / MACOS_RUNTIME,
            size=MACOS_RUNTIME_SIZE,
            sha256=MACOS_RUNTIME_SHA256,
            label="macOS arm64 ONNX Runtime",
        )
        for name, (size, sha256) in LEGAL_IDENTITIES.items():
            verify(
                staging / name,
                size=size,
                sha256=sha256,
                label=f"ONNX Runtime {name}",
            )
        archive_path.unlink()
        if output.exists():
            shutil.rmtree(output)
        os.replace(staging, output)


def prepare_android_aar(output: Path) -> None:
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    partial = output.with_name(f".{output.name}.{os.getpid()}.partial")
    try:
        download(ANDROID_AAR_URL, partial)
        verify(
            partial,
            size=ANDROID_AAR_SIZE,
            sha256=ANDROID_AAR_SHA256,
            label="official ONNX Runtime Android AAR",
        )
        os.replace(partial, output)
    finally:
        partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    macos = commands.add_parser("macos")
    macos.add_argument("--output", type=Path, required=True)
    android = commands.add_parser("android-aar")
    android.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "macos":
            prepare_macos(args.output)
        else:
            prepare_android_aar(args.output)
    except (OSError, RuntimePreparationError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
