#!/usr/bin/env python3
"""Tests for the Kotlin/JVM embedding qualification driver."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("qualify-kotlin-embedding.py")
SPEC = importlib.util.spec_from_file_location("qualify_kotlin_embedding", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
QUALIFIER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = QUALIFIER
SPEC.loader.exec_module(QUALIFIER)


class KotlinEmbeddingQualificationTests(unittest.TestCase):
    def write_input(self, root: Path, items: list[dict[str, object]]) -> Path:
        path = root / "input.json"
        path.write_text(
            json.dumps({"schema_version": 1, "items": items}), encoding="utf-8"
        )
        return path

    def test_load_input_preserves_unicode_and_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_input(
                Path(directory),
                [
                    {"id": "first", "text": "İstanbul", "role": "corpus"},
                    {"id": "second", "text": "東京", "role": "query"},
                ],
            )
            self.assertEqual(
                QUALIFIER.load_input(path),
                [("first", "İstanbul"), ("second", "東京")],
            )

    def test_load_input_rejects_blank_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_input(
                Path(directory), [{"id": "blank", "text": "  ", "role": "diagnostic"}]
            )
            with self.assertRaisesRegex(QUALIFIER.QualificationError, "non-blank"):
                QUALIFIER.load_input(path)

    def test_load_input_rejects_duplicate_ids(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = self.write_input(
                Path(directory),
                [
                    {"id": "duplicate", "text": "a"},
                    {"id": "duplicate", "text": "b"},
                ],
            )
            with self.assertRaisesRegex(QUALIFIER.QualificationError, "duplicate"):
                QUALIFIER.load_input(path)

    def test_driver_fixes_required_sample_counts_and_local_only(self) -> None:
        self.assertIn("private static final int WARMUPS = 50", QUALIFIER.DRIVER_SOURCE)
        self.assertIn("private static final int MEASURED = 750", QUALIFIER.DRIVER_SOURCE)
        self.assertIn("OnnxEmbedder.prefetch(cache, localOnly)", QUALIFIER.DRIVER_SOURCE)
        self.assertIn("localOnly = true", QUALIFIER.DRIVER_SOURCE)
        self.assertIn(
            "OnnxEmbedder.load(localOnly, cache, runtime",
            QUALIFIER.DRIVER_SOURCE,
        )

    def test_explicit_native_paths_are_required_without_packaged_mode(self) -> None:
        parser = QUALIFIER.parser()
        arguments = parser.parse_args(
            [
                "--input",
                "input.json",
                "--output",
                "vectors.json",
                "--benchmark-output",
                "benchmark.json",
                "--embedding-jar",
                "embedding.jar",
                "--cache-directory",
                "cache",
            ]
        )
        self.assertFalse(arguments.packaged_libraries)
        self.assertIsNone(arguments.native_library)
        self.assertIsNone(arguments.runtime_library)


if __name__ == "__main__":
    unittest.main()
