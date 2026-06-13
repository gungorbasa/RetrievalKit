#!/usr/bin/env python3
from __future__ import annotations

import os
import sys
from pathlib import Path


def main() -> None:
    generic_converter = Path(__file__).with_name("convert-embedding-coreml.py")
    os.execv(
        sys.executable,
        [
            sys.executable,
            str(generic_converter),
            "--preset",
            "bge-small-en-v1.5",
            *sys.argv[1:],
        ],
    )


if __name__ == "__main__":
    main()
