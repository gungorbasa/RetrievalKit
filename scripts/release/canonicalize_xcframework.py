#!/usr/bin/env python3
"""Canonicalize order-sensitive XCFramework metadata."""

from __future__ import annotations

import argparse
import plistlib
from pathlib import Path


def canonicalize(path: Path) -> None:
    info = path / "Info.plist"
    value = plistlib.loads(info.read_bytes())
    libraries = value.get("AvailableLibraries")
    if not isinstance(libraries, list):
        raise ValueError(f"XCFramework has no AvailableLibraries array: {path}")
    value["AvailableLibraries"] = sorted(libraries, key=lambda row: row["LibraryIdentifier"])
    info.write_bytes(plistlib.dumps(value, fmt=plistlib.FMT_XML, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("xcframework", type=Path)
    args = parser.parse_args()
    canonicalize(args.xcframework.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
