from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

import numpy as np


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/embedding/qualify-minilm-i8-storage-policy.py"


def load_qualifier():
    spec = importlib.util.spec_from_file_location("minilm_i8_policy", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


qualifier = load_qualifier()


class MiniLMI8StoragePolicyTests(unittest.TestCase):
    def test_rust_rounding_uses_half_away_from_zero(self) -> None:
        values = np.asarray(
            [-2.5, -1.5, -0.5, -0.49, 0.0, 0.49, 0.5, 1.5, 2.5],
            dtype=np.float32,
        )
        actual = qualifier.rust_round_f32(values, np)
        np.testing.assert_array_equal(
            actual,
            np.asarray([-3, -2, -1, 0, 0, 0, 1, 2, 3], dtype=np.float32),
        )

    def test_i8_encoding_matches_rust_scale_rounding_and_zero_vector(self) -> None:
        vectors = np.asarray(
            [
                [127.0, 0.5, -0.5, -127.0],
                [0.0, 0.0, 0.0, 0.0],
            ],
            dtype=np.float32,
        )
        encoded = qualifier.encode_i8_vectors(vectors, np)
        np.testing.assert_array_equal(
            encoded.values,
            np.asarray([[127, 1, -1, -127], [0, 0, 0, 0]], dtype=np.int8),
        )
        np.testing.assert_array_equal(
            encoded.scales,
            np.asarray([1.0, 0.0], dtype=np.float32),
        )

    def test_i8_scoring_rescales_query_and_database_vectors(self) -> None:
        database = qualifier.I8Vectors(
            values=np.asarray([[2, -1], [1, 3]], dtype=np.int8),
            scales=np.asarray([0.5, 0.25], dtype=np.float32),
        )
        queries = qualifier.I8Vectors(
            values=np.asarray([[4, 2]], dtype=np.int8),
            scales=np.asarray([0.125], dtype=np.float32),
        )
        scores = qualifier.score_i8(database, queries, np)
        # Integer dots are 6 and 10. Rust rescales query first, then database.
        np.testing.assert_array_equal(
            scores,
            np.asarray([[0.375, 0.3125]], dtype=np.float32),
        )

    def test_ranking_metrics_report_mean_exact_and_minimum_overlap(self) -> None:
        reference = np.asarray(
            [
                list(range(12, 0, -1)),
                list(range(12, 0, -1)),
            ],
            dtype=np.float32,
        )
        candidate = reference.copy()
        candidate[1, 9] = 0.0
        candidate[1, 10] = 4.0
        metrics = qualifier.ranking_overlap_metrics(
            reference,
            candidate,
            top_k=10,
            np=np,
        )
        self.assertEqual(
            metrics,
            {
                "mean_top10_overlap": 0.95,
                "exact_top10_fraction": 0.5,
                "minimum_top10_overlap": 0.9,
            },
        )

    def test_stable_top_k_breaks_score_ties_by_index(self) -> None:
        scores = np.asarray([1.0, 3.0, 3.0, 2.0], dtype=np.float32)
        self.assertEqual(qualifier.stable_top_k(scores, 3), [1, 2, 3])

    def test_policy_report_names_both_directions_and_references(self) -> None:
        corpus = np.eye(12, dtype=np.float32)
        queries = np.stack((corpus[0], corpus[1]))
        diagnostics = np.stack((corpus[2], corpus[3]))
        vectors = np.concatenate((corpus, queries, diagnostics), axis=0)
        report = qualifier.analyze_policy(
            onnx_vectors=vectors,
            coreml_vectors=vectors.copy(),
            corpus_count=12,
            query_count=2,
            np=np,
            top_k=10,
        )
        self.assertTrue(report["passed"])
        self.assertEqual(
            report["direct_fp32_comparison"]["name"],
            "direct_fp32_onnx_vs_coreml",
        )
        directions = report["database_directions"]
        self.assertEqual(
            [item["name"] for item in directions],
            [
                "onnx_database_coreml_queries",
                "coreml_database_onnx_queries",
            ],
        )
        self.assertEqual(
            directions[0]["reference"],
            {
                "database": "onnx_fp32",
                "queries": "onnx_fp32",
                "scoring": "f32_dot",
            },
        )
        self.assertEqual(
            directions[1]["reference"],
            {
                "database": "coreml_fp32",
                "queries": "coreml_fp32",
                "scoring": "f32_dot",
            },
        )
        for section in [report["direct_fp32_comparison"], *directions]:
            self.assertEqual(section["median_cosine"], 1.0)
            self.assertEqual(section["mean_top10_overlap"], 1.0)
            self.assertEqual(section["exact_top10_fraction"], 1.0)
            self.assertEqual(section["minimum_top10_overlap"], 1.0)

    def test_policy_rejects_mismatched_provider_shapes(self) -> None:
        with self.assertRaisesRegex(ValueError, "provider vector shapes differ"):
            qualifier.analyze_policy(
                onnx_vectors=np.zeros((12, 4), dtype=np.float32),
                coreml_vectors=np.zeros((13, 4), dtype=np.float32),
                corpus_count=10,
                query_count=2,
                np=np,
            )


if __name__ == "__main__":
    unittest.main()
