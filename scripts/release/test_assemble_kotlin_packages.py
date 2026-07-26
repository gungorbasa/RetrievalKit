#!/usr/bin/env python3
"""Determinism and fail-closed tests for Kotlin Maven package assembly."""

from __future__ import annotations

import importlib.util
import os
import tempfile
import unittest
import zipfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("assemble_kotlin_packages.py")
SPEC = importlib.util.spec_from_file_location("assemble_kotlin_packages", SCRIPT)
assert SPEC and SPEC.loader
ASSEMBLER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSEMBLER)


def find_jdk17() -> Path | None:
    configured = os.environ.get("RETRIEVALKIT_JAVA_HOME")
    candidates = [
        Path(configured) if configured else None,
        Path("/Applications/Android Studio.app/Contents/jbr/Contents/Home"),
    ]
    for candidate in candidates:
        if candidate is None:
            continue
        java = candidate / "bin" / "java"
        if java.is_file():
            result = ASSEMBLER.subprocess.run(
                [str(java), "-version"],
                text=True,
                capture_output=True,
                check=False,
            )
            if 'version "17.' in result.stderr:
                return candidate
    return None


JDK17 = find_jdk17()


class KotlinPackageAssemblyTests(unittest.TestCase):
    def test_rejects_placeholder_or_invalid_groups(self) -> None:
        for invalid in (
            "retrievalkit",
            "AI.RetrievalKit",
            "ai..retrievalkit",
            "ai/retrievalkit",
        ):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ASSEMBLER.AssemblyError):
                    ASSEMBLER.validate_group(invalid)

    def test_rejects_other_syntactically_valid_groups(self) -> None:
        with tempfile.TemporaryDirectory(prefix="retrievalkit-kotlin-identity-gate-") as root:
            with self.assertRaisesRegex(ASSEMBLER.AssemblyError, "must be exactly"):
                ASSEMBLER.assemble(
                    group="com.example.retrievalkit",
                    version="0.1.0",
                    output=Path(root) / "output",
                    skip_native_build=True,
                )

    @unittest.skipUnless(
        JDK17 is not None
        and ASSEMBLER.platform.system() == "Darwin"
        and ASSEMBLER.platform.machine() in {"arm64", "aarch64"},
        "JDK 17 plus the macOS arm64 release host are required",
    )
    def test_maven_layout_is_deterministic_and_capability_isolated(self) -> None:
        assert JDK17 is not None
        with tempfile.TemporaryDirectory(prefix="retrievalkit-kotlin-assembly-test-") as root:
            first = Path(root) / "first"
            second = Path(root) / "second"
            arguments = {
                "group": "io.github.gungorbasa",
                "version": "0.1.0-test.1",
                "java_home": JDK17,
                "skip_native_build": True,
            }
            first_inventory = ASSEMBLER.assemble(output=first, **arguments)
            second_inventory = ASSEMBLER.assemble(output=second, **arguments)

            self.assertEqual(first_inventory, second_inventory)
            self.assertFalse(first_inventory["publicationReady"])
            self.assertEqual(len(first_inventory["artifacts"]), 4)
            self.assertEqual(
                (first / "SHA256SUMS").read_bytes(),
                (second / "SHA256SUMS").read_bytes(),
            )
            first_files = sorted(
                path.relative_to(first) for path in first.rglob("*") if path.is_file()
            )
            second_files = sorted(
                path.relative_to(second) for path in second.rglob("*") if path.is_file()
            )
            self.assertEqual(first_files, second_files)
            for relative in first_files:
                self.assertEqual((first / relative).read_bytes(), (second / relative).read_bytes())
            bundle = first / first_inventory["bundle"]["file"]
            with zipfile.ZipFile(bundle) as archive:
                names = archive.namelist()
            self.assertTrue(any(name.endswith("-sources.jar") for name in names))
            self.assertTrue(any(name.endswith("-javadoc.jar") for name in names))
            self.assertTrue(any(name.endswith(".pom.sha512") for name in names))


if __name__ == "__main__":
    unittest.main()
