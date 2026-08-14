import copy
import importlib.util
import json
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


validator = load_module("validate_results", HERE / "validate_results.py")


class ValidateResultsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workloads, cls.protocol = validator.load_descriptors(HERE)
        cls.queries = {
            "schedule": [f"query-{index % 100:03d}" for index in range(750)]
        }

    def report(self) -> dict:
        samples = []
        for ordinal, query_id in enumerate(self.queries["schedule"]):
            start = 1_000_000 + ordinal * 10_000
            embedding = 100 + ordinal
            retrieval = 50 + ordinal
            total = embedding + retrieval + 10
            samples.append({
                "ordinal": ordinal,
                "query_id": query_id,
                "start_clock_ns": start,
                "end_clock_ns": start + total,
                "embedding_total_ns": embedding,
                "retrieval_total_ns": retrieval,
                "end_to_end_text_search_ns": total,
                "result_count": 10,
                "top_result_identity": "record-00000:1",
                "result_identity_digest": "a" * 64,
            })
        return {
            "schema_version": 1,
            "contract_version": "apple-end-to-end-v1",
            "workload_id": "apple-e2e-10k-384d-i8-v1",
            "workload_classification": "supported_product",
            "marketing_eligible": True,
            "supported_v1_capacity_changed": False,
            "profile_id": "coreml-fp32-production-v1",
            "profile_classification": "production_control",
            "session_id": "session-1",
            "search_mode": "vector",
            "top_k": 10,
            "warmup_count": 50,
            "samples": samples,
            "summaries": {
                stage: validator.expected_summary([sample[key] for sample in samples])
                for stage, key in validator.STAGES.items()
            },
            "environment": {
                "platform": "mac",
                "architecture": "arm64",
                "hardware": "MacBookPro18,4",
                "process_id": 123,
                "debugger_attached": False,
                "graph_linked": False,
                "onnx_runtime_linked": False,
            },
        }

    def test_valid_report_passes(self) -> None:
        report = self.report()
        key = validator.validate_report(report, self.queries, self.workloads, self.protocol)
        self.assertEqual(key[:4], (
            "mac", "apple-e2e-10k-384d-i8-v1", "coreml-fp32-production-v1", "vector"
        ))

    def test_summed_or_fabricated_total_fails(self) -> None:
        report = self.report()
        report["samples"][3]["end_to_end_text_search_ns"] += 1
        with self.assertRaisesRegex(validator.ValidationError, "direct nested timing"):
            validator.validate_report(report, self.queries, self.workloads, self.protocol)

    def test_summary_not_derived_from_raw_samples_fails(self) -> None:
        report = self.report()
        report["summaries"]["embedding_total"]["p95_ns"] += 1
        with self.assertRaisesRegex(validator.ValidationError, "retained raw samples"):
            validator.validate_report(report, self.queries, self.workloads, self.protocol)

    def test_stress_marketing_claim_fails(self) -> None:
        report = self.report()
        report.update({
            "workload_id": "apple-e2e-100k-384d-i8-stress-v1",
            "workload_classification": "stress",
            "marketing_eligible": True,
        })
        with self.assertRaisesRegex(validator.ValidationError, "marketing eligibility"):
            validator.validate_report(report, self.queries, self.workloads, self.protocol)

    def test_same_process_cannot_run_both_modes(self) -> None:
        first = self.report()
        second = copy.deepcopy(first)
        second["session_id"] = "session-2"
        second["search_mode"] = "weighted_hybrid"
        with self.assertRaisesRegex(validator.ValidationError, "more than one search mode"):
            validator.validate_collection(
                [first, second], self.queries, self.workloads, self.protocol, False
            )

    def test_powered_iphone_requires_explicit_powered_battery_state(self) -> None:
        workloads, protocol = validator.load_descriptors(HERE, "v2")
        report = self.report()
        report.update({
            "contract_version": "apple-end-to-end-v2",
            "workload_id": "apple-e2e-10k-384d-i8-usb-powered-v2",
        })
        report["environment"].update({
            "platform": "iphone",
            "hardware": "iPhone18,2",
        })
        report["iphone_validity"] = {
            "physical_device": True,
            "foreground_start": True,
            "foreground_end": True,
            "network_disabled": True,
            "low_power_mode": False,
            "battery_percent": 80,
            "battery_state": "charging",
            "charging": False,
            "thermal_start": "nominal",
            "thermal_end": "nominal",
            "memory_warning": False,
        }
        with self.assertRaisesRegex(validator.ValidationError, "battery state"):
            validator.validate_report(report, self.queries, workloads, protocol)


if __name__ == "__main__":
    unittest.main()
