from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).parents[1] / "validate_artifacts.py"
SPEC = importlib.util.spec_from_file_location("phase4_validator", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
validator = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validator
SPEC.loader.exec_module(validator)


class ValidatorTests(unittest.TestCase):
    def test_nearest_rank_uses_ceiling(self) -> None:
        values = list(range(1, 101))
        self.assertEqual(validator.nearest_rank(values, 50), 50)
        self.assertEqual(validator.nearest_rank(values, 95), 95)
        self.assertEqual(validator.nearest_rank(values, 99), 99)

    def test_100k_rejects_support_and_marketing(self) -> None:
        valid = {
            "workload_id": "100k-384d-v3-stress",
            "classification": "stress",
            "marketing_eligible": False,
            "supported_v1_capacity_changed": False,
        }
        validator.reject_100k_claim(valid, "valid")
        for mutation in (
            {"classification": "supported_product"},
            {"marketing_eligible": True},
            {"supported_v1_capacity_changed": True},
            {"claim_classification": "production"},
        ):
            with self.subTest(mutation=mutation), self.assertRaises(validator.ValidationError):
                validator.reject_100k_claim(valid | mutation, "invalid")

    def test_staged_report_rejects_percentile_sample_and_boundary_mutations(self) -> None:
        workload = validator.Workload("10k-384d-v3", "supported_product", 2500, 25, 10000, 100, 12500, 39000)
        report = staged_report()
        validator.validate_staged_report(report, workload)
        mutations = []
        bad_count = json.loads(json.dumps(report))
        bad_count["configurations"][0]["samples"].pop()
        mutations.append(bad_count)
        bad_percentile = json.loads(json.dumps(report))
        bad_percentile["configurations"][0]["distributions"][0]["p95_ns"] += 1
        mutations.append(bad_percentile)
        bad_boundary = json.loads(json.dumps(report))
        bad_boundary["configurations"][0]["samples"][0]["stages"][0]["sequence"] = 9
        mutations.append(bad_boundary)
        for mutation in mutations:
            with self.subTest(), self.assertRaises(validator.ValidationError):
                validator.validate_staged_report(mutation, workload)

    def test_device_sessions_require_three_thermal_valid_fresh_processes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_device_matrix(root)
            counts = validator.validate_device_sessions(root)
            self.assertEqual(counts, {"iphone17-pro-max": 24, "iphone14-pro-max": 18})
            broken = (
                root / "devices" / "iphone14-pro-max" / "supported"
                / "10k-384d-v3" / "f32" / "session-0.json"
            )
            value = json.loads(broken.read_text(encoding="utf-8"))
            value["thermal_state_end"] = "critical"
            broken.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaises(validator.ValidationError):
                validator.validate_device_sessions(root)

    def test_device_matrix_does_not_require_100k_on_iphone14(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_device_matrix(root)
            validator.validate_device_sessions(root)
            self.assertFalse((root / "devices" / "iphone14-pro-max" / "stress").exists())

    def test_device_matrix_rejects_100k_on_iphone14(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_device_matrix(root)
            (root / "devices" / "iphone14-pro-max" / "stress").mkdir()
            with self.assertRaisesRegex(validator.ValidationError, "iPhone-17-only"):
                validator.validate_device_sessions(root)

    def test_device_matrix_rejects_missing_or_wrong_device(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_device_matrix(root)
            missing = root / "devices" / "iphone14-pro-max"
            missing.rename(root / "devices" / "wrong-device")
            with self.assertRaisesRegex(validator.ValidationError, "directory matrix"):
                validator.validate_device_sessions(root)


def write_device_matrix(root: Path) -> None:
    for role, model, identifier, runs_stress in validator.DEVICE_MATRIX:
        workloads = validator.SUPPORTED_WORKLOAD_IDS + (
            (validator.STRESS_WORKLOAD_ID,) if runs_stress else ()
        )
        for workload in workloads:
            lane = "stress" if workload == validator.STRESS_WORKLOAD_ID else "supported"
            for encoding in ("f32", "i8"):
                directory = root / "devices" / role / lane / workload / encoding
                directory.mkdir(parents=True)
                for session in range(3):
                    value = {
                        "device_role": role,
                        "workload_id": workload,
                        "encoding": encoding,
                        "classification": "stress" if "100k" in workload else "supported_product",
                        "marketing_eligible": False,
                        "supported_v1_capacity_changed": False,
                        "physical_device": True,
                        "simulator": False,
                        "device_model": model,
                        "device_identifier": identifier,
                        "os_version": "26.5",
                        "os_build": "23F77",
                        "power_state": "battery",
                        "thermal_state_start": "nominal",
                        "thermal_state_end": "fair",
                        "rss_interval_ms": 1,
                        "memory_repetitions": 5,
                        "lifecycle_samples": 20,
                        "process_id": session + 1,
                        "lane": "graph_free",
                        "graph_free_evidence": {
                            "state_creations": 0,
                            "file_opens": 0,
                            "dispatches": 0,
                        },
                    }
                    (directory / f"session-{session}.json").write_text(
                        json.dumps(value), encoding="utf-8"
                    )


def staged_report() -> dict[str, object]:
    digest = "a" * 64
    samples = []
    for index in range(1000):
        samples.append({
            "sample_index": index,
            "query_id": "graph_filter_semantic",
            "stages": [
                {"stage": stage, "sequence": sequence, "duration_ns": index + sequence,
                 "directly_measured": stage == "end_to_end_total"}
                for sequence, stage in enumerate(validator.STAGES)
            ],
            "result_identity_sha256": digest,
            "selection_identity_sha256": digest,
            "path_identity_sha256": digest,
            "filter_identity_sha256": digest,
            "deleted_results": 0,
        })
    distributions = []
    for sequence, stage in enumerate(validator.STAGES):
        values = [index + sequence for index in range(1000)]
        distributions.append({
            "stage": stage,
            "sample_count": 1000,
            "min_ns": min(values),
            "max_ns": max(values),
            "mean_ns": sum(values) // 1000,
            "p50_ns": validator.nearest_rank(values, 50),
            "p95_ns": validator.nearest_rank(values, 95),
            "p99_ns": validator.nearest_rank(values, 99),
        })
    configuration = {
        "encoding": "f32",
        "result_identity_sha256": digest,
        "selection_identity_sha256": digest,
        "path_identity_sha256": digest,
        "filter_identity_sha256": digest,
        "samples": samples,
        "distributions": distributions,
    }
    return {
        "workload_id": "10k-384d-v3",
        "classification": "supported_product",
        "build_configuration": "release",
        "embedding_included": False,
        "warmups": 100,
        "samples_per_stage": 1000,
        "percentile_method": "nearest_rank",
        "stages": list(validator.STAGES),
        "configurations": [configuration, configuration | {"encoding": "i8"}],
    }


if __name__ == "__main__":
    unittest.main()
