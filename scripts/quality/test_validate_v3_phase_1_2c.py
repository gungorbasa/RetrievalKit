from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.quality import validate_v3_phase_1_2c as validator


class Phase12cIndependentValidatorTests(unittest.TestCase):
    def test_reconstructs_all_frozen_graph_scoped_runs_without_rust_inputs(self) -> None:
        expected = validator.execute_expected(validator.DEFAULT_COLLECTION)

        self.assertEqual(len(expected["runs"]), 9)
        self.assertEqual(sum(len(run["selections"]) for run in expected["runs"]), 15)
        self.assertEqual(sum(len(run["paths"]) for run in expected["runs"]), 33)
        self.assertEqual(len(expected["paired"]), 9)
        self.assertEqual(len(expected["fingerprints"]["fingerprints"]), 3)

    def test_numeric_comparison_enforces_the_frozen_tolerance(self) -> None:
        differences = {"maximum_numeric": 0.0}
        validator.assert_structure(1.0, 1.0 + 1.0e-8, "score", differences, 2.0e-7)
        self.assertGreater(differences["maximum_numeric"], 0.0)
        with self.assertRaisesRegex(validator.ValidationError, "difference"):
            validator.assert_structure(1.0, 1.001, "score", differences, 2.0e-7)

    def test_report_is_canonical_and_refuses_overwrite(self) -> None:
        report = {"status": "passed", "partial": True}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / validator.REPORT_NAME
            validator.write_report(path, report)
            self.assertEqual(path.read_bytes(), b'{"partial":true,"status":"passed"}\n')
            with self.assertRaisesRegex(validator.ValidationError, "refusing to overwrite"):
                validator.write_report(path, report)


if __name__ == "__main__":
    unittest.main()
