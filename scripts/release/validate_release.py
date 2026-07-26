#!/usr/bin/env python3
"""Validate RetrievalKit release metadata, artifacts, and publication authority."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import plistlib
import re
import sys
import zipfile
from pathlib import Path
from typing import Any


ZERO_CHECKSUM = "0" * 64
ACTION_PATTERN = re.compile(r"uses:\s+[^\s@]+@([0-9a-f]{40})(?:\s|$)")
BUNDLE_LEGAL_FILES = {"LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}


class ValidationError(RuntimeError):
    """Release validation failed closed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def static_validation(repo: Path) -> dict[str, Any]:
    config = load_json(repo / "release/release-v0.1.0.json")
    version = (repo / "VERSION").read_text().strip()
    semver = r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)"
    require(re.fullmatch(semver, version) is not None, "VERSION is not semantic")
    require(version == "0.1.0" == config["version"], "release version mismatch")
    require(f"## {version} - Unreleased preview" in (repo / "CHANGELOG.md").read_text(), "changelog version mismatch")
    require((repo / f"docs/product/v{version}-migration.md").is_file(), "release migration guide missing")
    cargo = (repo / "Cargo.toml").read_text()
    require(f'version = "{version}"' in cargo, "Cargo workspace version mismatch")
    for manifest in sorted((repo / "crates").glob("*/Cargo.toml")):
        require("version.workspace = true" in manifest.read_text(), f"crate does not inherit workspace version: {manifest}")
    for pyproject in (repo / "wrappers/python/pyproject.toml", repo / "wrappers/python-graph/pyproject.toml"):
        require(f'version = "{version}"' in pyproject.read_text(), f"Python version mismatch: {pyproject}")
    swift_packages = config["apple"]["packages"]
    require(set(swift_packages) == {"unified"}, "Swift package set changed")
    package_texts: dict[str, str] = {}
    for capability, package_config in swift_packages.items():
        manifest = repo / package_config["manifest"]
        require(manifest.is_file(), f"Swift {capability} package manifest missing")
        package = manifest.read_text()
        package_texts[capability] = package
        require(
            f'let version = "{version}"' in package,
            f"Swift {capability} package version mismatch",
        )
        require(
            f'name: "{package_config["name"]}"' in package,
            f"Swift {capability} package name mismatch",
        )
        for product in package_config["products"]:
            require(
                f'.library(name: "{product}"' in package,
                f"Swift {capability} package missing product: {product}",
            )
        require(
            "RETRIEVALKIT_USE_LOCAL_ARTIFACTS" in package,
            f"Swift {capability} package missing explicit local-artifact mode",
        )
    swift_package = package_texts["unified"]
    require(
        swift_package.count(".binaryTarget(") == 2,
        "Swift package must declare one local and one remote form of the same native target",
    )
    require(
        "RetrievalKitGraphFFI.xcframework.zip" in swift_package
        and "RetrievalKitFFI.xcframework.zip" not in swift_package,
        "Swift package must resolve only the graph-capable native aggregate",
    )
    require(
        'name: "RetrievalKitIngest"' not in swift_package,
        "Swift text chunking must be part of RetrievalKit rather than a separate product",
    )
    require(
        "links exactly one RetrievalKitGraphFFI" in config["apple"]["native_aggregate_rule"],
        "aggregate linkage rule missing",
    )
    require("VERSION" in (repo / "scripts/build-xcframework.sh").read_text(), "XCFramework metadata does not read VERSION")
    require(config["python"]["implementations"] == ["cp310", "cp311", "cp312", "cp313", "cp314"], "Python release matrix changed")
    require(config["python"]["distributions"] == ["retrievalkit", "retrievalkit-graph"], "Python distribution set changed")
    require("mutually exclusive" in (repo / "wrappers/python/python/retrievalkit/__init__.py").read_text(), "base Python co-install diagnostic missing")
    require("mutually exclusive" in (repo / "wrappers/python-graph/python/retrievalkit_graph/__init__.py").read_text(), "graph Python co-install diagnostic missing")
    validate_markdown_links(repo)
    validate_workflows(repo)
    checksums = {name: row["swiftpm_checksum"] for name, row in config["apple"]["artifacts"].items()}
    for name, checksum in checksums.items():
        require(re.fullmatch(r"[0-9a-f]{64}", checksum) is not None, f"invalid SwiftPM checksum: {name}")
        require(
            swift_package.count(checksum) >= 1,
            f"Swift package checksum mismatch: {name}",
        )
    return {"version": version, "swiftpm_checksums": checksums, "publication_blockers": publication_blockers(repo, config)}


