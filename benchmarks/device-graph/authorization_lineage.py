"""Closed authorization-lineage rules for resumable Phase 4b collection."""

from __future__ import annotations

import fnmatch
import hashlib
import json
from pathlib import Path
from typing import Any

PRESERVED_V3_PATH_PATTERNS = (
    "devices/iphone17-pro-max/supported/*/*/query/session-*.json",
    "devices/iphone17-pro-max/supported/10k-384d-v3/f32/lifecycle/prepare.json",
    "devices/iphone17-pro-max/supported/10k-384d-v3/f32/lifecycle/build/*.json",
    "devices/iphone17-pro-max/supported/10k-384d-v3/f32/lifecycle/save/*.json",
)


class LineageError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def is_preserved_v3_path(relative_path: str) -> bool:
    return any(
        fnmatch.fnmatchcase(relative_path, pattern)
        for pattern in PRESERVED_V3_PATH_PATTERNS
    )


def preserved_artifact_entries(root: Path) -> list[dict[str, str]]:
    devices = root / "devices"
    if not devices.is_dir():
        raise LineageError("preserved Phase 4b device evidence root is missing")
    entries = [
        {
            "path": path.relative_to(root).as_posix(),
            "sha256": sha256_file(path),
        }
        for path in sorted(devices.rglob("*.json"))
        if is_preserved_v3_path(path.relative_to(root).as_posix())
    ]
    if not entries:
        raise LineageError("preserved Phase 4b evidence set is empty")
    return entries


def artifact_set_sha256(entries: list[dict[str, str]]) -> str:
    payload = json.dumps(
        entries,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def validate_lineage(
    authorization: dict[str, Any],
    prior_authorization_sha256: str,
    artifact_root: Path,
) -> dict[str, Any]:
    lineage = authorization.get("evidence_lineage")
    if not isinstance(lineage, dict):
        raise LineageError("current authorization is missing evidence_lineage")
    if lineage.get("prior_authorization_sha256") != prior_authorization_sha256:
        raise LineageError("prior authorization hash does not match lineage")
    if tuple(lineage.get("preserved_path_patterns", ())) != PRESERVED_V3_PATH_PATTERNS:
        raise LineageError("preserved v3 path contract changed")
    if lineage.get("current_authorization_covers_unmatched_required_paths") is not True:
        raise LineageError("current authorization does not cover unmatched paths")
    if lineage.get("preserve_prior_artifact_bytes") is not True:
        raise LineageError("prior artifact preservation is not required")
    allowed_builds = lineage.get("prior_allowed_os_builds")
    if allowed_builds != ["23F81", "23F84"]:
        raise LineageError("prior OS-build variance is not the owner-approved set")

    entries = preserved_artifact_entries(artifact_root)
    if lineage.get("preserved_artifact_count") != len(entries):
        raise LineageError("preserved artifact count mismatch")
    if lineage.get("preserved_artifact_set_sha256") != artifact_set_sha256(entries):
        raise LineageError("preserved artifact bytes or inventory changed")
    return lineage
