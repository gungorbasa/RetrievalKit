from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.quality import assemble_v3_publication as publication
from scripts.quality import validate_v3_conformance as foundation
from scripts.quality.validate_v3_phase_1_2a import ValidationError


class V3PublicationAssemblyTests(unittest.TestCase):
    def test_evidence_alternative_selection_uses_exact_contract_tiebreak(self) -> None:
        recall, complete, matched, required = publication.evidence_scores(
            ["d1", "d4"], [["d1", "d2"], ["d1", "d4"]], 5
        )
        self.assertEqual((recall, complete, matched, required), (1.0, 1.0, 2, 2))

    def test_macro_preserves_all_status_counts(self) -> None:
        rows = [
            {"metric": {"status": "valid", "value": 0.25}},
            {"metric": {"status": "not_applicable", "value": None}},
            {"metric": {"status": "undefined", "value": None}},
        ]
        value = publication.macro(rows, "metric")
        self.assertEqual(value["denominator"], 1)
        self.assertEqual(value["numerator"], 0.25)
        self.assertEqual(value["value"], 0.25)
        self.assertEqual(
            value["status_counts"],
            {
                "excluded_pre_freeze": 0,
                "invalid_execution": 0,
                "not_applicable": 1,
                "undefined": 1,
                "valid": 1,
            },
        )

    def test_release_context_changes_ids_and_preserves_logical_mapping(self) -> None:
        collection = Path("benchmarks/retrieval-quality/v3")
        files = publication.collection_files(collection)
        qualification = foundation.derive_runs(files)
        revision = {
            "binary_sha256": "a" * 64,
            "git_commit": "b" * 40,
            "source_sha256": None,
        }
        release = foundation.derive_runs(files, revision)
        self.assertNotEqual(
            [row["run_id"] for row in qualification], [row["run_id"] for row in release]
        )
        self.assertEqual(
            {row["logical_run_sha256"] for row in qualification},
            {row["logical_run_sha256"] for row in release},
        )

    def test_failed_gate_never_creates_public_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            qualification = root / "qualification"
            qualification.mkdir()
            publication.write_json(
                qualification / "qualification.json", {"status": "invalid_execution"}
            )
            output = root / "public"
            with self.assertRaisesRegex(ValidationError, "not valid"):
                publication.assemble(
                    Path("benchmarks/retrieval-quality/v3"),
                    qualification,
                    output,
                    foundation.implementation_revision(),
                    {},
                    {},
                )
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
