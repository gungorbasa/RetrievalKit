#!/usr/bin/env python3
"""Create a byte-stable zip archive from an XCFramework directory."""

from __future__ import annotations

import argparse
import stat
import zipfile
from pathlib import Path


FIXED_TIME = (1980, 1, 1, 0, 0, 0)


def archive(source: Path, output: Path) -> None:
    if not source.is_dir() or source.suffix != ".xcframework":
        raise ValueError(f"expected XCFramework directory: {source}")
    output.parent.mkdir(parents=True, exist_ok=True)
    root = source.parent
    entries = [source, *sorted(source.rglob("*"), key=lambda path: path.as_posix())]
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in entries:
            name = path.relative_to(root).as_posix()
            if path.is_dir():
                info = zipfile.ZipInfo(name + "/", FIXED_TIME)
                info.external_attr = (stat.S_IFDIR | 0o755) << 16
                info.compress_type = zipfile.ZIP_STORED
                bundle.writestr(info, b"")
                continue
            info = zipfile.ZipInfo(name, FIXED_TIME)
            mode = path.stat().st_mode & 0o777
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            bundle.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    archive(args.source.resolve(), args.output.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
