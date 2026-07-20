from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

BENCHMARK_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = BENCHMARK_ROOT.parents[1]
sys.path.insert(0, str(BENCHMARK_ROOT))

from phase5_common import canonical, canonical_file, sha256_bytes, sha256_file  # noqa: E402
from validate_artifacts import ValidationError, validate  # noqa: E402


@unittest.skipUnless(
    os.environ.get("PHASE5_RUN_INTEGRATION") == "1",
    "set PHASE5_RUN_INTEGRATION=1 to execute isolated external adapters",
)
class Phase5IntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory(prefix="phase5-tests-")
        cls.root = Path(cls.temporary.name)
        cls.first = cls.root / "first"
        cls.second = cls.root / "second"
        for output in [cls.first, cls.second]:
            completed = subprocess.run(
                [
                    sys.executable,
                    str(BENCHMARK_ROOT / "run_phase5.py"),
                    "--config",
                    str(BENCHMARK_ROOT / "configs" / "smoke-v1.json"),
                    "--output",
                    str(output),
                    "--python",
                    sys.executable,
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
            )
            if completed.returncode != 0:
                raise RuntimeError(
                    f"smoke run failed: stdout={completed.stdout}\nstderr={completed.stderr}"
                )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temporary.cleanup()

    def copy_root(self, name: str) -> Path:
        destination = self.root / name
        shutil.copytree(self.first, destination)
        return destination

    def rehash(self, root: Path) -> None:
        artifact_files = [
            "config.json",
            "environment.json",
            "feature-parity.json",
            "input-manifests.json",
            "raw-measurements.jsonl",
            "raw-results.jsonl",
            "failures.jsonl",
            "summary.json",
        ]
        entries = [
            {"path": name, "sha256": sha256_file(root / name)}
            for name in artifact_files
        ]
        canonical_file(
            root / "checksums.json",
            {
                "algorithm": "sha256",
                "artifact_type": "phase5_checksums",
                "entries": entries,
                "preimage_sha256": sha256_bytes(canonical(entries)),
                "schema_version": 1,
            },
        )
        manifest = json.loads((root / "manifest.json").read_text())
        manifest_entries = [
            {"path": name, "sha256": sha256_file(root / name)}
            for name in [*artifact_files, "checksums.json"]
        ]
        manifest["entries"] = manifest_entries
        manifest["artifact_set_sha256"] = sha256_bytes(canonical(manifest_entries))
        manifest["config_sha256"] = sha256_file(root / "config.json")
        manifest["feature_parity_sha256"] = sha256_file(root / "feature-parity.json")
        canonical_file(root / "manifest.json", manifest)

    def test_smoke_artifacts_validate(self) -> None:
        self.assertEqual(validate(self.first)["result"], "PASS")

    def test_deterministic_projection_matches(self) -> None:
        for name in [
            "config.json",
            "feature-parity.json",
            "input-manifests.json",
            "raw-results.jsonl",
            "failures.jsonl",
        ]:
            self.assertEqual(
                (self.first / name).read_bytes(), (self.second / name).read_bytes()
            )

    def test_percentile_mutation_is_rejected_after_rehash(self) -> None:
        root = self.copy_root("mutated-percentile")
        summary = json.loads((root / "summary.json").read_text())
        row = next(
            value
            for value in summary["rows"]
            if value["system_id"] == "sqlite_vec_exact"
        )
        operation = next(
            value
            for value in row["operations"]
            if value["operation_id"] == "exact_unfiltered"
        )
        operation["distribution"]["p95_ns"] += 1
        canonical_file(root / "summary.json", summary)
        self.rehash(root)
        with self.assertRaisesRegex(ValidationError, "distribution differs"):
            validate(root)

    def test_feature_parity_mutation_is_rejected_after_rehash(self) -> None:
        root = self.copy_root("mutated-parity")
        parity = json.loads((root / "feature-parity.json").read_text())
        parity["cells"]["usearch_hnsw"]["ann_equality_filter"] = "equivalent"
        canonical_file(root / "feature-parity.json", parity)
        self.rehash(root)
        with self.assertRaisesRegex(ValidationError, "feature parity differs"):
            validate(root)

    def test_honest_recall_gate_failure_is_validated(self) -> None:
        root = self.copy_root("honest-recall-failure")
        results_path = root / "raw-results.jsonl"
        results = [json.loads(line) for line in results_path.read_text().splitlines()]
        selected = [
            value
            for value in results
            if value["system_id"] == "usearch_hnsw"
            and value["query_id"] == "q-0000"
        ]
        oracle = next(
            value
            for value in results
            if value["system_id"] == "numpy_f32_oracle"
            and value["operation_id"] == "exact_unfiltered"
            and value["query_id"] == "q-0000"
        )
        replacement = next(
            f"chunk-{ordinal:08d}"
            for ordinal in range(256)
            if f"chunk-{ordinal:08d}" not in oracle["result_ids"]
            and f"chunk-{ordinal:08d}" not in selected[0]["result_ids"]
        )
        for value in selected:
            value["result_ids"][-1] = replacement
            value["result_identity_sha256"] = sha256_bytes(
                canonical(value["result_ids"])
            )
        results_path.write_bytes(
            b"".join(canonical(value) + b"\n" for value in results)
        )
        summary = json.loads((root / "summary.json").read_text())
        row = next(
            value
            for value in summary["rows"]
            if value["system_id"] == "usearch_hnsw"
        )
        row["gates"]["mean_recall_at_10"] = 0.975
        row["gates"]["recall_gate_passed"] = False
        row["acceptance"] = "failed"
        summary["overall_acceptance"] = "failed"
        canonical_file(root / "summary.json", summary)
        self.rehash(root)
        report = validate(root)
        self.assertEqual(report["result"], "PASS")
        self.assertEqual(report["benchmark_acceptance"], "failed")

    def test_unknown_inventory_entry_is_rejected(self) -> None:
        root = self.copy_root("mutated-inventory")
        (root / "unknown.json").write_text("{}\n")
        with self.assertRaisesRegex(ValidationError, "closed inventory differs"):
            validate(root)


if __name__ == "__main__":
    unittest.main()
