#!/usr/bin/env python3
"""Validate README product claims against the frozen Phase 6 claim register."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import date
from pathlib import Path
from typing import Any


class ValidationError(RuntimeError):
    """README evidence or product-status validation failed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def claim_blocks(readme: str) -> dict[str, str]:
    matches = re.findall(
        r"<!-- claim:([A-Z0-9-]+) -->\s*(.*?)\s*<!-- /claim -->",
        readme,
        flags=re.DOTALL,
    )
    require(len(matches) == len({claim_id for claim_id, _ in matches}), "duplicate README claim marker")
    require(readme.count("<!-- claim:") == len(matches), "unclosed README claim marker")
    return dict(matches)


def validate_headings(readme: str) -> None:
    levels = [len(match.group(1)) for match in re.finditer(r"^(#{1,6})\s+\S", readme, re.MULTILINE)]
    require(levels and levels[0] == 1, "README must begin with a level-one heading")
    require(levels.count(1) == 1, "README must contain exactly one level-one heading")
    for previous, current in zip(levels, levels[1:]):
        require(current <= previous + 1, "README heading levels must not skip")


def validate_local_links(repo: Path, readme: str) -> None:
    for target in re.findall(r"\[[^]]+\]\(([^)]+)\)", readme):
        if target.startswith(("http://", "https://", "#")):
            continue
        path_text = target.split("#", 1)[0]
        require((repo / path_text).exists(), f"README link target is missing: {path_text}")


def validate_status_labels(readme: str) -> None:
    for sdk in ("Swift `RetrievalKit`", "Swift `RetrievalKitGraph`", "Swift `EmbeddingKit`", "Swift `RetrievalKitPipeline`", "Python `retrievalkit`", "Python `retrievalkit-graph`"):
        require(re.search(rf"\| {re.escape(sdk)} \|.*\| \*\*Available from source\*\* \|", readme), f"incorrect source status for {sdk}")
    for sdk in (
        "TypeScript `@gungorbasa/retrievalkit`",
        "TypeScript `@gungorbasa/retrievalkit-graph`",
    ):
        require(
            re.search(
                rf"\| {re.escape(sdk)} \|.*\| \*\*Available from source; bootstrap placeholder only\*\* \|",
                readme,
            ),
            f"incorrect npm bootstrap status for {sdk}",
        )
    for sdk in (
        "Kotlin/JVM `io.github.gungorbasa:retrievalkit`",
        "Kotlin/JVM `io.github.gungorbasa:retrievalkit-graph`",
    ):
        require(
            re.search(
                rf"\| {re.escape(sdk)} \|.*\| \*\*Available from source; Maven unpublished\*\* \|",
                readme,
            ),
            f"incorrect Maven source status for {sdk}",
        )
    require(
        re.search(
            r"\| Browser `@gungorbasa/retrievalkit-browser` \|.*\| "
            r"\*\*Available from source; v0\.1\.0 candidate; "
            r"registry bootstrap pending\*\* \|",
            readme,
        ),
        "incorrect browser retrieval release status",
    )
    for sdk in (
        "Android `io.github.gungorbasa:retrievalkit-android`",
        "Android `io.github.gungorbasa:retrievalkit-graph-android`",
    ):
        require(
            re.search(
                rf"\| {re.escape(sdk)} \|.*\| "
                r"\*\*Preview from source; Maven unpublished; live-device unqualified\*\* \|",
                readme,
            ),
            f"incorrect Android preview status for {sdk}",
        )
    require(
        re.search(
            r"\| Android `io\.github\.gungorbasa:retrievalkit-embedding-android` "
            r"\|.*\| \*\*v0\.1\.0 preview candidate; "
            r"live-device inference unqualified\*\* \|",
            readme,
        ),
        "incorrect Android embedding preview status",
    )
    require(
        re.search(
            r"public SwiftPM, PyPI, npm, and Maven publication\s+remain blocked",
            readme,
            re.IGNORECASE,
        )
        is not None,
        "README must deny public registry availability for provisional wrappers",
    )


def validate_claims(repo: Path, readme: str, mapping: dict[str, Any], as_of: date) -> None:
    register_path = repo / mapping["claim_register"]
    require(sha256_file(register_path) == mapping["claim_register_sha256"], "Phase 6 claim register identity changed")
    register = load_json(register_path)
    require(date.fromisoformat(mapping["expires_on"]) >= as_of, "README claim mapping expired")
    require(register["expires_on"] == mapping["expires_on"], "README/register expiry mismatch")
    rows = {row["claim_id"]: row for row in register["claims"]}
    blocks = claim_blocks(readme)
    require(set(blocks) == set(mapping["claims"]), "README claim-marker inventory mismatch")
    for claim_id, tokens in mapping["claims"].items():
        require(claim_id in rows, f"unknown Phase 6 claim: {claim_id}")
        claim = rows[claim_id]
        require(claim["status"] == "permitted", f"README uses non-permitted claim: {claim_id}")
        require(claim["source_revision"] == mapping["source_revision"], f"source revision changed for {claim_id}")
        require(date.fromisoformat(claim["expires_on"]) >= as_of, f"expired README claim: {claim_id}")
        normalized_block = re.sub(r"\s+", " ", blocks[claim_id])
        for token in tokens:
            require(token in normalized_block, f"{claim_id} missing required qualifier/value: {token}")

    lowered = readme.lower()
    prohibited = {
        "universal superiority": r"retrievalkit (?:is|is always|is universally) (?:faster|better) than",
        "graph performance winner": r"(?:beats|faster than) (?:the )?graph baseline",
        "100K support": r"(?:supports|passes|qualified for) 100k",
        "USearch timing": r"usearch.{0,120}(?:faster|latency|speedup|performance advantage)",
        "current-source transfer": r"(?:current|latest) (?:checkout|source).{0,80}(?:7\.17|7\.60|10\.38)",
    }
    for label, pattern in prohibited.items():
        require(re.search(pattern, lowered, re.DOTALL) is None, f"prohibited README claim: {label}")
    require("historical observations" in lowered, "README must label frozen measurements as historical")
    require("not measurements of the current checkout" in lowered, "README must deny transfer to current source")
    require(mapping["source_revision"] in readme, "README missing full frozen source revision")
    require(mapping["expires_on"] in readme, "README missing claim expiry")


def validate(repo: Path, mapping_path: Path, as_of: date) -> dict[str, Any]:
    mapping = load_json(mapping_path)
    readme_path = repo / mapping["readme"]
    readme = readme_path.read_text(encoding="utf-8")
    validate_headings(readme)
    validate_local_links(repo, readme)
    validate_status_labels(readme)
    validate_claims(repo, readme, mapping, as_of)
    require("Run from source" in readme, "README missing source CTA")
    require("See validated benchmarks" in readme, "README missing benchmark CTA")
    require("mutually exclusive within one process" in readme, "README missing Python aggregate isolation")
    return {
        "schema_version": 1,
        "result": "PASS",
        "readme_sha256": sha256_file(readme_path),
        "claim_mapping_sha256": sha256_file(mapping_path),
        "validated_claim_ids": sorted(mapping["claims"]),
        "as_of_date": as_of.isoformat(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--mapping", type=Path, default=Path("benchmarks/publication/readme-claims-v1.json"))
    parser.add_argument("--as-of-date", type=date.fromisoformat, default=date.today())
    args = parser.parse_args()
    repo = args.repo.resolve()
    mapping = args.mapping if args.mapping.is_absolute() else repo / args.mapping
    try:
        result = validate(repo, mapping, args.as_of_date)
    except (OSError, KeyError, TypeError, ValueError, ValidationError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
