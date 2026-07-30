import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name(
    "generate-python-node-wrapper-conformance-input.py"
)
SPEC = importlib.util.spec_from_file_location("wrapper_conformance_input", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class WrapperConformanceInputTests(unittest.TestCase):
    def test_frozen_role_counts_order_and_diagnostics(self) -> None:
        document = MODULE.document()
        items = document["items"]
        self.assertEqual(document["schema_version"], 1)
        self.assertEqual(len(items), 94)
        self.assertEqual(
            [sum(item["role"] == role for item in items) for role in (
                "corpus",
                "query",
                "diagnostic",
            )],
            [48, 42, 4],
        )
        self.assertEqual(items[0]["id"], "corpus-000")
        self.assertEqual(items[48]["id"], "query-000")
        self.assertEqual(items[90]["id"], "diagnostic-000")
        self.assertEqual(items[-1]["text"], "retrieval " * 300)


if __name__ == "__main__":
    unittest.main()
