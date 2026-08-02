#!/usr/bin/env python3
"""Validate a narrowly scoped recovery from a partial v0.1.0 publication."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

PUBLICATION_AUTHORIZATION_PATH = Path(__file__).with_name(
    "publication_authorization.py"
)
PUBLICATION_AUTHORIZATION_SPEC = importlib.util.spec_from_file_location(
    "recovery_publication_authorization", PUBLICATION_AUTHORIZATION_PATH
)
if (
    PUBLICATION_AUTHORIZATION_SPEC is None
    or PUBLICATION_AUTHORIZATION_SPEC.loader is None
):
    raise RuntimeError("cannot load publication authorization validator")
publication_authorization = importlib.util.module_from_spec(
    PUBLICATION_AUTHORIZATION_SPEC
)
PUBLICATION_AUTHORIZATION_SPEC.loader.exec_module(publication_authorization)


EXPECTED_JOBS = {
    "Validate signed identity and immutable candidate evidence": "success",
    "Record approval, attest, and publish GitHub/SwiftPM assets": "success",
    "Trusted PyPI publication": "failure",
    "Trusted npm publication with provenance": "failure",
    "Sign and upload Maven Central Portal bundle": "success",
}


class RecoveryError(RuntimeError):
    """Recovery inputs do not describe the authorized partial release."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RecoveryError(message)


def load_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"expected a JSON object: {path}")
    return value


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(
    *,
    repository: str,
    tag: str,
    source_revision: str,
    candidate_run_id: int,
    scheduled_run_id: int,
    release_gate_run_id: int,
    publication_run_id: int,
    bundle: Path,
    scheduled_result_path: Path,
    release_gate_result_path: Path,
    candidate_evidence_path: Path,
    authorization_record_path: Path,
    publication_run_path: Path,
    publication_jobs_path: Path,
    release_path: Path,
    immutable_releases_path: Path,
) -> dict[str, Any]:
    candidate_evidence = load_object(candidate_evidence_path)
    authorization_record = load_object(authorization_record_path)
    try:
        publication_authorization.validate_authorization_record(
            authorization_record,
            candidate_path=candidate_evidence_path,
            repository=repository,
            tag=tag,
            revision=source_revision,
            candidate_run_id=candidate_run_id,
            scheduled_run_id=scheduled_run_id,
            release_gate_run_id=release_gate_run_id,
            publication_run_id=publication_run_id,
            publication_run_attempt=1,
        )
        require(
            candidate_evidence["bundle"]
            == publication_authorization.bundle_evidence(bundle, tag, source_revision),
            "candidate bundle differs from the authorized evidence",
        )
        require(
            candidate_evidence["gate_results"]["scheduled_gate"]
            == publication_authorization.validate_gate_result(
                scheduled_result_path,
                tier="scheduled_full",
                revision=source_revision,
                label="scheduled gate result",
            ),
            "scheduled gate differs from the authorized evidence",
        )
        require(
            candidate_evidence["gate_results"]["release_gate"]
            == publication_authorization.validate_gate_result(
                release_gate_result_path,
                tier="release",
                revision=source_revision,
                label="release gate result",
            ),
            "release gate differs from the authorized evidence",
        )
    except publication_authorization.AuthorizationError as error:
        raise RecoveryError(str(error)) from error

    publication_run = load_object(publication_run_path)
    require(publication_run.get("id") == publication_run_id, "publication run ID mismatch")
    require(publication_run.get("run_attempt") == 1, "publication reruns cannot authorize recovery")
    require(publication_run.get("event") == "workflow_dispatch", "publication event mismatch")
    require(publication_run.get("head_sha") == source_revision, "publication revision mismatch")
    require(publication_run.get("head_branch") == tag, "publication tag mismatch")
    require(
        publication_run.get("path") == ".github/workflows/publish-release.yml",
        "publication workflow path mismatch",
    )
    require(
        publication_run.get("status") == "completed"
        and publication_run.get("conclusion") == "failure",
        "publication run is not the completed partial failure",
    )

    publication_jobs = load_object(publication_jobs_path)
    jobs = publication_jobs.get("jobs")
    require(isinstance(jobs, list), "publication job inventory is missing")
    observed_jobs = {
        row.get("name"): row.get("conclusion")
        for row in jobs
        if isinstance(row, dict) and row.get("name") in EXPECTED_JOBS
    }
    require(observed_jobs == EXPECTED_JOBS, "publication job conclusions changed")

    release = load_object(release_path)
    require(release.get("tag_name") == tag, "GitHub Release tag mismatch")
    require(release.get("draft") is False, "GitHub Release remains a draft")
    require(release.get("prerelease") is True, "GitHub Release must remain a preview")
    require(release.get("immutable") is True, "GitHub Release is not immutable")
    immutable_releases = load_object(immutable_releases_path)
    require(immutable_releases.get("enabled") is True, "immutable releases are disabled")

    return {
        "schemaVersion": 1,
        "kind": "retrievalkit-publication-recovery-authorization",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "repository": repository,
        "release": {
            "tag": tag,
            "sourceRevision": source_revision,
            "immutable": True,
            "preview": True,
        },
        "originalPublication": {
            "runId": publication_run_id,
            "runAttempt": 1,
            "conclusion": "failure",
            "jobConclusions": EXPECTED_JOBS,
        },
        "authorizedEvidence": {
            "candidateRunId": candidate_run_id,
            "scheduledRunId": scheduled_run_id,
            "releaseGateRunId": release_gate_run_id,
            "candidateEvidenceSha256": digest(candidate_evidence_path),
            "authorizationRecordSha256": digest(authorization_record_path),
            "checksumsSha256": digest(bundle / "checksums.sha256"),
            "inventorySha256": digest(bundle / "inventory.json"),
        },
        "allowedOperations": [
            "publish-exact-unpublished-pypi-wheels",
            "publish-exact-unpublished-browser-npm-tarballs-without-npm-provenance",
        ],
        "forbiddenOperations": [
            "replace-existing-registry-version",
            "recreate-github-release",
            "republish-maven",
            "retag-v0.1.0",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--candidate-run-id", required=True, type=int)
    parser.add_argument("--scheduled-run-id", required=True, type=int)
    parser.add_argument("--release-gate-run-id", required=True, type=int)
    parser.add_argument("--publication-run-id", required=True, type=int)
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--scheduled-result", required=True, type=Path)
    parser.add_argument("--release-gate-result", required=True, type=Path)
    parser.add_argument("--candidate-evidence", required=True, type=Path)
    parser.add_argument("--authorization-record", required=True, type=Path)
    parser.add_argument("--publication-run", required=True, type=Path)
    parser.add_argument("--publication-jobs", required=True, type=Path)
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--immutable-releases", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        result = validate(
            repository=args.repository,
            tag=args.tag,
            source_revision=args.source_revision,
            candidate_run_id=args.candidate_run_id,
            scheduled_run_id=args.scheduled_run_id,
            release_gate_run_id=args.release_gate_run_id,
            publication_run_id=args.publication_run_id,
            bundle=args.bundle,
            scheduled_result_path=args.scheduled_result,
            release_gate_result_path=args.release_gate_result,
            candidate_evidence_path=args.candidate_evidence,
            authorization_record_path=args.authorization_record,
            publication_run_path=args.publication_run,
            publication_jobs_path=args.publication_jobs,
            release_path=args.release,
            immutable_releases_path=args.immutable_releases,
        )
    except (OSError, KeyError, json.JSONDecodeError, RecoveryError) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
