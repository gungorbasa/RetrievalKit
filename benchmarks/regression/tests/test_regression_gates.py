from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from typing import Any, cast

ROOT = Path(__file__).resolve().parents[3]
REGRESSION = ROOT / "benchmarks/regression"


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


runner = load_module("phase7_runner", REGRESSION / "run_gates.py")
validator = load_module("phase7_validator", REGRESSION / "validate_gates.py")


class RegressionGateMutationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_observation(self, name: str, value: dict[str, Any]) -> Path:
        path = self.root / name
        path.write_bytes(runner.canonical_bytes(value))
        return path

    def evaluate(self, tier: str, observation: dict[str, Any]) -> dict[str, Any]:
        return cast(
            dict[str, Any],
            runner.evaluate(
                SimpleNamespace(
                    tier=tier,
                    observation=self.write_observation(f"{tier}.json", observation),
                    repo=ROOT,
                    source_revision="0123456789abcdef0123456789abcdef01234567",
                )
            ),
        )

    def pr_observation(self) -> dict[str, Any]:
        return cast(
            dict[str, Any],
            json.loads(
                (REGRESSION / "fixtures/expected-observation-v1.json").read_text()
            ),
        )

    def tier_observation(self, tier: str) -> dict[str, Any]:
        registry = json.loads((REGRESSION / "gate-registry-v1.json").read_text())
        gates = [gate for gate in registry["gates"] if gate["tier"] == tier]
        metrics: dict[str, Any] = {}
        provisioned: set[str] = set()
        for gate in gates:
            operator = gate["threshold"]["operator"]
            value = gate["threshold"]["value"]
            metrics[gate["metric"]] = value if operator != "eq" else value
            provisioned.update(gate["required_inputs"])
        baseline = json.loads((REGRESSION / "baselines-v1.json").read_text())
        return {
            "inputs": {
                "frozen_inputs": baseline["frozen_inputs"],
                "provisioned": sorted(provisioned),
            },
            "metrics": metrics,
            "platform": {
                "device_identifier": "iPhone18,2",
                "os": "iOS 26.5.2 (23F84)",
                "sample_count": 1000,
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "toolchain": "frozen release toolchain",
            },
        }

    def assert_gate_failed(
        self, result: dict[str, Any], gate_id: str
    ) -> None:
        row = next(gate for gate in result["gates"] if gate["gate_id"] == gate_id)
        self.assertEqual(row["status"], "failed")
        self.assertEqual(result["overall_status"], "failed")

    def test_changed_result_identity_is_rejected(self) -> None:
        observation = self.pr_observation()
        observation["result_identity_match"] = False
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-IDENTITY")

    def test_changed_stable_order_is_rejected(self) -> None:
        observation = self.pr_observation()
        observation["stable_order_match"] = False
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-ORDERING")

    def test_deleted_outdated_or_dimension_regression_is_rejected(self) -> None:
        for key, value in (("deleted_hits", 1), ("outdated_hits", 1), ("dimension_mismatch_rejected", False)):
            with self.subTest(key=key):
                observation = self.pr_observation()
                observation[key] = value
                self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-EXCLUSIONS")

    def test_filter_and_replay_divergence_are_rejected(self) -> None:
        observation = self.pr_observation()
        observation["filter_mismatches"] = 1
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-FILTERING")
        observation = self.pr_observation()
        observation["replay_divergences"] = 1
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-PERSISTENCE")

    def test_reduced_ndcg_or_recall_is_rejected(self) -> None:
        for key in ("ndcg_at_3", "recall_at_3", "complete_evidence_recall_at_3"):
            with self.subTest(key=key):
                observation = self.pr_observation()
                observation[key] = 0.99
                self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-QUALITY-SMOKE")

    def test_reduced_candidate_recall_or_complete_evidence_is_rejected(self) -> None:
        for key in ("candidate_recall", "candidate_complete_evidence"):
            with self.subTest(key=key):
                observation = self.pr_observation()
                observation[key] = 0.99
                self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-QUALITY-SMOKE")

    def test_unexpected_empty_or_invalid_graph_scope_is_rejected(self) -> None:
        observation = self.pr_observation()
        observation["unexpected_empty_scope_count"] = 1
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-GRAPH-SCOPE")
        observation = self.pr_observation()
        observation["invalid_scope_rejections"] = 0
        self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-GRAPH-SCOPE")

    def test_nonzero_graph_free_activity_is_rejected(self) -> None:
        for key in ("graph_queries", "graph_nodes_visited", "graph_edges_traversed", "graph_candidates_projected"):
            with self.subTest(key=key):
                observation = self.pr_observation()
                observation[key] = 1
                self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-GRAPH-FREE")

    def test_missing_extra_or_altered_artifact_is_rejected(self) -> None:
        for key in ("artifact_inventory_valid", "schema_valid"):
            with self.subTest(key=key):
                observation = self.pr_observation()
                observation[key] = False
                self.assert_gate_failed(self.evaluate("pull_request", observation), "P7-PR-ARTIFACT-INTEGRITY")

    def test_independent_validator_rejects_missing_extra_and_altered_artifacts(self) -> None:
        for mutation in ("missing", "extra", "altered"):
            with self.subTest(mutation=mutation):
                copied = self.root / mutation
                shutil.copytree(REGRESSION, copied)
                if mutation == "missing":
                    (copied / "fixtures/expected-observation-v1.json").unlink()
                elif mutation == "extra":
                    (copied / "fixtures/undeclared.json").write_text("{}\n", encoding="utf-8")
                else:
                    with (copied / "contract-v1.json").open("ab") as stream:
                        stream.write(b" ")
                original = validator.BENCHMARK_ROOT
                validator.BENCHMARK_ROOT = copied
                try:
                    with self.assertRaises((OSError, validator.ValidationError)):
                        validator.validate_static(ROOT)
                finally:
                    validator.BENCHMARK_ROOT = original

    def test_full_quality_regressions_are_rejected(self) -> None:
        cases = {
            "P7-FULL-NDCG10": "ndcg_at_10",
            "P7-FULL-RECALL10": "recall_at_10",
            "P7-FULL-COMPLETE-EVIDENCE": "complete_evidence_recall_at_10",
            "P7-FULL-CANDIDATE-RECALL": "candidate_recall",
            "P7-FULL-CANDIDATE-COMPLETE": "candidate_complete_evidence",
        }
        for gate_id, metric in cases.items():
            with self.subTest(gate_id=gate_id):
                observation = self.tier_observation("scheduled_full")
                observation["metrics"][metric] = 0.0
                self.assert_gate_failed(self.evaluate("scheduled_full", observation), gate_id)

    def test_graph_free_slowdown_memory_and_size_violations_are_rejected(self) -> None:
        cases = {
            "P7-RELEASE-GRAPH-FREE-RATIO": ("maximum_graph_free_median_p95_ratio", 1.031),
            "P7-RELEASE-MEMORY": ("peak_process_memory_bytes", 1610612737),
            "P7-RELEASE-PERSISTED-SIZE": ("maximum_persisted_component_or_total_ratio", 1.051),
        }
        for gate_id, (metric, value) in cases.items():
            with self.subTest(gate_id=gate_id):
                observation = self.tier_observation("release")
                observation["metrics"][metric] = value
                self.assert_gate_failed(self.evaluate("release", observation), gate_id)

    def test_hidden_skip_is_rejected(self) -> None:
        result = self.evaluate("pull_request", self.pr_observation())
        result["gates"][0]["status"] = "not_provisioned"
        result["gates"][0]["actual"] = None
        result["overall_status"] = "not_provisioned"
        result_root = self.root / "hidden-skip"
        result_root.mkdir()
        (result_root / "result.json").write_bytes(runner.canonical_bytes(result))
        (result_root / "failure-summary.md").write_text(
            validator.expected_summary(result), encoding="utf-8"
        )
        with self.assertRaises(validator.ValidationError):
            validator.validate_result(result_root)

    def test_unauthorized_baseline_or_threshold_change_is_rejected(self) -> None:
        result = self.evaluate("pull_request", self.pr_observation())
        result["baseline"]["sha256"] = "0" * 64
        result_root = self.root / "bad-baseline"
        result_root.mkdir()
        (result_root / "result.json").write_bytes(runner.canonical_bytes(result))
        (result_root / "failure-summary.md").write_text(
            validator.expected_summary(result), encoding="utf-8"
        )
        with self.assertRaises(validator.ValidationError):
            validator.validate_result(result_root)

    def test_usearch_winner_and_physical_device_100k_claims_are_rejected(self) -> None:
        observation = self.tier_observation("release")
        observation["proposed_claim"] = "USearch performance winner"
        self.assert_gate_failed(self.evaluate("release", observation), "P7-RELEASE-CLAIMS")
        observation = self.tier_observation("release")
        observation["proposed_workload"] = "100k physical device support"
        self.assert_gate_failed(self.evaluate("release", observation), "P7-RELEASE-NO-100K")

    def test_required_zero_physical_device_violation_metric_can_pass(self) -> None:
        result = self.evaluate("release", self.tier_observation("release"))
        row = next(
            gate
            for gate in result["gates"]
            if gate["gate_id"] == "P7-RELEASE-NO-100K"
        )
        self.assertEqual(row["actual"], 0)
        self.assertEqual(row["status"], "passed")

    def test_missing_platform_or_version_qualifier_is_rejected(self) -> None:
        observation = self.tier_observation("release")
        del observation["platform"]["toolchain"]
        self.assert_gate_failed(self.evaluate("release", observation), "P7-RELEASE-CLAIMS")

    def test_unprovisioned_full_run_is_not_a_false_pass(self) -> None:
        result = runner.evaluate(
            SimpleNamespace(
                tier="scheduled_full",
                observation=None,
                repo=ROOT,
                source_revision="0123456789abcdef0123456789abcdef01234567",
            )
        )
        self.assertEqual(result["overall_status"], "not_provisioned")
        self.assertTrue(all(row["status"] == "not_provisioned" for row in result["gates"]))

    def test_release_lifecycle_and_latency_regressions_are_rejected(self) -> None:
        observation = self.tier_observation("release")
        observation["metrics"]["maximum_query_or_lifecycle_percentile_ratio"] = 1.101
        self.assert_gate_failed(self.evaluate("release", observation), "P7-RELEASE-LATENCY")
        observation = self.tier_observation("release")
        observation["metrics"]["lifecycle_violation_count"] = 1
        self.assert_gate_failed(self.evaluate("release", observation), "P7-RELEASE-LIFECYCLE")


if __name__ == "__main__":
    unittest.main()
