#!/usr/bin/env python3
"""Focused tests for Kotlin embedding package architecture validation."""

from __future__ import annotations

import importlib.util
import struct
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("verify-embedding-package.py")
SPEC = importlib.util.spec_from_file_location("verify_embedding_package", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
VERIFIER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = VERIFIER
SPEC.loader.exec_module(VERIFIER)


class VerifyEmbeddingPackageTests(unittest.TestCase):
    def test_accepts_thin_arm64_macho(self) -> None:
        data = struct.pack("<III", 0xFEEDFACF, 0x0100000C, 0)
        VERIFIER.verify_macho_arm64(data, "test")

    def test_rejects_x86_64_macho(self) -> None:
        data = struct.pack("<III", 0xFEEDFACF, 0x01000007, 0)
        with self.assertRaisesRegex(VERIFIER.PackageError, "arm64"):
            VERIFIER.verify_macho_arm64(data, "test")

    def test_accepts_arm64_elf(self) -> None:
        data = bytearray(20)
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", data, 18, 183)
        VERIFIER.verify_elf_arm64(bytes(data), "test")

    def test_rejects_x86_64_elf(self) -> None:
        data = bytearray(20)
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", data, 18, 62)
        with self.assertRaisesRegex(VERIFIER.PackageError, "arm64"):
            VERIFIER.verify_elf_arm64(bytes(data), "test")


if __name__ == "__main__":
    unittest.main()
