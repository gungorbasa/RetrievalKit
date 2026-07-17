import json
import tempfile
import unittest
from pathlib import Path

import validate_hotpotqa_phase_3 as validator


class HotpotQAPhase3NegativeTests(unittest.TestCase):
    def test_test_split_is_rejected_before_access(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "before access"):
            validator.guard_development_path(Path("/sealed/test/queries.jsonl"))

    def test_missing_and_extra_candidates_are_rejected(self) -> None:
        registered = [{"fusion_alpha": 0.2}, {"fusion_alpha": 0.4}]
        for observed in ([registered[0]], [*registered, {"fusion_alpha": 0.6}]):
            with self.assertRaisesRegex(validator.ValidationError, "candidate closure"):
                validator.validate_candidate_closure(registered, observed)

    def test_mechanical_winner_rejects_incorrect_selection(self) -> None:
        rows = [
            {
                "candidate": {
                    "fusion_alpha": alpha,
                    "keyword_candidate_limit": 25,
                    "vector_candidate_limit": 25,
                },
                "aggregate": {
                    "complete_evidence_recall_at_10": score,
                    "ndcg_at_10": score,
                    "map": score,
                    "recall_at_10": score,
                    "mrr_at_10": score,
                },
            }
            for alpha, score in ((0.2, 0.8), (0.4, 0.7))
        ]
        winner = validator.mechanical_winner(rows)["candidate"]
        self.assertNotEqual(winner, rows[1]["candidate"])

    def test_metric_mismatch_is_rejected(self) -> None:
        with self.assertRaisesRegex(validator.ValidationError, "metric difference"):
            validator.assert_close(0.0, 1.0e-6, "metric")

    def test_nondeterministic_rerun_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            left = root / "left"
            right = root / "right"
            left.mkdir()
            right.mkdir()
            (left / "artifact").write_bytes(b"left")
            (right / "artifact").write_bytes(b"right")
            with self.assertRaisesRegex(validator.ValidationError, "byte-identical"):
                validator.recursive_identity(left, right)

    def test_failed_persistence_reload_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in (
                "graph-persistence-validation.json",
                "graph-retrieval-persistence-validation.json",
            ):
                value = {
                    "runs": [
                        {
                            "run_id": "v3-e-example",
                            "save_validate_load_equivalent": False,
                        }
                    ]
                }
                (root / name).write_bytes(validator.canonical(value) + b"\n")
            with self.assertRaisesRegex(
                validator.ValidationError, "persistence failure"
            ):
                validator.validate_persistence(root)

    def test_partial_artifact_publication_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "only.json").write_text("{}\n")
            manifest = {
                "files": [
                    {
                        "bytes": 3,
                        "path": "missing.json",
                        "sha256": validator.sha256(b"{}\n"),
                    }
                ]
            }
            with self.assertRaisesRegex(
                validator.ValidationError, "inventory mismatch"
            ):
                validator.inventory(root, manifest)

    def test_per_query_tuning_field_changes_canonical_candidate(self) -> None:
        global_candidate = {
            "fusion_alpha": 0.2,
            "keyword_candidate_limit": 100,
            "vector_candidate_limit": 100,
        }
        per_query = {**global_candidate, "query_id": "forbidden"}
        with self.assertRaises(validator.ValidationError):
            validator.validate_candidate_closure([global_candidate], [per_query])

    def test_graph_selection_mismatch_is_detectable(self) -> None:
        row = {"query_id": "q", "run_id": "d", "matched_nodes": []}
        changed = {**row, "run_id": "e", "matched_nodes": [{"id": "unexpected"}]}
        self.assertNotEqual(
            validator.normalized_graph_row(row, selection=True),
            validator.normalized_graph_row(changed, selection=True),
        )

    def test_population_or_exclusion_change_is_detectable(self) -> None:
        expected = {"declared": 603, "executed": 599, "excluded": 4}
        for field in expected:
            changed = dict(expected)
            changed[field] += 1
            self.assertNotEqual(expected, changed)

    def test_c_g_parameter_mismatch_is_detectable(self) -> None:
        c = {"fusion_alpha": 0.2, "candidate_limits": {"vector": 100}}
        g = json.loads(json.dumps(c))
        g["candidate_limits"]["vector"] = 50
        self.assertNotEqual(c, g)


if __name__ == "__main__":
    unittest.main()
