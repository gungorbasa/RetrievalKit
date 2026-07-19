from __future__ import annotations

import importlib.util
import json
import sys
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

    def test_memory_validation_recomputes_peak_and_delta(self) -> None:
        memory = {
            "sample_interval_ms": 1,
            "baseline_resident_bytes": 100,
            "peak_resident_bytes": 140,
            "peak_delta_bytes": 40,
            "samples": [
                {"offset_ns": 1, "resident_bytes": 110},
                {"offset_ns": 2, "resident_bytes": 140},
            ],
        }
        validator.validate_memory(memory, "valid")
        for mutation in (
            {"sample_interval_ms": 2},
            {"peak_resident_bytes": 139},
            {"peak_delta_bytes": 39},
        ):
            with self.subTest(mutation=mutation), self.assertRaises(validator.ValidationError):
                validator.validate_memory(memory | mutation, "invalid")

    def test_envelope_rejects_simulator_thermal_and_stale_binary(self) -> None:
        hashes = {
            "core_device_id": "core", "product_type": "iPhone18,2",
            "hardware_model": "V54AP", "os_build": "23F81",
            "candidate_app": "a" * 64, "candidate_framework": "b" * 64,
        }
        value = device_envelope(hashes)
        authorization = validator.EvidenceAuthorization("c" * 64, hashes)
        validator.validate_envelope(
            value, Path("artifact.json"), "iphone17-pro-max", "candidate",
            authorization, set(),
        )
        mutations = []
        simulated = json.loads(json.dumps(value))
        simulated["environment"]["physical_device"] = False
        mutations.append(simulated)
        thermal = json.loads(json.dumps(value))
        thermal["environment"]["thermal_state_end"] = "critical"
        mutations.append(thermal)
        mutations.append(value | {"app_executable_sha256": "d" * 64})
        for mutation in mutations:
            with self.subTest(), self.assertRaises(validator.ValidationError):
                validator.validate_envelope(
                    mutation, Path("bad.json"), "iphone17-pro-max", "candidate",
                    authorization, set(),
                )

    def test_authorization_resolver_uses_prior_only_for_frozen_v3_paths(self) -> None:
        root = Path("/artifacts")
        current = validator.EvidenceAuthorization("4" * 64, {})
        prior = validator.EvidenceAuthorization("3" * 64, {})
        resolver = validator.AuthorizationResolver(root, current, prior)
        prior_path = root / (
            "devices/iphone17-pro-max/supported/25k-384d-v3/i8/"
            "query/session-04.json"
        )
        current_path = root / (
            "devices/iphone17-pro-max/supported/10k-384d-v3/f32/"
            "lifecycle/read_only_validation/warmup-00.json"
        )
        self.assertIs(resolver.context_for(prior_path), prior)
        self.assertIs(resolver.context_for(current_path), current)
        self.assertIs(
            validator.AuthorizationResolver(root, current).context_for(prior_path),
            current,
        )

    def test_phase4b_matrix_is_iphone17_only(self) -> None:
        self.assertEqual(
            validator.DEVICE_MATRIX,
            (("iphone17-pro-max", "iPhone 17 Pro Max", "iPhone18,2", True),),
        )
        self.assertEqual(validator.SUPPORTED_WORKLOAD_IDS, validator.WORKLOAD_IDS[:3])


def device_envelope(hashes: dict[str, str]) -> dict[str, object]:
    return {
        "ok": True,
        "collector_exit_code": 0,
        "atomic_write_completed": True,
        "device_role": "iphone17-pro-max",
        "host_device_identifier": hashes["core_device_id"],
        "authorization_sha256": "c" * 64,
        "product_role": "candidate",
        "app_executable_sha256": hashes["candidate_app"],
        "framework_binary_sha256": hashes["candidate_framework"],
        "environment": {
            "physical_device": True,
            "simulator": False,
            "build_configuration": "release",
            "device_identifier": hashes["product_type"],
            "hardware_model": hashes["hardware_model"],
            "os_build": f"Version 26.5 (Build {hashes['os_build']})",
            "thermal_state_start": "nominal",
            "thermal_state_end": "fair",
            "one_scenario_per_fresh_process": True,
            "low_power_mode": False,
            "foreground": True,
            "network_disabled": True,
            "physical_memory_bytes": 6_000_000_000,
            "process_id": 42,
        },
    }


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
