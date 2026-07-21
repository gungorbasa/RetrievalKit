#!/usr/bin/env python3
"""Validate explicit authorization for Phase 7 evidence-only release qualification."""

from __future__ import annotations

import argparse
from datetime import date
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

SUPPORTED = ["10k-384d-v3", "25k-384d-v3", "50k-384d-v3"]


class AuthorizationError(RuntimeError):
    pass


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise AuthorizationError(f"{path}: expected a JSON object")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--authorization", type=Path, required=True)
    parser.add_argument("--observation", type=Path, required=True)
    args = parser.parse_args()
    try:
        authorization = load(args.authorization)
        observation = load(args.observation)
        expected_fields = {
            "artifact_type",
            "authorized",
            "authorized_encodings",
            "authorized_workloads",
            "device_commands_authorized",
            "evidence_only",
            "expires_on",
            "observation_sha256",
            "owner",
            "schema_version",
        }
        if set(authorization) != expected_fields:
            raise AuthorizationError("authorization field set differs")
        if authorization["artifact_type"] != "phase7_release_qualification_authorization":
            raise AuthorizationError("authorization type differs")
        if authorization["authorized"] is not True or authorization["evidence_only"] is not True:
            raise AuthorizationError("release evidence validation is not explicitly authorized")
        if authorization["device_commands_authorized"] is not False:
            raise AuthorizationError("device commands cannot be authorized by this workflow")
        if authorization["authorized_workloads"] != SUPPORTED:
            raise AuthorizationError("authorization must contain exactly 10K/25K/50K")
        if authorization["authorized_encodings"] != ["f32", "i8"]:
            raise AuthorizationError("authorization must contain exactly F32/I8")
        if authorization["observation_sha256"] != sha256(args.observation):
            raise AuthorizationError("authorization does not bind the observation")
        if not isinstance(authorization["owner"], str) or not authorization["owner"].strip():
            raise AuthorizationError("authorization owner is missing")
        if date.fromisoformat(authorization["expires_on"]) < date.today():
            raise AuthorizationError("release authorization is expired")
        encoded = json.dumps([authorization, observation], sort_keys=True).lower()
        if "100k" in encoded:
            raise AuthorizationError("100K physical-device evidence is permanently excluded")
        platform = observation.get("platform", {})
        if platform.get("device_identifier") != "iPhone18,2":
            raise AuthorizationError("release observation lacks the controlled device qualifier")
        for name in ("os", "toolchain", "source_revision", "sample_count"):
            if not platform.get(name):
                raise AuthorizationError(f"release observation lacks platform qualifier '{name}'")
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError, AuthorizationError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"result": "PASS", "authorization_sha256": sha256(args.authorization)}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
