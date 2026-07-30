#!/usr/bin/env python3
"""Unit tests for the fail-closed Kotlin embedding runtime preparer."""

from __future__ import annotations

import hashlib
import importlib.util
import sys
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).with_name("prepare-embedding-runtime.py")
SPEC = importlib.util.spec_from_file_location("prepare_embedding_runtime", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
PREPARER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PREPARER
SPEC.loader.exec_module(PREPARER)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class PrepareEmbeddingRuntimeTests(unittest.TestCase):
    def test_macos_publishes_only_verified_runtime_and_legal_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / PREPARER.MACOS_RUNTIME
            runtime.write_bytes(b"runtime")
            license_path = root / "LICENSE"
            license_path.write_bytes(b"license")
            notices = root / "ThirdPartyNotices.txt"
            notices.write_bytes(b"notices")
            output = root / "published"

            with (
                mock.patch.object(PREPARER, "MACOS_RUNTIME_SIZE", 7),
                mock.patch.object(PREPARER, "MACOS_RUNTIME_SHA256", sha256(b"runtime")),
                mock.patch.dict(
                    PREPARER.LEGAL_FILES,
                    {
                        "LICENSE": (7, sha256(b"license")),
                        "ThirdPartyNotices.txt": (7, sha256(b"notices")),
                    },
                    clear=True,
                ),
            ):
                PREPARER.prepare_macos(
                    PREPARER.argparse.Namespace(
                        runtime=runtime,
                        license=license_path,
                        notices=notices,
                        output=output,
                    )
                )

            self.assertEqual(
                sorted(path.name for path in output.iterdir()),
                [
                    "ONNX-Runtime-LICENSE",
                    "ONNX-Runtime-ThirdPartyNotices.txt",
                    PREPARER.MACOS_RUNTIME,
                    "runtime-identity.txt",
                ],
            )
            self.assertEqual((output / PREPARER.MACOS_RUNTIME).read_bytes(), b"runtime")

    def test_hash_failure_does_not_replace_existing_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / PREPARER.MACOS_RUNTIME
            runtime.write_bytes(b"wrong")
            license_path = root / "LICENSE"
            license_path.write_bytes(b"license")
            notices = root / "ThirdPartyNotices.txt"
            notices.write_bytes(b"notices")
            output = root / "published"
            output.mkdir()
            (output / "existing").write_text("keep", encoding="utf-8")

            with (
                mock.patch.object(PREPARER, "MACOS_RUNTIME_SIZE", 5),
                mock.patch.object(PREPARER, "MACOS_RUNTIME_SHA256", "0" * 64),
            ):
                with self.assertRaisesRegex(PREPARER.RuntimeError, "SHA-256 mismatch"):
                    PREPARER.prepare_macos(
                        PREPARER.argparse.Namespace(
                            runtime=runtime,
                            license=license_path,
                            notices=notices,
                            output=output,
                        )
                    )
            self.assertEqual((output / "existing").read_text(encoding="utf-8"), "keep")

    def test_android_rejects_duplicate_entries_even_when_archive_is_pinned(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            aar = root / PREPARER.ANDROID_AAR
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(aar, "w") as archive:
                    archive.writestr(PREPARER.ANDROID_RUNTIME_ENTRY, b"runtime")
                    archive.writestr(PREPARER.ANDROID_RUNTIME_ENTRY, b"runtime")

            with (
                mock.patch.object(PREPARER, "ANDROID_AAR_SIZE", aar.stat().st_size),
                mock.patch.object(PREPARER, "ANDROID_AAR_SHA256", PREPARER.digest(aar)),
                mock.patch.object(
                    PREPARER, "ANDROID_AAR_SHA1", PREPARER.digest(aar, "sha1")
                ),
            ):
                with self.assertRaisesRegex(PREPARER.RuntimeError, "duplicate"):
                    PREPARER.extract_android_runtime(aar, root / "runtime.so")

    def test_android_rejects_path_traversal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            aar = root / PREPARER.ANDROID_AAR
            with zipfile.ZipFile(aar, "w") as archive:
                archive.writestr("../outside", b"bad")
                archive.writestr(PREPARER.ANDROID_RUNTIME_ENTRY, b"runtime")

            with (
                mock.patch.object(PREPARER, "ANDROID_AAR_SIZE", aar.stat().st_size),
                mock.patch.object(PREPARER, "ANDROID_AAR_SHA256", PREPARER.digest(aar)),
                mock.patch.object(
                    PREPARER, "ANDROID_AAR_SHA1", PREPARER.digest(aar, "sha1")
                ),
            ):
                with self.assertRaisesRegex(PREPARER.RuntimeError, "unsafe"):
                    PREPARER.extract_android_runtime(aar, root / "runtime.so")


if __name__ == "__main__":
    unittest.main()
