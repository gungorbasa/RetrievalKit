from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from phase5_common import (  # noqa: E402
    distribution,
    generate_workload,
    oracle_results,
    result_identity,
)


class CommonTests(unittest.TestCase):
    def setUp(self) -> None:
        self.spec = {
            "active_chunks": 256,
            "deleted_chunks": 4,
            "dimension": 32,
            "query_count": 4,
            "seed": 5001,
            "workload_id": "256-32d-smoke-v1",
        }

    def test_generator_is_deterministic(self) -> None:
        first = generate_workload(self.spec)
        second = generate_workload(self.spec)
        self.assertEqual(first.input_manifest, second.input_manifest)
        self.assertEqual(first.vectors.tobytes(), second.vectors.tobytes())
        self.assertEqual(first.queries.tobytes(), second.queries.tobytes())

    def test_oracle_excludes_deleted_and_obeys_filter(self) -> None:
        data = generate_workload(self.spec)
        deleted = {
            f"chunk-{value:08d}"
            for value in range(data.active_chunks, data.total_chunks)
        }
        for row in oracle_results(data, filtered=False):
            self.assertFalse(deleted.intersection(row["result_ids"]))
        for row, query in zip(
            oracle_results(data, filtered=True), data.query_specs, strict=True
        ):
            tenant = int(query.tenant.removeprefix("tenant-"))
            for result in row["result_ids"]:
                ordinal = int(result.removeprefix("chunk-"))
                self.assertEqual(ordinal % 10, tenant)

    def test_nearest_rank_distribution(self) -> None:
        self.assertEqual(
            distribution([5, 1, 4, 2, 3]),
            {
                "max_ns": 5,
                "mean_ns": 3,
                "min_ns": 1,
                "p50_ns": 3,
                "p95_ns": 5,
                "p99_ns": 5,
                "percentile_method": "nearest_rank",
                "sample_count": 5,
            },
        )

    def test_result_identity_is_order_sensitive(self) -> None:
        self.assertNotEqual(result_identity(["a", "b"]), result_identity(["b", "a"]))


if __name__ == "__main__":
    unittest.main()