def validate_markdown_links(repo: Path) -> None:
    paths = [
        repo / "README.md",
        repo / "CONTRIBUTING.md",
        repo / "SECURITY.md",
        repo / "docs/README.md",
        repo / "docs/product/release-process.md",
        repo / "docs/product/release-approval-checklist.md",
        repo / "docs/product/compatibility-policy.md",
        repo / "docs/product/artifact-retention-policy.md",
        repo / "docs/product/v0.1.0-migration.md",
    ]
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for raw_target in re.findall(r"\[[^]]+\]\(([^)]+)\)", text):
            target = raw_target.strip("<>").split("#", 1)[0]
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            require((path.parent / target).exists(), f"Markdown link target is missing: {path.relative_to(repo)} -> {target}")


def validate_workflows(repo: Path) -> None:
    workflows = sorted((repo / ".github/workflows").glob("*.yml"))
    require(workflows, "release validation found no workflows")
    for path in workflows:
        text = path.read_text(encoding="utf-8")
        require("permissions:\n  contents: read" in text, f"least-privilege default missing: {path.name}")
        for line in text.splitlines():
            if "uses:" in line:
                require(ACTION_PATTERN.search(line) is not None, f"action is not pinned to a full commit: {path.name}: {line.strip()}")
    candidate = (repo / ".github/workflows/release-candidate.yml").read_text().lower()
    require("contents: write" not in candidate and "id-token: write" not in candidate, "candidate workflow has publication permissions")
    publication = (repo / ".github/workflows/publish-release.yml").read_text()
    require("environment: release" in publication and "environment: pypi" in publication, "publication jobs lack protected environments")
    require(
        "git verify-tag" in publication
        and "publication_authorization.py candidate" in publication
        and "publication_authorization.py authorize" in publication
        and "--authorization-record" in publication,
        "publication workflow bypasses signed-tag, candidate, or runtime authority validation",
    )
    release_workflows = candidate + publication.lower()
    forbidden_device_commands = ("xcrun devicectl", "ios-deploy", "xcodebuild test-without-building")
    require(not any(command in release_workflows for command in forbidden_device_commands), "distribution workflow contains a physical-device command")


def publication_blockers(repo: Path, config: dict[str, Any]) -> list[str]:
    blockers: list[str] = []
    license_path = repo / "LICENSE"
    notice_path = repo / "NOTICE"
    if not license_path.is_file():
        blockers.append("root LICENSE is absent")
    else:
        license_text = license_path.read_text(encoding="utf-8")
        if "Apache License" not in license_text or "Version 2.0" not in license_text:
            blockers.append("root LICENSE is not Apache-2.0")
    if not notice_path.is_file():
        blockers.append("owner-approved NOTICE is absent")
    elif (
        "Copyright 2026 EGGYOLK YAZILIM TİCARET LİMİTED ŞİRKETİ"
        not in notice_path.read_text(encoding="utf-8")
    ):
        blockers.append("NOTICE lacks the approved company attribution")
    cargo = (repo / "Cargo.toml").read_text(encoding="utf-8")
    if 'license = "Apache-2.0"' not in cargo:
        blockers.append("Cargo metadata is not reconciled with Apache-2.0")
    pyprojects = (
        repo / "wrappers/python/pyproject.toml",
        repo / "wrappers/python-graph/pyproject.toml",
    )
    for pyproject in pyprojects:
        if 'license = { text = "Apache-2.0" }' not in pyproject.read_text(encoding="utf-8"):
            relative_path = pyproject.relative_to(repo)
            blockers.append(
                f"Python metadata is not reconciled with Apache-2.0: {relative_path}"
            )
    if not (repo / "THIRD_PARTY_NOTICES.md").is_file():
        blockers.append("third-party notices are absent")
    for wrapper in ("wrappers/python", "wrappers/python-graph"):
        for legal_name in ("LICENSE", "NOTICE"):
            root_file = repo / legal_name
            copy_file = repo / wrapper / legal_name
            if not root_file.is_file():
                continue
            if not copy_file.is_file() or copy_file.read_bytes() != root_file.read_bytes():
                blockers.append(f"wrapper legal file out of sync: {wrapper}/{legal_name}")
    for checksum in (row["swiftpm_checksum"] for row in config["apple"]["artifacts"].values()):
        if checksum == ZERO_CHECKSUM:
            blockers.append("SwiftPM release checksums are placeholders")
            break
    return blockers


