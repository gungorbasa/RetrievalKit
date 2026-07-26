#!/usr/bin/env python3
"""Build one explicit non-SDK PyPI ownership placeholder."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path
from zipfile import ZipFile


PROJECTS = {
    "retrievalkit": {
        "module": "retrievalkit_bootstrap_placeholder",
        "artifact": "retrievalkit",
    },
    "retrievalkit-graph": {
        "module": "retrievalkit_graph_bootstrap_placeholder",
        "artifact": "retrievalkit_graph",
    },
}
VERSION = "0.0.0a0"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", choices=sorted(PROJECTS), required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def write_source(repo: Path, project: str, source: Path) -> None:
    module = PROJECTS[project]["module"]
    source.mkdir(parents=True)
    (source / "pyproject.toml").write_text(
        f"""\
[build-system]
requires = ["setuptools==80.9.0"]
build-backend = "setuptools.build_meta"

[project]
name = "{project}"
version = "{VERSION}"
description = "Ownership bootstrap placeholder; not a RetrievalKit SDK release"
readme = "README.md"
requires-python = ">=3.10"
license = "Apache-2.0"
license-files = ["LICENSE", "NOTICE"]
authors = [
  {{ name = "EGGYOLK YAZILIM TİCARET LİMİTED ŞİRKETİ" }}
]
classifiers = [
  "Development Status :: 1 - Planning",
  "Programming Language :: Python :: 3",
]

[project.urls]
Repository = "https://github.com/gungorbasa/RetrievalKit"

[tool.setuptools]
py-modules = ["{module}"]
""",
        encoding="utf-8",
    )
    (source / "README.md").write_text(
        f"""\
# {project} ownership placeholder

This `{VERSION}` package only establishes the approved PyPI project identity.
It contains no RetrievalKit SDK and must not be installed.

The first SDK release remains gated and will use version `0.1.0`.
""",
        encoding="utf-8",
    )
    (source / f"{module}.py").write_text(
        '"""Ownership bootstrap placeholder; no SDK code."""\n',
        encoding="utf-8",
    )
    (source / "LICENSE").write_bytes((repo / "LICENSE").read_bytes())
    (source / "NOTICE").write_bytes((repo / "NOTICE").read_bytes())


def validate_artifacts(project: str, output: Path) -> None:
    artifact = PROJECTS[project]["artifact"]
    expected = {
        f"{artifact}-{VERSION}-py3-none-any.whl",
        f"{artifact}-{VERSION}.tar.gz",
    }
    actual = {path.name for path in output.iterdir() if path.is_file()}
    if actual != expected:
        raise SystemExit(f"unexpected PyPI bootstrap artifacts: {sorted(actual)}")

    wheel_path = output / f"{artifact}-{VERSION}-py3-none-any.whl"
    with ZipFile(wheel_path) as wheel:
        metadata_name = next(
            name for name in wheel.namelist() if name.endswith(".dist-info/METADATA")
        )
        metadata = wheel.read(metadata_name).decode("utf-8")
        names = set(wheel.namelist())
    required_metadata = (
        f"Name: {project}\n",
        f"Version: {VERSION}\n",
        "License-Expression: Apache-2.0\n",
    )
    for value in required_metadata:
        if value not in metadata:
            raise SystemExit(f"missing {value.strip()!r} in {wheel_path.name}")
    for license_name in ("LICENSE", "NOTICE"):
        if not any(name.endswith(f".dist-info/licenses/{license_name}") for name in names):
            raise SystemExit(f"missing {license_name} in {wheel_path.name}")


def main() -> None:
    args = parse_args()
    repo = Path(__file__).resolve().parents[2]
    target_root = (repo / "target").resolve()
    output = args.output.resolve()
    if output != target_root and target_root not in output.parents:
        raise SystemExit("--output must be inside the repository target directory")

    source = target_root / f"pypi-bootstrap-{PROJECTS[args.project]['artifact']}-src"
    shutil.rmtree(source, ignore_errors=True)
    shutil.rmtree(output, ignore_errors=True)
    write_source(repo, args.project, source)
    output.mkdir(parents=True)
    subprocess.run(
        [
            sys.executable,
            "-m",
            "build",
            "--outdir",
            str(output),
            str(source),
        ],
        check=True,
    )
    validate_artifacts(args.project, output)


if __name__ == "__main__":
    main()
