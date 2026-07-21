from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import tempfile
import unittest
from datetime import date
from pathlib import Path
from typing import Any


REPO = Path(__file__).resolve().parents[3]
VALIDATOR_PATH = REPO / "benchmarks/publication/validate_publication.py"
SPEC = importlib.util.spec_from_file_location("phase6_validator", VALIDATOR_PATH)
assert SPEC and SPEC.loader
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)
ARTIFACT = REPO / "benchmarks/publication/artifacts/phase6-publication-v1"
PHASE3 = REPO / "target/benchmarks/hotpotqa-phase-3b/locked-reporting"
PHASE4 = REPO / "target/phase4b/device-results-v3-02b8971"
PHASE5 = REPO / "benchmarks/external-reference/artifacts/mac-comparison-v1"


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n",
        encoding="utf-8",
    )


class Phase6MutationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "publication"
        shutil.copytree(ARTIFACT, self.root)
        self.claims = load_json(self.root / "claim-register.json")
        self.index = load_json(self.root / "evidence-index.json")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def claim(self, claim_id: str) -> dict[str, Any]:
        return next(row for row in self.claims["claims"] if row["claim_id"] == claim_id)

    def assert_claims_rejected(self) -> None:
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_claims(self.claims, date(2026, 7, 21))

    def test_changed_metric_or_rounding_is_rejected(self) -> None:
        self.index["mac"]["exact_rows"][0]["p50_ratio_sqlite_over_vectorkit"] = "7.18"
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_mac(self.index, PHASE5)

    def test_missing_file_is_rejected(self) -> None:
        (self.root / "methodology.md").unlink()
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_inventory(self.root)

    def test_extra_file_is_rejected(self) -> None:
        (self.root / "extra.md").write_text("extra\n", encoding="utf-8")
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_inventory(self.root)

    def test_incorrect_hash_is_rejected(self) -> None:
        path = self.root / "methodology.md"
        path.write_text(path.read_text(encoding="utf-8") + "changed\n", encoding="utf-8")
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_hashes(self.root, REPO)

    def test_unsupported_universal_winner_is_rejected(self) -> None:
        self.claim("P6-MAC-EXACT-001")["claim_text"] = "VectorKit is universally faster and better."
        self.assert_claims_rejected()

    def test_usearch_performance_claim_is_rejected(self) -> None:
        self.claim("P6-ANN-NEGATIVE-001")["claim_text"] = "USearch has a latency advantage and is faster."
        self.assert_claims_rejected()

    def test_graph_winner_claim_is_rejected(self) -> None:
        self.claim("P6-QUALITY-001")["claim_text"] = "VectorKit beats the graph baseline on performance."
        self.assert_claims_rejected()

    def test_100k_support_or_marketing_claim_is_rejected(self) -> None:
        self.claim("P6-DEVICE-SAFETY-001")["claim_text"] = "VectorKit supports 100K chunks on iPhone."
        self.assert_claims_rejected()

    def test_missing_hardware_or_version_qualifier_is_rejected(self) -> None:
        self.claim("P6-MAC-EXACT-001")["hardware"] = ""
        self.assert_claims_rejected()

    def test_expired_claims_are_rejected(self) -> None:
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_claims(self.claims, date(2027, 7, 22))

    def test_unlicensed_public_artifact_is_rejected(self) -> None:
        (self.root / "model.bin").write_bytes(b"not licensed for this package")
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_licensing(self.root)

    def test_rejected_or_disqualified_reference_is_rejected(self) -> None:
        row = self.claim("P6-MAC-EXACT-001")["evidence"][0]
        row["path"] = "target/rejected/disqualified-timing.json"
        with self.assertRaises(VALIDATOR.ValidationError):
            VALIDATOR.validate_evidence_references(self.index, self.claims, REPO)


@unittest.skipUnless(os.environ.get("PHASE6_RUN_INTEGRATION") == "1", "set PHASE6_RUN_INTEGRATION=1")
class Phase6IntegrationTests(unittest.TestCase):
    def test_frozen_package_recomputes_from_evidence(self) -> None:
        args = argparse.Namespace(
            repo=REPO,
            phase3_root=PHASE3,
            phase4_root=PHASE4,
            phase5_root=PHASE5,
            root=ARTIFACT,
            as_of_date=date(2026, 7, 21),
        )
        result = VALIDATOR.validate_package(args)
        self.assertEqual(result["result"], "PASS")


if __name__ == "__main__":
    unittest.main()
