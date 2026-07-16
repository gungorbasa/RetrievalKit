from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.quality import validate_v3_phase_1_2a as validator


class Phase12aIndependentValidatorTests(unittest.TestCase):
    def test_rejects_a_collection_outside_the_frozen_hash(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "collection.json").write_text("{}\n", encoding="utf-8")
            with self.assertRaisesRegex(validator.ValidationError, "frozen collection hash"):
                validator.verify_frozen_fixture(Path(directory))

    def test_rejects_a_ranking_or_identifier_difference(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "native_rank"):
            validator.assert_equal(
                [{"native_rank": 1, "record_id": "alpha"}],
                [{"native_rank": 2, "record_id": "alpha"}],
                "run.query.chunk_hits",
            )

    def test_existing_metric_formulas_use_fixed_precision_cutoff(self) -> None:
        documents = [{"record_id": "positive"}, {"record_id": "zero"}]
        metrics = validator.retrieval_metrics(documents, {"positive": 2, "zero": 0})
        self.assertEqual(metrics["precision_at_5"], 0.2)
        self.assertEqual(metrics["recall_at_5"], 1.0)
        self.assertEqual(metrics["ap"], 1.0)
        self.assertEqual(metrics["judged_at_5"], 1.0)
        self.assertEqual(metrics["ndcg_at_5"], 1.0)


if __name__ == "__main__":
    unittest.main()
