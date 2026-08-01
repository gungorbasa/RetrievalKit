#!/usr/bin/env python3
"""Fail-closed tests for browser embedding release assembly."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("assemble_browser_embedding_package.py")
SPEC = importlib.util.spec_from_file_location(
    "assemble_browser_embedding_package", SCRIPT
)
assert SPEC and SPEC.loader
ASSEMBLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSEMBLER)


class BrowserEmbeddingPackageAssemblyTests(unittest.TestCase):
    def test_rejects_invalid_names_and_versions(self) -> None:
        with self.assertRaises(ASSEMBLER.AssemblyError):
            ASSEMBLER.validate_name("Browser Embedding")
        with self.assertRaises(ASSEMBLER.AssemblyError):
            ASSEMBLER.validate_version("v0.1.0")

    def test_requires_owner_approval(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "approval is unresolved"):
                ASSEMBLER.assemble(
                    name=ASSEMBLER.APPROVED_NAME,
                    version="0.1.0",
                    output=Path(root) / "output",
                    skip_build=True,
                )

    def test_rejects_other_valid_name(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "must be exactly"):
                ASSEMBLER.assemble(
                    name="@example/retrievalkit-browser-embedding",
                    version="0.1.0",
                    output=Path(root) / "output",
                    name_approved=True,
                    skip_build=True,
                )


if __name__ == "__main__":
    unittest.main()
