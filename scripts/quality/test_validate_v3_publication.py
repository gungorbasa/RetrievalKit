from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.quality import assemble_v3_publication as assembler
from scripts.quality import validate_v3_conformance as foundation
from scripts.quality import validate_v3_publication as validator
from scripts.quality.validate_v3_phase_1_2a import ValidationError


COLLECTION = Path("benchmarks/retrieval-quality/v3")


def valid_manifest_run_state() -> tuple[dict[str, object], dict[str, bytes]]:
    files = validator.collection_files(COLLECTION)
    revision = {
        "binary_sha256": "a" * 64,
        "git_commit": "b" * 40,
        "source_sha256": None,
    }
    runs = foundation.derive_runs(files, revision)
    fingerprints = foundation.derive_generation_fingerprints(files, runs)
    by_run = {row["run_id"]: row["fingerprint"] for row in fingerprints["bindings"]}
    manifest: dict[str, object] = {
        "implementation_revision": revision,
        "run_configurations": [
            {
                "configuration": row["configuration"],
                "declared_population_sha256": row["declared_population_sha256"],
                "execution_population_sha256": row["execution_population_sha256"],
                "generation_fingerprint": by_run.get(row["run_id"]),
                "logical_run_sha256": row["logical_run_sha256"],
                "run_id": row["run_id"],
            }
            for row in runs
        ],
        "population_hashes": [
            {
                "declared": row["declared_population_sha256"],
                "execution": row["execution_population_sha256"],
                "run_id": row["run_id"],
            }
            for row in runs
        ],
        "generation_fingerprints": fingerprints["preimages"],
    }
    return manifest, files


class V3IndependentPublicationValidatorTests(unittest.TestCase):
    def test_positive_run_population_and_fingerprint_validation(self) -> None:
        manifest, files = valid_manifest_run_state()
        runs = validator.validate_run_configurations(manifest, files)
        self.assertEqual(len(runs), 15)

    def test_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValidationError, "missing files"):
                validator.validate_inventory(root, {"manifest.json"})

    def test_extra_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "extra").write_text("x", encoding="utf-8")
            with self.assertRaisesRegex(ValidationError, "extra files"):
                validator.validate_inventory(root, set())

    def test_incorrect_file_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").write_bytes(b"x")
            manifest = {"files": [{"bytes": 1, "path": "data", "sha256": "0" * 64}], "deterministic_files": ["data"]}
            with self.assertRaisesRegex(ValidationError, "digest or byte-count"):
                validator.validate_manifest_file_index(root, manifest)

    def test_incorrect_byte_count(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").write_bytes(b"x")
            manifest = {"files": [{"bytes": 2, "path": "data", "sha256": validator.sha256(b"x")}], "deterministic_files": ["data"]}
            with self.assertRaisesRegex(ValidationError, "digest or byte-count"):
                validator.validate_manifest_file_index(root, manifest)

    def test_incorrect_binary_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "binary"
            executable.write_bytes(b"binary")
            manifest = {"implementation_revision": {"binary_sha256": "0" * 64, "git_commit": "b" * 40, "source_sha256": None}}
            with self.assertRaisesRegex(ValidationError, "incorrect binary digest"):
                validator.validate_revision(manifest, executable, Path(directory))

    def test_incorrect_environment_digest(self) -> None:
        environment = {
            "cpu_architecture": "arm64",
            "cpu_features": [],
            "execution_threads": 1,
            "floating_point_mode": "round_to_nearest_ties_to_even",
            "locale": "C",
            "os_build": "test",
            "runtime_flags": [],
        }
        manifest = {
            "determinism_environment": environment,
            "determinism_context": {
                "binary_sha256": "a" * 64,
                "environment_sha256": "0" * 64,
                "runtime_id": "rustc",
                "runtime_version": "test",
                "target_triple": "test",
            },
            "implementation_revision": {"binary_sha256": "a" * 64},
        }
        with self.assertRaisesRegex(ValidationError, "environment digest"):
            validator.validate_environment(manifest)

    def test_wrong_implementation_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "binary"
            executable.write_bytes(b"binary")
            manifest = {
                "implementation_revision": {
                    "binary_sha256": validator.sha256(b"binary"),
                    "git_commit": "b" * 40,
                    "source_sha256": None,
                }
            }
            completed = subprocess.CompletedProcess([], 0, "c" * 40 + "\n", "")
            with mock.patch.object(validator.subprocess, "run", return_value=completed):
                with self.assertRaisesRegex(ValidationError, "wrong implementation revision"):
                    validator.validate_revision(manifest, executable, Path(directory))

    def test_run_id_mismatch(self) -> None:
        manifest, files = valid_manifest_run_state()
        manifest["run_configurations"][0]["run_id"] = "wrong"
        with self.assertRaisesRegex(ValidationError, "run-ID or logical-run"):
            validator.validate_run_configurations(manifest, files)

    def test_logical_run_mismatch(self) -> None:
        manifest, files = valid_manifest_run_state()
        manifest["run_configurations"][0]["logical_run_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValidationError, "run-ID or logical-run"):
            validator.validate_run_configurations(manifest, files)

    def test_population_mismatch(self) -> None:
        manifest, files = valid_manifest_run_state()
        manifest["population_hashes"][0]["declared"] = "0" * 64
        with self.assertRaisesRegex(ValidationError, "population mismatch"):
            validator.validate_run_configurations(manifest, files)

    def test_generation_fingerprint_mismatch(self) -> None:
        manifest, files = valid_manifest_run_state()
        manifest["generation_fingerprints"][0]["fingerprint"] = "0" * 64
        with self.assertRaisesRegex(ValidationError, "generation-fingerprint mismatch"):
            validator.validate_run_configurations(manifest, files)

    def test_noncanonical_serialization(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "data.json"
            path.write_text('{"b": 1, "a": 2}\n', encoding="utf-8")
            with self.assertRaisesRegex(ValidationError, "noncanonical serialization"):
                validator.canonical_json_file(path)

    def test_invalid_run_status(self) -> None:
        with self.assertRaisesRegex(ValidationError, "invalid run status"):
            validator.validate_valid_run_statuses(
                [{"run_id": "run", "status": "invalid_execution"}],
                [{"run_id": "run", "status": "invalid_execution"}],
            )

    def test_failed_or_mismatched_trec_eval_report(self) -> None:
        with self.assertRaisesRegex(ValidationError, "trec_eval report"):
            validator.require_gate_status({"status": "failed"}, "trec_eval")

    def test_manifest_is_not_finalized_before_all_gates_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            qualification = root / "qualification"
            qualification.mkdir()
            assembler.write_json(
                qualification / "qualification.json", {"status": "invalid_execution"}
            )
            output = root / "public"
            with self.assertRaises(ValidationError):
                assembler.assemble(
                    COLLECTION,
                    qualification,
                    output,
                    foundation.implementation_revision(),
                    {},
                    {},
                )
            self.assertFalse((output / "manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
