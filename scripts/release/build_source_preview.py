#!/usr/bin/env python3
"""Build or verify the deterministic public RetrievalKit source preview."""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

DIRECTORY_NAME = "retrievalkit-python-source-preview"
DEFAULT_OUTPUT = Path("public/downloads/retrievalkit-python-source-preview.tar.gz")
ARCHIVE_PATHS = (
    "Cargo.toml",
    "Cargo.lock",
    "LICENSE",
    "NOTICE",
    "SOURCE_PREVIEW.md",
    "THIRD_PARTY_NOTICES.md",
    "VERSION",
    "benchmarks/graph-conformance/v1/fixture.json",
    "crates",
    "release/release-v0.1.0.json",
    "scripts/check-python-graph-wrapper.sh",
    "scripts/preflight-python-wrapper.sh",
    "scripts/release/build_source_preview.py",
    "wrappers/python-graph",
)
REQUIRED_MEMBERS = (
    "Cargo.toml",
    "LICENSE",
    "NOTICE",
    "SOURCE_PREVIEW.md",
    "THIRD_PARTY_NOTICES.md",
    "VERSION",
    "release/release-v0.1.0.json",
    "scripts/check-python-graph-wrapper.sh",
    "scripts/release/build_source_preview.py",
    "wrappers/python-graph/pyproject.toml",
    "wrappers/python-graph/examples/graph_retrieval_quickstart.py",
)


class PreviewError(RuntimeError):
    """The checked-in source preview differs from its declared source."""


def git(repo: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def build_archive(repo: Path, revision: str, output: Path) -> None:
    environment = os.environ.copy()
    environment["LC_ALL"] = "C"
    subprocess.run(
        [
            "git",
            "archive",
            "--format=tar.gz",
            f"--prefix={DIRECTORY_NAME}/",
            f"--output={output}",
            revision,
            "--",
            *ARCHIVE_PATHS,
        ],
        cwd=repo,
        env=environment,
        check=True,
    )


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_inventory(path: Path) -> None:
    prefix = f"{DIRECTORY_NAME}/"
    with tarfile.open(path, "r:gz") as archive:
        names = archive.getnames()
        if not names:
            raise PreviewError("source preview is empty")
        for name in names:
            pure = PurePosixPath(name)
            if (
                pure.is_absolute()
                or ".." in pure.parts
                or (name != DIRECTORY_NAME and not name.startswith(prefix))
            ):
                raise PreviewError(f"unsafe source preview member: {name}")
        missing = [
            relative
            for relative in REQUIRED_MEMBERS
            if f"{prefix}{relative}" not in names
        ]
        if missing:
            raise PreviewError(
                f"source preview is missing required members: {', '.join(missing)}"
            )
        if any(name.startswith(f"{prefix}website/") for name in names):
            raise PreviewError("source preview contains website build inputs")


def release_metadata(release_path: Path) -> tuple[str, str]:
    text = release_path.read_text(encoding="utf-8")
    revision = re.search(r'sourceRevision:\s*"([0-9a-f]{40})"', text)
    checksum = re.search(r'archiveSha256:\s*"([0-9a-f]{64})"', text)
    if revision is None or checksum is None:
        raise PreviewError("release.ts lacks a full revision or SHA-256 checksum")
    return revision.group(1), checksum.group(1)


def update_release_metadata(
    release_path: Path,
    revision: str,
    checksum: str,
) -> None:
    text = release_path.read_text(encoding="utf-8")
    text, revision_count = re.subn(
        r'sourceRevision:\s*"[0-9a-f]+"',
        f'sourceRevision: "{revision}"',
        text,
        count=1,
    )
    text, checksum_count = re.subn(
        r'archiveSha256:\s*"[0-9a-f]{64}"',
        f'archiveSha256: "{checksum}"',
        text,
        count=1,
    )
    if revision_count != 1 or checksum_count != 1:
        raise PreviewError("could not update release.ts metadata")
    release_path.write_text(text, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument(
        "--site-root",
        type=Path,
        required=True,
        help="Path to a gungorbasa/RetrievalKit-Website checkout",
    )
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument(
        "--check",
        action="store_true",
        help="regenerate from release.ts and compare without changing files",
    )
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[2]
    site_root = args.site_root.resolve()
    release_path = site_root / "app/release.ts"
    output = args.output if args.output.is_absolute() else site_root / args.output
    if not release_path.is_file():
        raise PreviewError(
            f"website release metadata is missing from site root: {release_path}"
        )

    if args.check:
        revision, expected_checksum = release_metadata(release_path)
        git(repo, "cat-file", "-e", f"{revision}^{{commit}}")
        if not output.is_file():
            raise PreviewError(f"source preview is missing: {output}")
        validate_inventory(output)
        observed_checksum = digest(output)
        if observed_checksum != expected_checksum:
            raise PreviewError(
                "source preview checksum differs from website release metadata"
            )
        with tempfile.TemporaryDirectory() as temporary:
            rebuilt = Path(temporary) / output.name
            build_archive(repo, revision, rebuilt)
            if rebuilt.read_bytes() != output.read_bytes():
                raise PreviewError(
                    "source preview bytes differ from a clean git archive rebuild"
                )
        print(f"verified {revision} sha256={observed_checksum}")
        return 0

    revision = git(repo, "rev-parse", f"{args.revision}^{{commit}}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        dir=output.parent,
        prefix=f".{output.name}.",
        suffix=".tmp",
        delete=False,
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        build_archive(repo, revision, temporary_path)
        validate_inventory(temporary_path)
        checksum = digest(temporary_path)
        os.replace(temporary_path, output)
    finally:
        temporary_path.unlink(missing_ok=True)
    update_release_metadata(release_path, revision, checksum)
    try:
        display_output = output.relative_to(site_root)
    except ValueError:
        display_output = output
    print(f"built {display_output}")
    print(f"revision={revision}")
    print(f"sha256={checksum}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
