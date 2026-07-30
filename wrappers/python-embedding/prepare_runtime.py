"""Prepare the qualified ONNX Runtime library for a local wheel build."""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

FILENAME = "libonnxruntime.1.24.3.dylib"
EXPECTED_SIZE = 27_724_968
EXPECTED_SHA256 = "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"


def digest(path: Path) -> str:
    sha256 = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            sha256.update(block)
    return sha256.hexdigest()


def main() -> None:
    configured = os.environ.get("RETRIEVALKIT_ONNX_RUNTIME_LIBRARY")
    if configured is None:
        raise SystemExit("RETRIEVALKIT_ONNX_RUNTIME_LIBRARY is unset")
    source = Path(configured).resolve(strict=True)
    if source.name != FILENAME:
        raise SystemExit(f"expected runtime filename {FILENAME!r}")
    if source.stat().st_size != EXPECTED_SIZE or digest(source) != EXPECTED_SHA256:
        raise SystemExit("ONNX Runtime exact-size or SHA-256 verification failed")

    legal_files: dict[str, Path] = {}
    for name in ("LICENSE", "ThirdPartyNotices.txt"):
        candidates = (source.parent / name, source.parent.parent / name)
        candidate = next((path for path in candidates if path.is_file()), None)
        if candidate is None:
            raise SystemExit(
                f"required ONNX Runtime legal file {name!r} was not found beside "
                "the runtime or in its parent package directory"
            )
        legal_files[name] = candidate

    wheel_destination = (
        Path(__file__).parent
        / "wheel-data"
        / "platlib"
        / "retrievalkit_embedding"
        / "runtime"
    )
    wheel_destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, wheel_destination / FILENAME)
    for name, path in legal_files.items():
        shutil.copy2(path, wheel_destination / name)

    destination = (
        Path(__file__).parent / "python" / "retrievalkit_embedding" / "runtime"
    )
    destination.mkdir(parents=True, exist_ok=True)
    for name in (FILENAME, *legal_files):
        link = destination / name
        if link.exists() or link.is_symlink():
            link.unlink()
        target = Path(os.path.relpath(wheel_destination / name, destination))
        link.symlink_to(target)


if __name__ == "__main__":
    main()
