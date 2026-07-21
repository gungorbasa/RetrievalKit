from __future__ import annotations

import copy
import importlib.util
import json
import re
import tempfile
import unittest
from datetime import date
from pathlib import Path
from typing import Any


REPO = Path(__file__).resolve().parents[3]
VALIDATOR_PATH = REPO / "benchmarks/publication/validate_readme.py"
SPEC = importlib.util.spec_from_file_location("readme_validator", VALIDATOR_PATH)
assert SPEC and SPEC.loader
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)
MAPPING = json.loads((REPO / "benchmarks/publication/readme-claims-v1.json").read_text())
README = (REPO / "README.md").read_text()


class ReadmeClaimMutationTests(unittest.TestCase):
    def assert_rejected(self, readme: str, mapping: dict[str, Any] | None = None, as_of: date = date(2026, 7, 21)) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "README.md").write_text(readme)
            with self.assertRaises(VALIDATOR.ValidationError):
                VALIDATOR.validate_claims(REPO, readme, mapping or MAPPING, as_of)

    def test_checked_in_readme_passes(self) -> None:
        result = VALIDATOR.validate(REPO, REPO / "benchmarks/publication/readme-claims-v1.json", date(2026, 7, 21))
        self.assertEqual(result["result"], "PASS")

    def test_readme_quickstarts_match_executed_sources(self) -> None:
        python_blocks = re.findall(r"```python\n(.*?)```", README, re.DOTALL)
        swift_blocks = re.findall(r"```swift\n(.*?)```", README, re.DOTALL)
        self.assertEqual(len(python_blocks), 1)
        self.assertEqual(len(swift_blocks), 1)
        python_source = (REPO / "wrappers/python/examples/database_quickstart.py").read_text()
        self.assertEqual(python_blocks[0], "\n".join(python_source.splitlines()[2:]) + "\n")
        swift_source = (REPO / "wrappers/swift/VectorKit/Sources/VectorKitDatabaseQuickstart/main.swift").read_text()
        self.assertEqual(swift_blocks[0], swift_source)

    def test_changed_number_is_rejected(self) -> None:
        self.assert_rejected(README.replace("7.17×", "7.18×"))

    def test_missing_hardware_and_version_qualifiers_are_rejected(self) -> None:
        self.assert_rejected(README.replace("Apple M1 Max", "test Mac", 2))
        self.assert_rejected(README.replace("sqlite-vec `0.1.9`", "sqlite-vec", 1))

    def test_universal_superiority_is_rejected(self) -> None:
        self.assert_rejected(README + "\nVectorKit is universally faster than sqlite-vec.\n")

    def test_usearch_timing_claim_is_rejected(self) -> None:
        self.assert_rejected(README + "\nUSearch has lower latency and is faster.\n")

    def test_graph_winner_claim_is_rejected(self) -> None:
        self.assert_rejected(README + "\nVectorKit beats the graph baseline.\n")

    def test_100k_support_claim_is_rejected(self) -> None:
        self.assert_rejected(README + "\nVectorKit supports 100K chunks.\n")

    def test_expired_claim_is_rejected(self) -> None:
        self.assert_rejected(README, as_of=date(2027, 7, 22))

    def test_prohibited_or_broadened_mapping_is_rejected(self) -> None:
        mapping = copy.deepcopy(MAPPING)
        mapping["claims"]["P6-PROHIBITED-001"] = []
        self.assert_rejected(README + "\n<!-- claim:P6-PROHIBITED-001 -->x<!-- /claim -->", mapping)


if __name__ == "__main__":
    unittest.main()
