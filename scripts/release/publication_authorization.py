#!/usr/bin/env python3
"""Build and validate runtime publication authorization provenance.

The protected GitHub ``release`` environment is the authorization authority.
Nothing committed to the release revision claims that the same revision was
already approved. Instead, this tool:

1. closes candidate, workflow-run, and Phase 7 evidence before approval; and
2. after the environment review, records the GitHub approval event together
   with that immutable candidate evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SHA256_PATTERN = re.compile(r"[0-9a-f]{64}")
REVISION_PATTERN = re.compile(r"[0-9a-f]{40}")
TIMESTAMP_PATTERN = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z")
CANDIDATE_KIND = "retrievalkit-publication-candidate-evidence"
AUTHORIZATION_KIND = "retrievalkit-publication-authorization-provenance"
WORKFLOW_PATHS = {
    "candidate": ".github/workflows/release-candidate.yml",
    "scheduled_gate": ".github/workflows/regression-full.yml",
    "release_gate": ".github/workflows/release-qualification.yml",
    "publication": ".github/workflows/publish-release.yml",
}


class AuthorizationError(RuntimeError):
    """Publication authorization evidence failed closed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AuthorizationError(message)


def load_object(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AuthorizationError(f"cannot read JSON '{path}': {error}") from error
    require(isinstance(value, dict), f"'{path}' must contain one JSON object")
    return value


def load_array(path: Path) -> list[Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AuthorizationError(f"cannot read JSON '{path}': {error}") from error
    require(isinstance(value, list), f"'{path}' must contain one JSON array")
    return value


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode()


def write_object(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_bytes(value))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(
        set(value) == expected,
        f"{label} keys differ: missing={sorted(expected - set(value))}, "
        f"extra={sorted(set(value) - expected)}",
    )


def positive_run_id(value: Any, label: str) -> int:
    require(
        isinstance(value, int) and not isinstance(value, bool) and value > 0,
        f"{label} must be a positive integer",
    )
    return value


def timestamp(value: Any, label: str) -> datetime:
    require(
        isinstance(value, str) and TIMESTAMP_PATTERN.fullmatch(value) is not None,
        f"{label} timestamp is invalid",
    )
    try:
        return datetime.fromisoformat(value.removesuffix("Z") + "+00:00")
    except ValueError as error:
        raise AuthorizationError(f"{label} timestamp is invalid") from error


def validate_identity(repository: str, tag: str, revision: str) -> None:
    require(
        bool(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)),
        "repository must be an owner/name identity",
    )
    require(bool(re.fullmatch(r"v\d+\.\d+\.\d+", tag)), "release tag is invalid")
    require(REVISION_PATTERN.fullmatch(revision) is not None, "source revision is invalid")


def validate_run(
    run: dict[str, Any],
    *,
    label: str,
    run_id: int,
    repository: str,
    revision: str,
    workflow_path: str,
    allowed_events: set[str],
    require_completed: bool,
) -> dict[str, Any]:
    require(positive_run_id(run.get("id"), f"{label} run id") == run_id, f"{label} run ID mismatch")
    require(run.get("head_sha") == revision, f"{label} run revision mismatch")
    require(run.get("path") == workflow_path, f"{label} workflow path mismatch")
    require(run.get("event") in allowed_events, f"{label} workflow event is not allowed")
    repository_value = run.get("repository")
    require(isinstance(repository_value, dict), f"{label} run repository metadata missing")
    require(repository_value.get("full_name") == repository, f"{label} run repository mismatch")
    if require_completed:
        require(run.get("status") == "completed", f"{label} run is not completed")
        require(run.get("conclusion") == "success", f"{label} run did not succeed")
    else:
        require(
            run.get("status") in {"queued", "in_progress", "completed"},
            f"{label} publication run status is invalid",
        )
        if run.get("status") == "completed":
            require(
                run.get("conclusion") in {None, "success"},
                f"{label} publication run has a failed conclusion",
            )
    html_url = run.get("html_url")
    require(isinstance(html_url, str) and html_url.startswith("https://github.com/"), f"{label} run URL missing")
    run_started_at = run.get("run_started_at")
    timestamp(run_started_at, f"{label} run start")
    return {
        "run_id": run_id,
        "run_attempt": positive_run_id(run.get("run_attempt", 1), f"{label} run attempt"),
        "workflow_path": workflow_path,
        "event": run["event"],
        "head_sha": revision,
        "html_url": html_url,
        "run_started_at": run_started_at,
    }


def validate_gate_result(
    path: Path,
    *,
    tier: str,
    revision: str,
    label: str,
) -> dict[str, Any]:
    result = load_object(path)
    require(result.get("tier") == tier, f"{label} tier mismatch")
    require(result.get("overall_status") == "passed", f"{label} did not pass")
    require(result.get("source_revision") == revision, f"{label} source revision mismatch")
    return {
        "overall_status": "passed",
        "source_revision": revision,
        "tier": tier,
        "sha256": sha256(path),
    }


def bundle_evidence(bundle: Path, tag: str, revision: str) -> dict[str, Any]:
    manifest_path = bundle / "release-manifest.json"
    inventory_path = bundle / "inventory.json"
    checksums_path = bundle / "checksums.sha256"
    manifest = load_object(manifest_path)
    require(manifest.get("tag") == tag, "release bundle tag mismatch")
    require(manifest.get("source_revision") == revision, "release bundle revision mismatch")
    require(manifest.get("publication_ready") is False, "candidate bundle must remain pre-publication")
    artifact_count = manifest.get("artifact_count")
    require(
        isinstance(artifact_count, int)
        and not isinstance(artifact_count, bool)
        and artifact_count > 0,
        "release bundle artifact count is invalid",
    )
    require(
        manifest.get("inventory_sha256") == sha256(inventory_path),
        "release bundle manifest inventory digest mismatch",
    )
    return {
        "artifact_count": artifact_count,
        "checksums_sha256": sha256(checksums_path),
        "inventory_sha256": sha256(inventory_path),
        "release_manifest_sha256": sha256(manifest_path),
    }


def build_candidate_evidence(args: argparse.Namespace) -> dict[str, Any]:
    validate_identity(args.repository, args.tag, args.source_revision)
    candidate_run_id = positive_run_id(args.candidate_run_id, "candidate run id")
    scheduled_run_id = positive_run_id(args.scheduled_run_id, "scheduled run id")
    release_gate_run_id = positive_run_id(args.release_gate_run_id, "release gate run id")
    runs = {
        "candidate": validate_run(
            load_object(args.candidate_run_json),
            label="candidate",
            run_id=candidate_run_id,
            repository=args.repository,
            revision=args.source_revision,
            workflow_path=WORKFLOW_PATHS["candidate"],
            allowed_events={"workflow_dispatch"},
            require_completed=True,
        ),
        "scheduled_gate": validate_run(
            load_object(args.scheduled_run_json),
            label="scheduled gate",
            run_id=scheduled_run_id,
            repository=args.repository,
            revision=args.source_revision,
            workflow_path=WORKFLOW_PATHS["scheduled_gate"],
            allowed_events={"schedule", "workflow_dispatch"},
            require_completed=True,
        ),
        "release_gate": validate_run(
            load_object(args.release_gate_run_json),
            label="release gate",
            run_id=release_gate_run_id,
            repository=args.repository,
            revision=args.source_revision,
            workflow_path=WORKFLOW_PATHS["release_gate"],
            allowed_events={"workflow_dispatch"},
            require_completed=True,
        ),
    }
    evidence = {
        "schema_version": 1,
        "kind": CANDIDATE_KIND,
        "release": {
            "repository": args.repository,
            "source_revision": args.source_revision,
            "tag": args.tag,
        },
        "runs": runs,
        "gate_results": {
            "scheduled_gate": validate_gate_result(
                args.scheduled_result,
                tier="scheduled_full",
                revision=args.source_revision,
                label="scheduled gate result",
            ),
            "release_gate": validate_gate_result(
                args.release_gate_result,
                tier="release",
                revision=args.source_revision,
                label="release gate result",
            ),
        },
        "bundle": bundle_evidence(args.bundle, args.tag, args.source_revision),
    }
    validate_candidate_evidence(
        evidence,
        repository=args.repository,
        tag=args.tag,
        revision=args.source_revision,
        candidate_run_id=candidate_run_id,
        scheduled_run_id=scheduled_run_id,
        release_gate_run_id=release_gate_run_id,
    )
    return evidence


def validate_candidate_evidence(
    evidence: dict[str, Any],
    *,
    repository: str,
    tag: str,
    revision: str,
    candidate_run_id: int,
    scheduled_run_id: int,
    release_gate_run_id: int,
) -> None:
    require_keys(evidence, {"schema_version", "kind", "release", "runs", "gate_results", "bundle"}, "candidate evidence")
    require(evidence["schema_version"] == 1, "candidate evidence schema version mismatch")
    require(evidence["kind"] == CANDIDATE_KIND, "candidate evidence kind mismatch")
    require(
        evidence["release"]
        == {"repository": repository, "source_revision": revision, "tag": tag},
        "candidate release identity mismatch",
    )
    runs = evidence["runs"]
    require(isinstance(runs, dict), "candidate workflow runs are invalid")
    require_keys(runs, {"candidate", "scheduled_gate", "release_gate"}, "candidate workflow runs")
    for key, expected_id in (
        ("candidate", candidate_run_id),
        ("scheduled_gate", scheduled_run_id),
        ("release_gate", release_gate_run_id),
    ):
        row = runs[key]
        require(isinstance(row, dict), f"{key} run evidence is invalid")
        require(row.get("run_id") == expected_id, f"{key} run evidence ID mismatch")
        require(row.get("head_sha") == revision, f"{key} run evidence revision mismatch")
        require(row.get("workflow_path") == WORKFLOW_PATHS[key], f"{key} workflow evidence path mismatch")
        require(
            isinstance(row.get("html_url"), str)
            and row["html_url"].startswith(f"https://github.com/{repository}/actions/runs/"),
            f"{key} run evidence URL mismatch",
        )
    gates = evidence["gate_results"]
    require(isinstance(gates, dict), "candidate gate evidence is invalid")
    require_keys(gates, {"scheduled_gate", "release_gate"}, "candidate gate results")
    for key, tier in (("scheduled_gate", "scheduled_full"), ("release_gate", "release")):
        row = gates[key]
        require(isinstance(row, dict), f"{key} result evidence is invalid")
        require(row.get("tier") == tier, f"{key} result evidence tier mismatch")
        require(row.get("overall_status") == "passed", f"{key} result evidence did not pass")
        require(row.get("source_revision") == revision, f"{key} result evidence revision mismatch")
        require(SHA256_PATTERN.fullmatch(str(row.get("sha256", ""))) is not None, f"{key} result digest is invalid")
    bundle = evidence["bundle"]
    require(isinstance(bundle, dict), "candidate bundle evidence is invalid")
    require_keys(
        bundle,
        {"artifact_count", "checksums_sha256", "inventory_sha256", "release_manifest_sha256"},
        "candidate bundle evidence",
    )
    require(
        isinstance(bundle["artifact_count"], int)
        and not isinstance(bundle["artifact_count"], bool)
        and bundle["artifact_count"] > 0,
        "candidate bundle artifact count is invalid",
    )
    for name in ("checksums_sha256", "inventory_sha256", "release_manifest_sha256"):
        require(SHA256_PATTERN.fullmatch(str(bundle[name])) is not None, f"candidate bundle {name} is invalid")


def approval_evidence(
    approvals: list[Any],
    environment: str,
    *,
    not_before: str,
) -> list[dict[str, Any]]:
    attempt_started_at = timestamp(not_before, "publication run start")
    accepted: list[dict[str, Any]] = []
    for value in approvals:
        if not isinstance(value, dict) or value.get("state") != "approved":
            continue
        environments = value.get("environments")
        if not isinstance(environments, list) or not any(
            isinstance(item, dict) and item.get("name") == environment
            for item in environments
        ):
            continue
        user = value.get("user")
        login = user.get("login") if isinstance(user, dict) else None
        created_at = value.get("created_at")
        require(isinstance(login, str) and login, "environment approval reviewer identity missing")
        approved_at = timestamp(created_at, "environment approval")
        if approved_at < attempt_started_at:
            continue
        accepted.append(
            {
                "reviewer": login,
                "state": "approved",
                "created_at": created_at,
                "comment": value.get("comment") if isinstance(value.get("comment"), str) else "",
            }
        )
    require(
        accepted,
        f"no approved required-reviewer event found for GitHub environment '{environment}'",
    )
    return sorted(accepted, key=lambda row: (row["created_at"], row["reviewer"], row["comment"]))


def build_authorization_record(args: argparse.Namespace) -> dict[str, Any]:
    validate_identity(args.repository, args.tag, args.source_revision)
    candidate_run_id = positive_run_id(args.candidate_run_id, "candidate run id")
    scheduled_run_id = positive_run_id(args.scheduled_run_id, "scheduled run id")
    release_gate_run_id = positive_run_id(args.release_gate_run_id, "release gate run id")
    publication_run_id = positive_run_id(args.publication_run_id, "publication run id")
    publication_run_attempt = positive_run_id(args.publication_run_attempt, "publication run attempt")
    candidate = load_object(args.candidate_evidence)
    validate_candidate_evidence(
        candidate,
        repository=args.repository,
        tag=args.tag,
        revision=args.source_revision,
        candidate_run_id=candidate_run_id,
        scheduled_run_id=scheduled_run_id,
        release_gate_run_id=release_gate_run_id,
    )
    publication_run = validate_run(
        load_object(args.publication_run_json),
        label="publication",
        run_id=publication_run_id,
        repository=args.repository,
        revision=args.source_revision,
        workflow_path=WORKFLOW_PATHS["publication"],
        allowed_events={"workflow_dispatch"},
        require_completed=False,
    )
    require(
        publication_run["run_attempt"] == publication_run_attempt,
        "publication run attempt mismatch",
    )
    expected_workflow_ref = (
        f"{args.repository}/{WORKFLOW_PATHS['publication']}@refs/tags/{args.tag}"
    )
    require(args.workflow_ref == expected_workflow_ref, "publication workflow ref must be the exact signed tag")
    generated_at = args.generated_at or datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    require(TIMESTAMP_PATTERN.fullmatch(generated_at) is not None, "authorization generation timestamp is invalid")
    approvals = approval_evidence(
        load_array(args.approvals_json),
        args.environment,
        not_before=publication_run["run_started_at"],
    )
    record = {
        "schema_version": 1,
        "kind": AUTHORIZATION_KIND,
        "decision": "approved",
        "release": candidate["release"],
        "authority": {
            "type": "github_environment_required_reviewer",
            "environment": args.environment,
            "approvals": approvals,
        },
        "publication_run": {
            **publication_run,
            "workflow_ref": args.workflow_ref,
            "actor": args.actor,
            "triggering_actor": args.triggering_actor,
        },
        "candidate_evidence": candidate,
        "candidate_evidence_sha256": sha256(args.candidate_evidence),
        "generated_at": generated_at,
    }
    validate_authorization_record(
        record,
        candidate_path=args.candidate_evidence,
        repository=args.repository,
        tag=args.tag,
        revision=args.source_revision,
        candidate_run_id=candidate_run_id,
        scheduled_run_id=scheduled_run_id,
        release_gate_run_id=release_gate_run_id,
        publication_run_id=publication_run_id,
        publication_run_attempt=publication_run_attempt,
    )
    return record


def validate_authorization_record(
    record: dict[str, Any],
    *,
    candidate_path: Path,
    repository: str,
    tag: str,
    revision: str,
    candidate_run_id: int,
    scheduled_run_id: int,
    release_gate_run_id: int,
    publication_run_id: int,
    publication_run_attempt: int,
) -> None:
    require_keys(
        record,
        {
            "schema_version",
            "kind",
            "decision",
            "release",
            "authority",
            "publication_run",
            "candidate_evidence",
            "candidate_evidence_sha256",
            "generated_at",
        },
        "authorization record",
    )
    require(record["schema_version"] == 1, "authorization schema version mismatch")
    require(record["kind"] == AUTHORIZATION_KIND, "authorization record kind mismatch")
    require(record["decision"] == "approved", "publication decision is not approved")
    require(
        record["release"]
        == {"repository": repository, "source_revision": revision, "tag": tag},
        "authorization release identity mismatch",
    )
    require(
        record["candidate_evidence_sha256"] == sha256(candidate_path),
        "authorization candidate-evidence digest mismatch",
    )
    require(
        record["candidate_evidence"] == load_object(candidate_path),
        "authorization embedded candidate evidence differs",
    )
    validate_candidate_evidence(
        record["candidate_evidence"],
        repository=repository,
        tag=tag,
        revision=revision,
        candidate_run_id=candidate_run_id,
        scheduled_run_id=scheduled_run_id,
        release_gate_run_id=release_gate_run_id,
    )
    authority = record["authority"]
    require(isinstance(authority, dict), "authorization authority is invalid")
    require(authority.get("type") == "github_environment_required_reviewer", "authorization authority type mismatch")
    require(authority.get("environment") == "release", "authorization must come from the release environment")
    approvals = authority.get("approvals")
    require(isinstance(approvals, list) and approvals, "authorization approval evidence is empty")
    for approval in approvals:
        require(
            isinstance(approval, dict)
            and approval.get("state") == "approved"
            and isinstance(approval.get("reviewer"), str)
            and bool(approval["reviewer"])
            and isinstance(approval.get("created_at"), str)
            and TIMESTAMP_PATTERN.fullmatch(approval["created_at"]) is not None,
            "authorization approval evidence is invalid",
        )
    run = record["publication_run"]
    require(isinstance(run, dict), "publication run evidence is invalid")
    require(run.get("run_id") == publication_run_id, "authorization publication run ID mismatch")
    require(run.get("run_attempt") == publication_run_attempt, "authorization publication run attempt mismatch")
    require(run.get("head_sha") == revision, "authorization publication run revision mismatch")
    require(run.get("workflow_path") == WORKFLOW_PATHS["publication"], "authorization workflow path mismatch")
    run_start = timestamp(
        run.get("run_started_at"),
        "authorization publication run start",
    )
    for approval in approvals:
        require(
            timestamp(approval["created_at"], "authorization approval") >= run_start,
            "authorization approval predates the current publication run attempt",
        )
    require(
        run.get("workflow_ref")
        == f"{repository}/{WORKFLOW_PATHS['publication']}@refs/tags/{tag}",
        "authorization workflow ref mismatch",
    )
    require(
        isinstance(record["generated_at"], str)
        and TIMESTAMP_PATTERN.fullmatch(record["generated_at"]) is not None,
        "authorization generation timestamp is invalid",
    )


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    subcommands = root.add_subparsers(dest="command", required=True)

    candidate = subcommands.add_parser("candidate")
    candidate.add_argument("--repository", required=True)
    candidate.add_argument("--tag", required=True)
    candidate.add_argument("--source-revision", required=True)
    candidate.add_argument("--candidate-run-id", type=int, required=True)
    candidate.add_argument("--candidate-run-json", type=Path, required=True)
    candidate.add_argument("--scheduled-run-id", type=int, required=True)
    candidate.add_argument("--scheduled-run-json", type=Path, required=True)
    candidate.add_argument("--release-gate-run-id", type=int, required=True)
    candidate.add_argument("--release-gate-run-json", type=Path, required=True)
    candidate.add_argument("--bundle", type=Path, required=True)
    candidate.add_argument("--scheduled-result", type=Path, required=True)
    candidate.add_argument("--release-gate-result", type=Path, required=True)
    candidate.add_argument("--output", type=Path, required=True)

    authorize = subcommands.add_parser("authorize")
    authorize.add_argument("--repository", required=True)
    authorize.add_argument("--tag", required=True)
    authorize.add_argument("--source-revision", required=True)
    authorize.add_argument("--candidate-run-id", type=int, required=True)
    authorize.add_argument("--scheduled-run-id", type=int, required=True)
    authorize.add_argument("--release-gate-run-id", type=int, required=True)
    authorize.add_argument("--candidate-evidence", type=Path, required=True)
    authorize.add_argument("--publication-run-id", type=int, required=True)
    authorize.add_argument("--publication-run-attempt", type=int, required=True)
    authorize.add_argument("--publication-run-json", type=Path, required=True)
    authorize.add_argument("--approvals-json", type=Path, required=True)
    authorize.add_argument("--workflow-ref", required=True)
    authorize.add_argument("--actor", required=True)
    authorize.add_argument("--triggering-actor", required=True)
    authorize.add_argument("--environment", default="release")
    authorize.add_argument("--generated-at")
    authorize.add_argument("--output", type=Path, required=True)

    validate = subcommands.add_parser("validate")
    validate.add_argument("--record", type=Path, required=True)
    validate.add_argument("--candidate-evidence", type=Path, required=True)
    validate.add_argument("--repository", required=True)
    validate.add_argument("--tag", required=True)
    validate.add_argument("--source-revision", required=True)
    validate.add_argument("--candidate-run-id", type=int, required=True)
    validate.add_argument("--scheduled-run-id", type=int, required=True)
    validate.add_argument("--release-gate-run-id", type=int, required=True)
    validate.add_argument("--publication-run-id", type=int, required=True)
    validate.add_argument("--publication-run-attempt", type=int, required=True)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "candidate":
            value = build_candidate_evidence(args)
            write_object(args.output, value)
        elif args.command == "authorize":
            value = build_authorization_record(args)
            write_object(args.output, value)
        else:
            validate_authorization_record(
                load_object(args.record),
                candidate_path=args.candidate_evidence,
                repository=args.repository,
                tag=args.tag,
                revision=args.source_revision,
                candidate_run_id=args.candidate_run_id,
                scheduled_run_id=args.scheduled_run_id,
                release_gate_run_id=args.release_gate_run_id,
                publication_run_id=args.publication_run_id,
                publication_run_attempt=args.publication_run_attempt,
            )
        print(json.dumps({"result": "PASS", "command": args.command}, sort_keys=True))
        return 0
    except (AuthorizationError, KeyError, OSError, TypeError, ValueError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
