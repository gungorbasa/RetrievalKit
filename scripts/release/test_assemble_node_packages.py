#!/usr/bin/env python3
"""Determinism and fail-closed tests for Node release package assembly."""

from __future__ import annotations

import importlib.util
import json
import tarfile
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("assemble_node_packages.py")
SPEC = importlib.util.spec_from_file_location("assemble_node_packages", SCRIPT)
assert SPEC and SPEC.loader
ASSEMBLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSEMBLER)


class NodePackageAssemblyTests(unittest.TestCase):
    def test_rejects_provisional_or_invalid_public_names(self) -> None:
        for invalid in (
            "RetrievalKit",
            "@missing-package",
            "name with spaces",
        ):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ASSEMBLER.AssemblyError):
                    ASSEMBLER.validate_npm_name(invalid)

    def test_rejects_non_semver_versions(self) -> None:
        for invalid in ("0.1", "v0.1.0", "latest", "01.0.0"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ASSEMBLER.AssemblyError):
                    ASSEMBLER.validate_version(invalid)

    def test_requires_explicit_name_ownership_assertion(self) -> None:
        with tempfile.TemporaryDirectory(prefix="retrievalkit-node-name-gate-") as root:
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "ownership is unresolved"):
                ASSEMBLER.assemble(
                    base_name="@retrievalkit-release-test/core",
                    graph_name="@retrievalkit-release-test/graph",
                    version="0.1.0",
                    output=Path(root) / "output",
                    skip_native_build=True,
                    skip_typescript_build=True,
                )

    def test_rejects_other_syntactically_valid_names(self) -> None:
        with tempfile.TemporaryDirectory(prefix="retrievalkit-node-identity-gate-") as root:
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "must be exactly"):
                ASSEMBLER.assemble(
                    base_name="@example/retrievalkit",
                    graph_name="@example/retrievalkit-graph",
                    version="0.1.0",
                    output=Path(root) / "output",
                    names_approved=True,
                    skip_native_build=True,
                    skip_typescript_build=True,
                )

    @unittest.skipUnless(
        ASSEMBLER.platform.system() == "Darwin"
        and ASSEMBLER.platform.machine() in {"arm64", "aarch64"},
        "release target is macOS arm64",
    )
    def test_artifacts_are_deterministic_closed_and_publishable(self) -> None:
        with tempfile.TemporaryDirectory(prefix="retrievalkit-node-assembly-test-") as root:
            first = Path(root) / "first"
            second = Path(root) / "second"
            arguments = {
                "base_name": "@gungorbasa/retrievalkit",
                "graph_name": "@gungorbasa/retrievalkit-graph",
                "version": "0.1.0-test.1",
                "names_approved": True,
                "skip_native_build": True,
                "skip_typescript_build": True,
            }
            first_inventory = ASSEMBLER.assemble(output=first, **arguments)
            second_inventory = ASSEMBLER.assemble(output=second, **arguments)

            self.assertEqual(first_inventory, second_inventory)
            self.assertTrue(first_inventory["artifactReady"])
            self.assertFalse(first_inventory["publicationReady"])
            self.assertEqual(
                (first / "SHA256SUMS").read_bytes(),
                (second / "SHA256SUMS").read_bytes(),
            )
            self.assertEqual(
                (first / "SHA512SUMS").read_bytes(),
                (second / "SHA512SUMS").read_bytes(),
            )
            for artifact in first_inventory["artifacts"]:
                filename = artifact["file"]
                self.assertEqual((first / filename).read_bytes(), (second / filename).read_bytes())
                with tarfile.open(first / filename, "r:gz") as package:
                    metadata_file = package.extractfile("package/package.json")
                    self.assertIsNotNone(metadata_file)
                    assert metadata_file is not None
                    metadata = json.load(metadata_file)
                self.assertNotIn("private", metadata)
                self.assertEqual(metadata["license"], "Apache-2.0")
                self.assertEqual(metadata["os"], ["darwin"])
                self.assertEqual(metadata["cpu"], ["arm64"])
                self.assertEqual(
                    metadata["publishConfig"]["registry"],
                    "https://registry.npmjs.org/",
                )


if __name__ == "__main__":
    unittest.main()
