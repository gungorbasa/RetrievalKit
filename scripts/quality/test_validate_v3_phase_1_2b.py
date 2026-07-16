from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.quality import validate_v3_phase_1_2b as validator


class Phase12bIndependentValidatorTests(unittest.TestCase):
    def test_reconstructs_frozen_run_d_without_rust_inputs(self) -> None:
        model = validator.load_collection(validator.DEFAULT_COLLECTION)
        runs = validator.frozen_d_runs(validator.DEFAULT_COLLECTION)
        _, fingerprint = validator.generation_fingerprint(
            validator.DEFAULT_COLLECTION, model["collection"]
        )

        result = validator.execute(model, runs, fingerprint)

        self.assertEqual(len(result["runs"]), 3)
        self.assertEqual(sum(len(run["selections"]) for run in result["runs"]), 7)
        self.assertEqual(sum(len(run["paths"]) for run in result["runs"]), 14)
        self.assertEqual(len(result["diagnostics"]), 6)

    def test_report_is_canonical_and_refuses_overwrite(self) -> None:
        report = {"status": "passed", "partial": True}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / validator.REPORT_NAME
            validator.write_report(path, report)
            self.assertEqual(path.read_bytes(), b'{"partial":true,"status":"passed"}\n')
            with self.assertRaisesRegex(validator.ValidationError, "refusing to overwrite"):
                validator.write_report(path, report)

    def test_exact_comparison_rejects_candidate_drift(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "candidate mismatch"):
            validator.assert_exact(
                [{"chunk_key": "summary", "record_id": "alpha"}],
                [{"chunk_key": "summary", "record_id": "beta"}],
                "candidate",
            )


if __name__ == "__main__":
    unittest.main()
