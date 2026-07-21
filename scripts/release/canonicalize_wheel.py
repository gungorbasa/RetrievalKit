#!/usr/bin/env python3
"""Remove checkout paths and rewrite a wheel with canonical bytes."""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import io
import json
import os
import stat
import tempfile
import zipfile
from pathlib import Path
from typing import Any


FIXED_TIME = (1980, 1, 1, 0, 0, 0)


def normalize_paths(value: Any, root_uri: str) -> Any:
    if isinstance(value, str):
        return value.replace(root_uri, "path+file:///workspace")
    if isinstance(value, list):
        return [normalize_paths(item, root_uri) for item in value]
    if isinstance(value, dict):
        return {key: normalize_paths(item, root_uri) for key, item in value.items()}
    return value


def record_bytes(files: dict[str, bytes], record_name: str) -> bytes:
    stream = io.StringIO(newline="")
    writer = csv.writer(stream, lineterminator="\n")
    for name in sorted(files):
        if name == record_name:
            continue
        payload = files[name]
        encoded = base64.urlsafe_b64encode(hashlib.sha256(payload).digest()).rstrip(b"=").decode()
        writer.writerow((name, f"sha256={encoded}", len(payload)))
    writer.writerow((record_name, "", ""))
    return stream.getvalue().encode()


def canonicalize(repo: Path, wheel: Path) -> None:
    with zipfile.ZipFile(wheel) as source:
        infos = {info.filename: info for info in source.infolist() if not info.is_dir()}
        files = {name: source.read(name) for name in infos}
    sboms = [name for name in files if ".dist-info/sboms/" in name and name.endswith(".json")]
    if len(sboms) != 1:
        raise ValueError(f"expected exactly one embedded wheel SBOM: {wheel.name}")
    root_uri = "path+file://" + repo.resolve().as_posix()
    sbom = normalize_paths(json.loads(files[sboms[0]]), root_uri)
    files[sboms[0]] = (json.dumps(sbom, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if root_uri.encode() in files[sboms[0]]:
        raise ValueError(f"wheel SBOM still contains checkout path: {wheel.name}")
    records = [name for name in files if name.endswith(".dist-info/RECORD")]
    if len(records) != 1:
        raise ValueError(f"expected exactly one wheel RECORD: {wheel.name}")
    files[records[0]] = record_bytes(files, records[0])

    descriptor, temporary_name = tempfile.mkstemp(prefix=wheel.name + ".", dir=wheel.parent)
    os.close(descriptor)
    temporary = Path(temporary_name)
    try:
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
            for name in sorted(files):
                original = infos[name]
                info = zipfile.ZipInfo(name, FIXED_TIME)
                mode = (original.external_attr >> 16) & 0o777
                info.external_attr = (stat.S_IFREG | (mode or 0o644)) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                output.writestr(info, files[name], compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
        temporary.replace(wheel)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("wheel", type=Path)
    args = parser.parse_args()
    canonicalize(args.repo, args.wheel)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
