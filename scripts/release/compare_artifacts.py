#!/usr/bin/env python3
"""Compare two closed release-artifact roots by path and SHA-256."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


def inventory(root: Path) -> dict[str, str]:
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def compare(first: Path, second: Path) -> None:
    left = inventory(first)
    right = inventory(second)
    if left.keys() != right.keys():
        missing = sorted(left.keys() - right.keys())
        extra = sorted(right.keys() - left.keys())
        raise ValueError(f"artifact inventories differ: missing={missing}, extra={extra}")
    changed = [name for name in left if left[name] != right[name]]
    if changed:
        raise ValueError(f"artifact bytes differ: {changed}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("first", type=Path)
    parser.add_argument("second", type=Path)
    args = parser.parse_args()
    compare(args.first.resolve(), args.second.resolve())
    print("two-root artifact inventories and bytes are identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