def validate_runtime_authorization(
    repo: Path,
    bundle: Path,
    record: Path,
    candidate_evidence: Path,
    scheduled_result: Path,
    release_gate_result: Path,
    repository: str,
    source_revision: str,
    candidate_run_id: int,
    scheduled_run_id: int,
    release_gate_run_id: int,
    publication_run_id: int,
    publication_run_attempt: int,
) -> None:
    module_path = repo / "scripts/release/publication_authorization.py"
    spec = importlib.util.spec_from_file_location("publication_authorization", module_path)
    require(spec is not None and spec.loader is not None, "cannot load publication authorization validator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    config = load_json(repo / "release/release-v0.1.0.json")
    try:
        module.validate_authorization_record(
            module.load_object(record),
            candidate_path=candidate_evidence,
            repository=repository,
            tag=config["tag"],
            revision=source_revision,
            candidate_run_id=candidate_run_id,
            scheduled_run_id=scheduled_run_id,
            release_gate_run_id=release_gate_run_id,
            publication_run_id=publication_run_id,
            publication_run_attempt=publication_run_attempt,
        )
        candidate = module.load_object(candidate_evidence)
        require(
            candidate["bundle"]
            == module.bundle_evidence(bundle, config["tag"], source_revision),
            "authorized bundle differs from candidate evidence",
        )
        require(
            candidate["gate_results"]["scheduled_gate"]
            == module.validate_gate_result(
                scheduled_result,
                tier="scheduled_full",
                revision=source_revision,
                label="scheduled gate result",
            ),
            "scheduled gate result differs from candidate evidence",
        )
        require(
            candidate["gate_results"]["release_gate"]
            == module.validate_gate_result(
                release_gate_result,
                tier="release",
                revision=source_revision,
                label="release gate result",
            ),
            "release gate result differs from candidate evidence",
        )
    except module.AuthorizationError as error:
        raise ValidationError(str(error)) from error


def validate_xcframework_archive(path: Path, version: str, checksum: str) -> None:
    require(digest(path) == checksum, f"SwiftPM checksum mismatch: {path.name}")
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        require(names and all(not name.startswith(("/", "../")) and "/../" not in name for name in names), f"unsafe zip inventory: {path.name}")
        require(all(info.date_time == (1980, 1, 1, 0, 0, 0) for info in archive.infolist()), f"non-canonical zip timestamp: {path.name}")
        plist_names = [name for name in names if name.endswith(".framework/Info.plist")]
        require(len(plist_names) == 3, f"XCFramework must contain three Apple slices: {path.name}")
        for name in plist_names:
            plist = plistlib.loads(archive.read(name))
            require(plist["CFBundleShortVersionString"] == version, f"XCFramework version mismatch: {name}")


def validate_wheels(paths: list[Path], config: dict[str, Any]) -> None:
    observed: set[tuple[str, str]] = set()
    for path in paths:
        name = path.name
        normalized = "retrievalkit-graph" if name.startswith("retrievalkit_graph-") else "retrievalkit" if name.startswith("retrievalkit-") else ""
        require(bool(normalized), f"unexpected Python wheel: {name}")
        tag = next((tag for tag in config["python"]["implementations"] if f"-{tag}-" in name), "")
        require(bool(tag), f"unexpected Python tag: {name}")
        require("macosx" in name and name.endswith("arm64.whl"), f"wheel is not macOS arm64: {name}")
        require(f"-{config['version']}-" in name, f"wheel version mismatch: {name}")
        observed.add((normalized, tag))
        with zipfile.ZipFile(path) as wheel:
            require(any(item.endswith(".dist-info/RECORD") for item in wheel.namelist()), f"wheel RECORD missing: {name}")
            sboms = [item for item in wheel.namelist() if ".dist-info/sboms/" in item]
            require(len(sboms) == 1, f"wheel SBOM inventory mismatch: {name}")
            sbom_bytes = wheel.read(sboms[0])
            require(b"path+file:///workspace/" in sbom_bytes, f"wheel SBOM lacks canonical source paths: {name}")
            require(b"path+file:///private/" not in sbom_bytes, f"wheel SBOM leaks checkout paths: {name}")
    expected = {(distribution, tag) for distribution in config["python"]["distributions"] for tag in config["python"]["implementations"]}
    require(observed == expected, f"Python wheel matrix mismatch: missing={sorted(expected - observed)}, extra={sorted(observed - expected)}")


def bundle_validation(repo: Path, bundle: Path) -> dict[str, Any]:
    static = static_validation(repo)
    config = load_json(repo / "release/release-v0.1.0.json")
    required = {
        *BUNDLE_LEGAL_FILES,
        "artifacts",
        "inventory.json",
        "release-manifest.json",
        "checksums.sha256",
        "sbom.spdx.json",
        "provenance.intoto.json",
    }
    require({path.name for path in bundle.iterdir()} == required, "release bundle root inventory mismatch")
    checksums: dict[str, str] = {}
    for line in (bundle / "checksums.sha256").read_text().splitlines():
        checksum, name = line.split("  ", 1)
        checksums[name] = checksum
    expected_payloads = sorted(path for path in bundle.rglob("*") if path.is_file() and path.name != "checksums.sha256")
    require(set(checksums) == {path.relative_to(bundle).as_posix() for path in expected_payloads}, "checksum inventory mismatch")
    for path in expected_payloads:
        require(digest(path) == checksums[path.relative_to(bundle).as_posix()], f"artifact checksum mismatch: {path.name}")
    inventory = load_json(bundle / "inventory.json")
    inventory_paths = sorted(
        [
            *bundle.glob("artifacts/*"),
            *(bundle / name for name in sorted(BUNDLE_LEGAL_FILES)),
            bundle / "sbom.spdx.json",
            bundle / "provenance.intoto.json",
        ]
    )
    require(set(inventory["files"]) == {path.relative_to(bundle).as_posix() for path in inventory_paths}, "closed artifact inventory mismatch")
    for path in inventory_paths:
        require(inventory["files"][path.relative_to(bundle).as_posix()] == digest(path), f"inventory hash mismatch: {path.name}")
    require(
        (bundle / "LICENSE").read_bytes() == (repo / "LICENSE").read_bytes(),
        "release bundle LICENSE differs from the repository license",
    )
    require(
        (bundle / "NOTICE").read_bytes() == (repo / "NOTICE").read_bytes(),
        "release bundle NOTICE differs from the repository notice",
    )
    manifest = load_json(bundle / "release-manifest.json")
    require(manifest["version"] == config["version"], "release manifest version mismatch")
    require(manifest["publication_ready"] is False, "candidate release bundle claims publication readiness")
    archives = list((bundle / "artifacts").glob("*.xcframework.zip"))
    require({path.name for path in archives} == set(config["apple"]["artifacts"]), "Apple artifact inventory mismatch")
    for path in archives:
        validate_xcframework_archive(path, config["version"], config["apple"]["artifacts"][path.name]["swiftpm_checksum"])
    validate_wheels(list((bundle / "artifacts").glob("*.whl")), config)
    sbom = load_json(bundle / "sbom.spdx.json")
    require(sbom["spdxVersion"] == "SPDX-2.3" and sbom["packages"], "SBOM is missing package inventory")
    provenance = load_json(bundle / "provenance.intoto.json")
    expected_subjects = {path.name: digest(path) for path in (bundle / "artifacts").iterdir()}
    observed_subjects = {row["name"]: row["digest"]["sha256"] for row in provenance["subject"]}
    require(observed_subjects == expected_subjects, "provenance subjects mismatch")
    return {"result": "PASS", "version": static["version"], "artifact_count": len(expected_subjects), "publication_ready": False, "publication_blockers": static["publication_blockers"]}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--bundle", type=Path)
    parser.add_argument("--publication", action="store_true")
    parser.add_argument("--authorization-record", type=Path)
    parser.add_argument("--candidate-evidence", type=Path)
    parser.add_argument("--scheduled-result", type=Path)
    parser.add_argument("--release-gate-result", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--source-revision")
    parser.add_argument("--candidate-run-id", type=int)
    parser.add_argument("--scheduled-run-id", type=int)
    parser.add_argument("--release-gate-run-id", type=int)
    parser.add_argument("--publication-run-id", type=int)
    parser.add_argument("--publication-run-attempt", type=int)
    args = parser.parse_args()
    repo = args.repo.resolve()
    try:
        result = bundle_validation(repo, args.bundle.resolve()) if args.bundle else {"result": "PASS", **static_validation(repo)}
        if args.publication:
            require(not result["publication_blockers"], "publication blocked: " + "; ".join(result["publication_blockers"]))
            require(args.authorization_record is not None, "publication authorization record is required")
            require(args.candidate_evidence is not None, "publication candidate evidence is required")
            require(args.scheduled_result is not None, "scheduled gate result is required")
            require(args.release_gate_result is not None, "release gate result is required")
            require(bool(args.repository), "publication repository identity is required")
            require(bool(args.source_revision), "publication source revision is required")
            require(args.candidate_run_id is not None, "candidate workflow run ID is required")
            require(args.scheduled_run_id is not None, "scheduled workflow run ID is required")
            require(args.release_gate_run_id is not None, "release gate workflow run ID is required")
            require(args.publication_run_id is not None, "publication workflow run ID is required")
            require(args.publication_run_attempt is not None, "publication workflow run attempt is required")
            validate_runtime_authorization(
                repo,
                args.bundle.resolve(),
                args.authorization_record.resolve(),
                args.candidate_evidence.resolve(),
                args.scheduled_result.resolve(),
                args.release_gate_result.resolve(),
                args.repository,
                args.source_revision,
                args.candidate_run_id,
                args.scheduled_run_id,
                args.release_gate_run_id,
                args.publication_run_id,
                args.publication_run_attempt,
            )
            result["publication_ready"] = True
    except (OSError, KeyError, TypeError, ValueError, ValidationError, zipfile.BadZipFile) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
