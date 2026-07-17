import tempfile
import unittest
from pathlib import Path

import validate_hotpotqa_phase_3_locked as validator


class LockedValidatorNegativeTests(unittest.TestCase):
    def stage_a(self) -> dict:
        return {
            "forbidden_files_opened": [],
            "opened_collection_files": ["collection.json", "queries.jsonl"],
            "previous_result_inputs": [],
            "status": "passed",
        }

    def selected_lock(self) -> dict:
        return {
            "selected_candidate": {
                "fusion_alpha": 0.2,
                "fusion_alpha_f32_bits": "3e4ccccd",
                "keyword_candidate_limit": 100,
                "vector_candidate_limit": 100,
            }
        }

    def test_qrels_opened_during_stage_a(self) -> None:
        audit = self.stage_a()
        audit["opened_collection_files"].append("qrels.tsv")
        with self.assertRaisesRegex(validator.ValidationError, "Stage A opened"):
            validator.validate_stage_a_audit(audit)

    def test_evidence_opened_during_stage_a(self) -> None:
        audit = self.stage_a()
        audit["opened_collection_files"].append("evidence-judgments.jsonl")
        with self.assertRaisesRegex(validator.ValidationError, "Stage A opened"):
            validator.validate_stage_a_audit(audit)

    def test_retrieval_invoked_during_stage_b(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "retrieval invoked"):
            validator.validate_stage_b_audit(
                {
                    "opened_label_files": sorted(validator.FORBIDDEN_STAGE_A),
                    "retrieval_invoked": True,
                }
            )

    def test_selected_lock_mismatch(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "lock mismatch"):
            validator.validate_selected_lock(self.selected_lock(), "0" * 64)

    def test_alpha_override(self) -> None:
        lock = self.selected_lock()
        lock["selected_candidate"]["fusion_alpha"] = 0.4
        with self.assertRaisesRegex(validator.ValidationError, "override"):
            validator.validate_selected_lock(lock, validator.LOCK_SHA256)

    def test_candidate_limit_override(self) -> None:
        lock = self.selected_lock()
        lock["selected_candidate"]["vector_candidate_limit"] = 50
        with self.assertRaisesRegex(validator.ValidationError, "override"):
            validator.validate_selected_lock(lock, validator.LOCK_SHA256)

    def test_extra_candidate_configuration(self) -> None:
        lock = self.selected_lock()
        lock["selected_candidate"]["query_id"] = "forbidden"
        with self.assertRaisesRegex(validator.ValidationError, "extra configuration"):
            validator.validate_selected_lock(lock, validator.LOCK_SHA256)

    def test_population_mutation(self) -> None:
        original = {"q1", "q2"}
        changed = original | {"q3"}
        self.assertNotEqual(
            validator.population_hash(original), validator.population_hash(changed)
        )

    def test_exclusion_mutation(self) -> None:
        population = {"q1", "q2", "q3"}
        expected = population - {"q3"}
        changed = population - {"q2"}
        self.assertNotEqual(
            validator.population_hash(expected), validator.population_hash(changed)
        )

    def test_test_query_removal(self) -> None:
        population = {"q1", "q2", "q3"}
        removed = population - {"q3"}
        self.assertNotEqual(
            validator.population_hash(population), validator.population_hash(removed)
        )

    def test_development_result_supplied_as_configuration_input(self) -> None:
        audit = self.stage_a()
        audit["previous_result_inputs"] = ["development-matrix/metrics.json"]
        with self.assertRaisesRegex(validator.ValidationError, "stale prior result"):
            validator.validate_stage_a_audit(audit)

    def test_stale_prior_test_artifact(self) -> None:
        audit = self.stage_a()
        audit["previous_result_inputs"] = ["prior-test/locked-analysis.json"]
        with self.assertRaisesRegex(validator.ValidationError, "stale prior result"):
            validator.validate_stage_a_audit(audit)

    def test_existing_output_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "report.json"
            output.write_text("existing")
            with self.assertRaisesRegex(validator.ValidationError, "overwrite"):
                validator.require_fresh_output(output)

    def test_ranking_modification_after_seal(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "rust-results.json"
            artifact.write_bytes(b"before\n")
            files = [
                {
                    "bytes": len(artifact.read_bytes()),
                    "path": artifact.name,
                    "sha256": validator.sha256(artifact.read_bytes()),
                }
            ]
            preimage = {"files": files, "schema_version": 1}
            seal = {
                "preimage": preimage,
                "ranking_seal_sha256": validator.sha256(validator.canonical(preimage)),
            }
            artifact.write_bytes(b"after\n")
            with self.assertRaisesRegex(validator.ValidationError, "modification"):
                validator.validate_ranking_seal(root, seal)

    def test_second_unauthorized_reporting_attempt(self) -> None:
        attempt = {"attempt": 2, "status": "passed"}
        with self.assertRaisesRegex(validator.ValidationError, "second unauthorized"):
            validator.validate_attempt(attempt, 1)

    def test_partial_artifact_publication(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "only.json").write_text("{}\n")
            manifest = {
                "artifact_root_sha256": validator.sha256(validator.canonical([])),
                "files": [],
            }
            with self.assertRaisesRegex(validator.ValidationError, "inventory mismatch"):
                validator.validate_inventory(root, manifest)

    def test_failed_persistence_reload(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in (
                "retrieval-persistence-validation.json",
                "graph-persistence-validation.json",
                "graph-retrieval-persistence-validation.json",
            ):
                value = {
                    "runs": [
                        {
                            "run_id": "run",
                            "save_validate_load_equivalent": False,
                        }
                    ],
                    "status": "valid",
                }
                (root / name).write_bytes(validator.canonical(value) + b"\n")
            with self.assertRaisesRegex(validator.ValidationError, "persistence"):
                validator.validate_persistence(root)

    def test_nondeterministic_ranking(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "nondeterministic"):
            validator.validate_rerun_equality(
                {
                    "mandatory_ranking_rerun_equal": False,
                    "mandatory_scoring_rerun_equal": True,
                }
            )

    def test_invalid_run_publication(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "invalid run"):
            validator.validate_execution_rows(
                [
                    {
                        "queries": [
                            {"execution_status": "invalid_execution", "query_id": "q"}
                        ]
                    }
                ]
            )


if __name__ == "__main__":
    unittest.main()
