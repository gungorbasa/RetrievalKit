from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).with_name("validate_publication_recovery.py")
SPEC = importlib.util.spec_from_file_location("validate_publication_recovery", SCRIPT)
assert SPEC and SPEC.loader
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


class PublicationRecoveryTests(unittest.TestCase):
    def write_json(self, path: Path, value: object) -> Path:
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def inputs(self, root: Path) -> dict[str, object]:
        bundle = root / "bundle"
        bundle.mkdir()
        (bundle / "checksums.sha256").write_text("checksums\n", encoding="utf-8")
        (bundle / "inventory.json").write_text("{}\n", encoding="utf-8")
        candidate = self.write_json(
            root / "candidate.json",
            {
                "bundle": {"identity": "candidate"},
                "gate_results": {
                    "scheduled_gate": {"identity": "scheduled"},
                    "release_gate": {"identity": "release"},
                },
            },
        )
        authorization = self.write_json(root / "authorization.json", {})
        publication_run = self.write_json(
            root / "run.json",
            {
                "id": 40,
                "run_attempt": 1,
                "event": "workflow_dispatch",
                "head_sha": "a" * 40,
                "head_branch": "v0.1.0",
                "path": ".github/workflows/publish-release.yml",
                "status": "completed",
                "conclusion": "failure",
            },
        )
        jobs = self.write_json(
            root / "jobs.json",
            {
                "jobs": [
                    {"name": name, "conclusion": conclusion}
                    for name, conclusion in VALIDATOR.EXPECTED_JOBS.items()
                ]
            },
        )
        return {
            "repository": "gungorbasa/RetrievalKit",
            "tag": "v0.1.0",
            "source_revision": "a" * 40,
            "candidate_run_id": 10,
            "scheduled_run_id": 20,
            "release_gate_run_id": 30,
            "publication_run_id": 40,
            "bundle": bundle,
            "scheduled_result_path": self.write_json(root / "scheduled.json", {}),
            "release_gate_result_path": self.write_json(root / "release-gate.json", {}),
            "candidate_evidence_path": candidate,
            "authorization_record_path": authorization,
            "publication_run_path": publication_run,
            "publication_jobs_path": jobs,
            "release_path": self.write_json(
                root / "release.json",
                {
                    "tag_name": "v0.1.0",
                    "draft": False,
                    "prerelease": True,
                    "immutable": True,
                },
            ),
            "immutable_releases_path": self.write_json(
                root / "immutable.json", {"enabled": True}
            ),
        }

    def test_accepts_only_the_authorized_partial_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            inputs = self.inputs(Path(directory))
            with (
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "validate_authorization_record",
                ),
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "bundle_evidence",
                    return_value={"identity": "candidate"},
                ),
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "validate_gate_result",
                    side_effect=[
                        {"identity": "scheduled"},
                        {"identity": "release"},
                    ],
                ),
            ):
                result = VALIDATOR.validate(**inputs)
            self.assertEqual(
                result["allowedOperations"],
                [
                    "publish-exact-unpublished-pypi-wheels",
                    "publish-exact-unpublished-browser-npm-tarballs-without-npm-provenance",
                ],
            )
            self.assertIn("republish-maven", result["forbiddenOperations"])

    def test_rejects_changed_original_job_conclusions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = self.inputs(root)
            jobs = json.loads(Path(inputs["publication_jobs_path"]).read_text())
            jobs["jobs"][0]["conclusion"] = "failure"
            self.write_json(Path(inputs["publication_jobs_path"]), jobs)
            with (
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "validate_authorization_record",
                ),
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "bundle_evidence",
                    return_value={"identity": "candidate"},
                ),
                mock.patch.object(
                    VALIDATOR.publication_authorization,
                    "validate_gate_result",
                    side_effect=[
                        {"identity": "scheduled"},
                        {"identity": "release"},
                    ],
                ),
            ):
                with self.assertRaisesRegex(
                    VALIDATOR.RecoveryError, "job conclusions"
                ):
                    VALIDATOR.validate(**inputs)


if __name__ == "__main__":
    unittest.main()
