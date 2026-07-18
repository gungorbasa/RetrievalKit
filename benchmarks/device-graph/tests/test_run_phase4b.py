from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
