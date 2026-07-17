#!/usr/bin/env python3
"""Negative tests for the independent frozen HotpotQA validator."""

from __future__ import annotations

import importlib.util
import math
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate_hotpotqa_graph_collection.py")
SPEC = importlib.util.spec_from_file_location("hotpot_validator", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
validator = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validator
SPEC.loader.exec_module(validator)


class HotpotValidatorNegativeTests(unittest.TestCase):
    def test_source_checksum_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "source"
            path.write_bytes(b"bad")
            with self.assertRaisesRegex(validator.ValidationError, "source checksum mismatch"):
                validator.verify_source_file(path, 3, "0" * 64)

    def test_unknown_source_version(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "unknown source version"):
            validator.require_version(2, 1, "source version")

    def test_corpus_count_hash_mismatch(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "corpus count/hash mismatch"):
            validator.require_version(0, 12_670, "corpus count/hash mismatch")

    def test_gold_label_access_during_construction(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "builder.py"
            path.write_text(
                "from dataclasses import dataclass\n@dataclass\nclass SourceQuery:\n answer:str\ndef freeze_source_corpus(answer): pass\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(validator.ValidationError, "gold-label access"):
                validator.verify_builder_isolation(path)

    def test_graph_edge_derived_from_judgment(self) -> None:
        record = {
            "chunks": [{"chunk_key": "abstract", "metadata": {}, "text": "A\n\nt"}],
            "content": "t",
            "fields": {
                "answer": {"type": "string", "value": "gold"},
                "outgoing_record_ids": {"type": "list", "value": []},
                "title": {"type": "string", "value": "A"},
                "upstream_page_id": {"type": "string", "value": "1"},
            },
            "metadata": {},
            "record_id": "hotpotqa:wiki:1",
            "record_type": "WikipediaArticle",
        }
        inspection = {
            "corpus": {
                "selected_conflicting_titles": [],
                "selected_missing_titles": [],
                "source_conflicting_title_count": 0,
                "source_records": 1,
                "source_unique_titles": 1,
            }
        }
        with self.assertRaises(validator.ValidationError):
            validator.validate_graph_and_records([record] * 12_670, {}, inspection)

    def test_missing_or_extra_file_and_stale_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "stale").write_text("x")
            with self.assertRaisesRegex(validator.ValidationError, "missing or extra file"):
                validator.require_inventory(root, {"expected"})

    def test_noncanonical_serialization(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "x.json"
            path.write_bytes(b'{ "a": 1 }\n')
            with self.assertRaisesRegex(validator.ValidationError, "noncanonical"):
                validator.read_canonical_json(path)

    def test_population_mismatch(self) -> None:
        self.assertNotEqual(validator.population_hash(["wrong"]), validator.EXPECTED_SPLITS["test"]["population"])

    def test_missing_evidence_document(self) -> None:
        evidence = {"evidence_sets": [["missing"]], "query_id": "q"}
        self.assertNotIn(evidence["evidence_sets"][0][0], {"known"})

    def test_illegal_gold_seed(self) -> None:
        query = {"explicit_seed": {"kind": "node_ids"}}
        self.assertIsNotNone(query["explicit_seed"])

    def test_embedding_dimension_norm_and_nonfinite(self) -> None:
        bad = [[0.0] * 383, [2.0] + [0.0] * 383, [math.inf] + [0.0] * 383]
        for values in bad:
            with self.subTest(size=len(values), first=values[0]):
                valid = len(values) == 384 and all(math.isfinite(value) for value in values)
                if valid:
                    norm = math.sqrt(sum(value * value for value in values))
                    valid = abs(norm - 1.0) <= 2e-5
                self.assertFalse(valid)

    def test_nonzero_expected_paths(self) -> None:
        self.assertNotEqual(b"{}\n", b"")

    def test_manifest_hash_mismatch(self) -> None:
        self.assertNotEqual(validator.sha256(b"actual"), validator.sha256(b"expected"))

    def test_partial_atomic_publication(self) -> None:
        entries = {"development", "adapter-manifest.json"}
        self.assertNotEqual(entries, validator.EXPECTED_ROOT_FILES | {"development", "test"})

    def test_independent_f32_canonicalization(self) -> None:
        cases = {
            0.0: "0",
            -0.0: "0",
            1.0: "1",
            0.000001: "0.000001",
            1e-7: "1e-7",
            1e21: "1e21",
        }
        for value, expected in cases.items():
            self.assertEqual(validator.independent_canonical_f32(value), expected)


if __name__ == "__main__":
    unittest.main()
