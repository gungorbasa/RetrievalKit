from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).parents[1] / "run_phase4b.py"
SPEC = importlib.util.spec_from_file_location("phase4_collector", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
collector = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = collector
SPEC.loader.exec_module(collector)


class CollectorTests(unittest.TestCase):
    def test_console_parser_uses_last_benchmark_object(self) -> None:
        output = 'noise {"not":"benchmark"}\n{"ok":false}\nmore\n{"ok":true,"value":7}\n'
        self.assertEqual(collector.parse_app_json(output), {"ok": True, "value": 7})

    def test_console_parser_fails_closed_without_benchmark_json(self) -> None:
        with self.assertRaises(collector.CollectorError):
            collector.parse_app_json("device log only")

    def test_atomic_json_replaces_complete_object_without_temp_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "nested" / "artifact.json"
            collector.atomic_json(path, {"ok": True, "sample": 1})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["sample"], 1)
            collector.atomic_json(path, {"ok": False, "sample": 2})
            self.assertEqual(json.loads(path.read_text(encoding="utf-8"))["sample"], 2)
            self.assertEqual([item.name for item in path.parent.iterdir()], [path.name])

    def test_collection_scope_is_iphone17_only(self) -> None:
        self.assertEqual(
            collector.DEVICES,
            {"iphone17-pro-max": "E342200A-C959-5384-A846-24F4163E5722"},
        )
        self.assertEqual(collector.STRESS, "100k-384d-v3-stress")
        self.assertNotIn("100k-384d-v3-stress", collector.SUPPORTED)

    def test_lifecycle_sample_ids_are_unique_across_configurations(self) -> None:
        first = collector.lifecycle_sample_id(
            "10k-384d-v3", "f32", "save", "warmup", 0
        )
        second = collector.lifecycle_sample_id(
            "10k-384d-v3", "i8", "save", "warmup", 0
        )
        third = collector.lifecycle_sample_id(
            "25k-384d-v3", "f32", "save", "warmup", 0
        )
        self.assertEqual(first, "10k-384d-v3-f32-save-warmup-00")
        self.assertEqual(len({first, second, third}), 3)

    def test_failed_launch_is_preserved_only_under_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            instance = collector.Collector.__new__(collector.Collector)
            instance.root = root
            instance.authorization_sha256 = "4" * 64
            instance.lineage = None
            instance.cooling_pause_seconds = 0
            item = collector.Product(
                role="candidate",
                app=root / "app",
                bundle_id="dev.retrievalkit.test",
                executable_sha256="a" * 64,
                framework_sha256="b" * 64,
            )
            destination = root / "devices" / "iphone17-pro-max" / "sample.json"
            completed = collector.subprocess.CompletedProcess(
                args=[], returncode=2, stdout='{"ok":false,"error":"thermal"}', stderr=""
            )
            with mock.patch.object(collector.subprocess, "run", return_value=completed):
                with self.assertRaises(collector.CollectorError):
                    instance.launch(
                        "iphone17-pro-max", item, [], destination, "thermal-sample"
                    )
            self.assertFalse(destination.exists())
            rejected = list((root / "rejected").rglob("*.json"))
            self.assertEqual(len(rejected), 1)
            self.assertIn("sample.attempt-", rejected[0].name)
            self.assertFalse(rejected[0].is_relative_to(root / "devices"))

    def test_resume_reuses_only_authorized_prior_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            instance = collector.Collector.__new__(collector.Collector)
            instance.root = root
            instance.authorization_sha256 = "4" * 64
            instance.lineage = {"prior_authorization_sha256": "3" * 64}
            preserved = root / (
                "devices/iphone17-pro-max/supported/10k-384d-v3/f32/"
                "query/session-00.json"
            )
            collector.atomic_json(preserved, {"authorization_sha256": "3" * 64})
            self.assertEqual(
                instance.reusable_evidence(preserved)["authorization_sha256"],
                "3" * 64,
            )

            unfinished = root / (
                "devices/iphone17-pro-max/supported/10k-384d-v3/f32/"
                "lifecycle/read_only_validation/warmup-00.json"
            )
            collector.atomic_json(unfinished, {"authorization_sha256": "3" * 64})
            with self.assertRaises(collector.CollectorError):
                instance.reusable_evidence(unfinished)


if __name__ == "__main__":
    unittest.main()
