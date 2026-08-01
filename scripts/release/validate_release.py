#!/usr/bin/env python3
"""Validate RetrievalKit release metadata, artifacts, and publication authority."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import io
import json
import plistlib
import re
import struct
import sys
import tarfile
import zipfile
from email.parser import BytesParser
from pathlib import Path
from typing import Any
from xml.etree import ElementTree


ZERO_CHECKSUM = "0" * 64
ACTION_PATTERN = re.compile(r"uses:\s+[^\s@]+@([0-9a-f]{40})(?:\s|$)")
BUNDLE_LEGAL_FILES = {"LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
MAVEN_NAMESPACE = {"m": "http://maven.apache.org/POM/4.0.0"}
ONNX_MACOS_SIZE = 27_724_968
ONNX_MACOS_SHA256 = (
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"
)
ONNX_ANDROID_SIZE = 25_831_632
ONNX_ANDROID_SHA256 = (
    "4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad"
)
ONNX_LEGAL_IDENTITIES = {
    "LICENSE": (
        1_073,
        "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c",
    ),
    "ThirdPartyNotices.txt": (
        325_054,
        "0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2",
    ),
}
BROWSER_RUNTIME_IDENTITIES = {
    "dist/runtime/ort-wasm-simd-threaded.mjs": (
        24_180,
        "0a1e718d99c41b22c21f2520ff4f9e883a6b5533856e398d21816ee8eb8185d3",
    ),
    "dist/runtime/ort-wasm-simd-threaded.wasm": (
        13_479_978,
        "d1ab1b94b16a65b29d710d0b587b29e7bed336827577623913479b8afe8113e6",
    ),
    "dist/runtime/ort-wasm-simd-threaded.asyncify.mjs": (
        47_507,
        "7236653b8565da4046e459cd0e274123419a1d9f1f8f18fd36c28058346ca655",
    ),
    "dist/runtime/ort-wasm-simd-threaded.asyncify.wasm": (
        24_254_953,
        "7e83cd6cee77e478bc96a7e91b198144fb5e4126287daf1f9b54bb195ebcd55a",
    ),
}


class ValidationError(RuntimeError):
    """Release validation failed closed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def normalize_whitespace(value: str) -> str:
    return re.sub(r"\s+", " ", value).strip()


def normalize_python_specifier(value: str) -> tuple[str, ...]:
    return tuple(sorted(part.strip() for part in value.split(",") if part.strip()))


def validate_python_release_metadata(repo: Path, config: dict[str, Any]) -> None:
    expected = config["python"]["requires_python"]
    require(
        expected == ">=3.10,<3.15",
        "Python requires-python release range changed",
    )
    for relative in (
        Path("wrappers/python/pyproject.toml"),
        Path("wrappers/python-graph/pyproject.toml"),
        Path("wrappers/python-embedding/pyproject.toml"),
    ):
        text = (repo / relative).read_text(encoding="utf-8")
        project = re.search(
            r"^\[project\]\s*$\n(.*?)(?=^\[|\Z)",
            text,
            re.MULTILINE | re.DOTALL,
        )
        require(project is not None, f"Python project table missing: {relative}")
        match = re.search(
            r'^requires-python\s*=\s*"([^"]+)"\s*$',
            project.group(1),
            re.MULTILINE,
        )
        require(match is not None, f"Python requires-python missing: {relative}")
        require(
            match.group(1) == expected,
            (
                f"Python requires-python mismatch: {relative}: "
                f"expected {expected}, observed {match.group(1)}"
            ),
        )


def validate_persistence_release_contract(
    repo: Path,
    config: dict[str, Any],
) -> None:
    expected = {
        "base_write_format": 4,
        "base_readable_formats": [1, 2, 3, 4],
    }
    require(
        config["persistence"] == expected,
        "base persistence release contract changed",
    )
    core = (repo / "crates/retrievalkit-core/src/index.rs").read_text(encoding="utf-8")
    constant_names = (
        "LEGACY_FORMAT_VERSION",
        "TRANSACTIONAL_FORMAT_VERSION",
        "CHECKSUM_FORMAT_VERSION",
        "FORMAT_VERSION",
    )
    observed_formats = []
    for constant in constant_names:
        match = re.search(rf"const {constant}: u32 = (\d+);", core)
        require(
            match is not None,
            f"Rust base persistence constant missing: {constant}",
        )
        observed_formats.append(int(match.group(1)))
    require(
        observed_formats == expected["base_readable_formats"],
        "Rust base readable persistence formats differ from release contract",
    )
    require(
        observed_formats[-1] == expected["base_write_format"],
        "Rust base write persistence format differs from release contract",
    )

    claims = {
        Path("CHANGELOG.md"): (
            "Checksummed persistence format V4",
            (
                "V1, V2, and V3 indexes remain readable; their next save "
                "publishes a checksummed V4 snapshot"
            ),
        ),
        Path("docs/product/compatibility-policy.md"): (
            (
                "Persistence: V1–V4 base snapshots remain readable; new saves "
                "use the current checksummed V4 format"
            ),
            "Graph capability formats are validated independently",
        ),
        Path("docs/product/v0.1.0-migration.md"): (
            (
                "V1, V2, and V3 base indexes remain readable; their next save "
                "publishes a checksummed V4 snapshot"
            ),
            "Graph capability formats are versioned and validated independently",
        ),
        Path("docs/product/retrievalkit-product-spec.md"): (
            (
                "V1, V2, and V3 indexes remain readable; their next save "
                "publishes a checksummed V4 snapshot"
            ),
            "Format V3 and V4 require SHA-256 checksums",
            (
                "V4 adds the canonical record payload and stable "
                "external/internal chunk mapping"
            ),
        ),
        Path("wrappers/python/README.md"): (
            "New saves use a checksummed V4 manifest",
            (
                "V1, V2, and V3 indexes remain readable; their next save "
                "publishes a checksummed V4 snapshot"
            ),
        ),
        Path("wrappers/swift/RetrievalKit/README.md"): (
            "New saves use a checksummed V4 manifest",
            (
                "V1, V2, and V3 indexes remain readable; their next save "
                "publishes a checksummed V4 snapshot"
            ),
        ),
    }
    forbidden = (
        "New saves use a checksummed V3 manifest",
        "publishes a V3 generation",
        "writes format V3",
    )
    for relative, required_claims in claims.items():
        text = normalize_whitespace((repo / relative).read_text(encoding="utf-8"))
        require(
            all(claim in text for claim in required_claims),
            f"base persistence documentation mismatch: {relative}",
        )
        require(
            not any(claim in text for claim in forbidden),
            f"stale base persistence documentation: {relative}",
        )


def validate_active_release_claims(repo: Path, config: dict[str, Any]) -> None:
    spec_path = repo / "docs/product/retrievalkit-product-spec.md"
    spec_text = spec_path.read_text(encoding="utf-8")
    section = re.search(
        r"TypeScript and Kotlin follow the same aggregate boundary\.(.*?)"
        r"The first optional graph release",
        spec_text,
        re.DOTALL,
    )
    require(section is not None, "active TypeScript and Kotlin product section missing")
    spec = normalize_whitespace(section.group(1))
    expected_claims = (
        f'`{config["node"]["packages"]["base"]["name"]}`',
        f'`{config["node"]["packages"]["graph"]["name"]}`',
        f'`{config["node"]["engines"]}`',
        f'`{config["kotlin"]["group"]}`',
        *(f"`{target}`" for target in config["kotlin"]["targets"]),
        (
            f"These npm names and Maven coordinates are fixed for "
            f"`{config['version']}`, but the SDK packages remain unpublished "
            "until the release gates pass"
        ),
    )
    require(
        all(claim in spec for claim in expected_claims),
        "active product spec lacks fixed Node or Maven release identities",
    )
    complete_spec = normalize_whitespace(spec_text)
    embedding_claims = (
        f'`{config["python"]["distributions"][2]}`',
        f'`{config["node"]["packages"]["embedding"]["name"]}`',
        f'`{config["browser_embedding"]["package"]["name"]}`',
        "`retrievalkit-embedding-android`",
        "Rust embedding crates remain source-only",
    )
    require(
        all(claim in complete_spec for claim in embedding_claims),
        "active product spec lacks fixed embedding release identities",
    )
    require(
        config["browser_retrieval"]["package"]["name"] in complete_spec
        and "portable and SIMD128" in complete_spec,
        "active product spec lacks browser retrieval release identity or tiers",
    )
    require(
        "retrievalkit-node-local" not in spec
        and "retrievalkit-node-graph-local" not in spec
        and "npm names and Maven coordinates remain provisional" not in spec,
        "active product spec contains obsolete release identities",
    )
    blocker_text = " ".join(config["publication_blockers"])
    require(
        "bootstrapped and trusted publisher configured" not in blocker_text
        and "npm trusted publishing configured" not in blocker_text
        and "Maven Central namespace verification" not in blocker_text,
        "release config lists completed registry setup as a publication blocker",
    )
    require(
        config.get("release_freeze")
        == {
            "status": "frozen",
            "frozen_on": "2026-08-01",
            "revision_binding": "commit-containing-this-record",
            "post_freeze_change_policy": "new-freeze-commit-required",
        },
        "release config does not contain the exact v0.1.0 freeze policy",
    )
    require(
        all(
            claim in blocker_text
            for claim in (
                "public docs and source preview",
                "fresh complete release candidate",
                "wrapper onboarding qualification",
                "Phase 7 scheduled and release gates",
                "signed v0.1.0 tag and owner release approval",
            )
        ),
        "release config omits an outstanding publication evidence gate",
    )

    report = normalize_whitespace(
        (
            repo
            / "docs/product/reports/cross-language-wrapper-parity-audit.md"
        ).read_text(encoding="utf-8")
    )
    require(
        all(
            claim in report
            for claim in (
                "Historical evidence:",
                "fccb3a9",
                "../../../release/release-v0.1.0.json",
                "../release-process.md",
                "../compatibility-policy.md",
            )
        ),
        "historical parity audit lacks its revision or current-guidance links",
    )


def static_validation(repo: Path) -> dict[str, Any]:
    config = load_json(repo / "release/release-v0.1.0.json")
    version = (repo / "VERSION").read_text().strip()
    semver = r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)"
    require(re.fullmatch(semver, version) is not None, "VERSION is not semantic")
    require(version == "0.1.0" == config["version"], "release version mismatch")
    require(
        config["rust"] == {"publication": "source-only", "crates_io": False},
        "Rust release must remain source-only",
    )
    require(f"## {version} - Unreleased preview" in (repo / "CHANGELOG.md").read_text(), "changelog version mismatch")
    require((repo / f"docs/product/v{version}-migration.md").is_file(), "release migration guide missing")
    cargo = (repo / "Cargo.toml").read_text()
    require(f'version = "{version}"' in cargo, "Cargo workspace version mismatch")
    require(
        f'repository = "{config["repository"]}"' in cargo,
        "Cargo workspace repository differs from release config",
    )
    for manifest in sorted((repo / "crates").glob("*/Cargo.toml")):
        require("version.workspace = true" in manifest.read_text(), f"crate does not inherit workspace version: {manifest}")
    for pyproject in (
        repo / "wrappers/python/pyproject.toml",
        repo / "wrappers/python-graph/pyproject.toml",
        repo / "wrappers/python-embedding/pyproject.toml",
    ):
        require(f'version = "{version}"' in pyproject.read_text(), f"Python version mismatch: {pyproject}")
    validate_python_release_metadata(repo, config)
    validate_persistence_release_contract(repo, config)
    validate_active_release_claims(repo, config)
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
    require(
        config["python"]["distributions"]
        == ["retrievalkit", "retrievalkit-graph", "retrievalkit-embedding"],
        "Python distribution set changed",
    )
    require(
        config["node"]["packages"]
        == {
            "base": {
                "name": "@gungorbasa/retrievalkit",
                "artifact": "gungorbasa-retrievalkit-0.1.0.tgz",
            },
            "graph": {
                "name": "@gungorbasa/retrievalkit-graph",
                "artifact": "gungorbasa-retrievalkit-graph-0.1.0.tgz",
            },
            "embedding": {
                "name": "@gungorbasa/retrievalkit-embedding",
                "artifact": "gungorbasa-retrievalkit-embedding-0.1.0.tgz",
            },
        },
        "Node release identities changed",
    )
    for capability, expected_name in (
        ("base", "@gungorbasa/retrievalkit"),
        ("graph", "@gungorbasa/retrievalkit-graph"),
        ("embedding", "@gungorbasa/retrievalkit-embedding"),
    ):
        package = load_json(repo / f"wrappers/typescript/{capability}/package.json")
        require(package["name"] == expected_name, f"Node {capability} package identity mismatch")
        require(package.get("private") is True, f"Node {capability} source package must remain private")
        require(package["version"] == version, f"Node {capability} version mismatch")
    browser_retrieval = load_json(repo / "wrappers/browser/package.json")
    require(
        config["browser_retrieval"]
        == {
            "engines": "^22.13.0 || ^24.0.0",
            "runtime": "dedicated-worker-wasm",
            "wasm_tiers": ["portable", "simd128"],
            "package": {
                "name": "@gungorbasa/retrievalkit-browser",
                "artifact": "gungorbasa-retrievalkit-browser-0.1.0.tgz",
            },
        },
        "browser retrieval release identity changed",
    )
    require(
        browser_retrieval["name"]
        == config["browser_retrieval"]["package"]["name"],
        "browser retrieval package identity mismatch",
    )
    require(
        browser_retrieval.get("private") is True
        and browser_retrieval["version"] == version,
        "browser retrieval source package must remain private at the release version",
    )
    browser_embedding = load_json(repo / "wrappers/browser-embedding/package.json")
    require(
        config["browser_embedding"]
        == {
            "engines": "^22.13.0 || ^24.0.0",
            "package": {
                "name": "@gungorbasa/retrievalkit-browser-embedding",
                "artifact": "gungorbasa-retrievalkit-browser-embedding-0.1.0.tgz",
            },
        },
        "browser embedding release identity changed",
    )
    require(
        browser_embedding["name"]
        == config["browser_embedding"]["package"]["name"],
        "browser embedding package identity mismatch",
    )
    require(
        browser_embedding.get("private") is True
        and browser_embedding["version"] == version,
        "browser embedding source package must remain private at the release version",
    )
    require(
        config["kotlin"]["group"] == "io.github.gungorbasa",
        "Kotlin Maven group changed",
    )
    require(
        config["kotlin"]["artifacts"]
        == {
            "retrievalkit": "jar",
            "retrievalkit-graph": "jar",
            "retrievalkit-android": "aar",
            "retrievalkit-graph-android": "aar",
            "retrievalkit-embedding": "jar",
            "retrievalkit-embedding-android": "aar",
        },
        "Kotlin artifact identities changed",
    )
    require(
        config["kotlin"]["android_preview"]
        == {
            "status": "preview",
            "min_sdk": 24,
            "abi": "arm64-v8a",
            "retained_non_device_checks": [
                "build",
                "packaging",
                "closed-inventory",
                "abi-architecture",
                "jvm-jni-contract",
                "fresh-consumer-compilation-install-resolution",
            ],
            "live_device_inference_qualified": False,
            "live_device_inference_publication_blocker": False,
            "claim_policy": (
                "no production, performance, or device-compatibility claims "
                "beyond existing evidence"
            ),
        },
        "Android preview release contract changed",
    )
    require(
        not any(
            "android" in blocker.lower()
            and (
                "device inference" in blocker.lower()
                or "physical device" in blocker.lower()
            )
            for blocker in config["publication_blockers"]
        ),
        "live Android device qualification must not be a publication blocker",
    )
    for relative in (
        "README.md",
        "docs/guides/kotlin.md",
        "docs/product/compatibility-policy.md",
        "docs/product/release-process.md",
        "docs/product/retrievalkit-product-spec.md",
        "wrappers/kotlin/README.md",
        "wrappers/kotlin/android-embedding/README.md",
    ):
        android_contract = normalize_whitespace(
            (repo / relative).read_text(encoding="utf-8")
        ).lower()
        require(
            "android" in android_contract
            and "preview" in android_contract
            and "unqualified" in android_contract,
            f"Android preview qualification boundary missing: {relative}",
        )
    signing = config["kotlin"]["signing"]
    require(
        signing["fingerprint"] == "0E82F1A5487A4EF3CCF1ED6C393266CD4DD158ED",
        "Maven signing fingerprint changed",
    )
    signing_key = repo / signing["public_key"]
    require(signing_key.is_file(), "Maven public signing key is missing")
    require(
        digest(signing_key) == signing["sha256"],
        "Maven public signing key checksum mismatch",
    )
    require(
        signing_key.read_text(encoding="utf-8").startswith(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----"
        ),
        "Maven public signing key is not ASCII-armored PGP",
    )
    require(
        'orElse("io.github.gungorbasa")'
        in (repo / "wrappers/kotlin/build.gradle.kts").read_text(encoding="utf-8"),
        "Kotlin checked-in Maven group mismatch",
    )
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
        repo / "docs/product/retrievalkit-product-spec.md",
        repo / "docs/product/v0.1.0-migration.md",
        repo / "docs/product/reports/cross-language-wrapper-parity-audit.md",
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
    require(
        "assemble_node_packages.py" in candidate
        and "--base-name @gungorbasa/retrievalkit" in candidate
        and "--graph-name @gungorbasa/retrievalkit-graph" in candidate
        and "--embedding-name @gungorbasa/retrievalkit-embedding" in candidate,
        "candidate workflow lacks the approved Node release identities",
    )
    require(
        "assemble_browser_package.py" in candidate
        and "--name @gungorbasa/retrievalkit-browser" in candidate,
        "candidate workflow lacks the approved browser retrieval identity",
    )
    require(
        "assemble_browser_embedding_package.py" in candidate
        and "--name @gungorbasa/retrievalkit-browser-embedding" in candidate,
        "candidate workflow lacks the approved browser embedding identity",
    )
    require(
        "assemble_kotlin_packages.py" in candidate
        and "--group io.github.gungorbasa" in candidate,
        "candidate workflow lacks the approved Kotlin release identity",
    )
    require(
        '"$android_home/cmdline-tools/latest/bin/sdkmanager"' in candidate,
        "candidate workflow does not invoke sdkmanager through ANDROID_HOME",
    )
    require(
        candidate.count("retention-days: 90") == 7
        and "retention-days: 180" not in candidate,
        "candidate workflow does not use the public-repository 90-day retention maximum",
    )
    publication = (repo / ".github/workflows/publish-release.yml").read_text()
    require("environment: release" in publication and "environment: pypi" in publication, "publication jobs lack protected environments")
    require(
        "git verify-tag" in publication
        and "release/retrievalkit-release-signing-key.asc" in publication
        and "0E82F1A5487A4EF3CCF1ED6C393266CD4DD158ED" in publication
        and 'GNUPGHOME="$verification_home" gpg --batch --import "$RELEASE_SIGNING_KEY"'
        in publication
        and 'GNUPGHOME="$verification_home" git verify-tag' in publication
        and "publication_authorization.py candidate" in publication
        and "publication_authorization.py authorize" in publication
        and "--authorization-record" in publication,
        "publication workflow bypasses clean-keyring signed-tag, candidate, or runtime authority validation",
    )
    require(
        publication.count("retention-days: 90") == 4
        and "retention-days: 180" not in publication
        and "X-GitHub-Api-Version: 2026-03-10" in publication
        and "immutable-releases" in publication
        and "jq --exit-status '.enabled == true'" in publication
        and "GH_TOKEN: ${{ secrets.RELEASE_GITHUB_TOKEN }}" in publication,
        "publication workflow lacks 90-day retention, immutable-release enforcement, or the protected release credential",
    )
    require(
        "All five approved npm packages" in publication
        and "@gungorbasa%2fretrievalkit-browser" in publication
        and "gungorbasa-retrievalkit-browser-$VERSION.tgz" in publication,
        "publication workflow lacks the approved browser retrieval package",
    )
    release_workflows = candidate + publication.lower()
    forbidden_device_commands = (
        "xcrun devicectl",
        "ios-deploy",
        "xcodebuild test-without-building",
        "adb ",
        "connectedandroidtest",
        "connectedcheck",
    )
    require(
        not any(command in release_workflows for command in forbidden_device_commands),
        "distribution workflow contains a physical-device command",
    )


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
        repo / "wrappers/python-embedding/pyproject.toml",
    )
    for pyproject in pyprojects:
        if 'license = { text = "Apache-2.0" }' not in pyproject.read_text(encoding="utf-8"):
            relative_path = pyproject.relative_to(repo)
            blockers.append(
                f"Python metadata is not reconciled with Apache-2.0: {relative_path}"
            )
    if not (repo / "THIRD_PARTY_NOTICES.md").is_file():
        blockers.append("third-party notices are absent")
    for wrapper in (
        "wrappers/python",
        "wrappers/python-graph",
        "wrappers/python-embedding",
    ):
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
        normalized = (
            "retrievalkit-embedding"
            if name.startswith("retrievalkit_embedding-")
            else "retrievalkit-graph"
            if name.startswith("retrievalkit_graph-")
            else "retrievalkit"
            if name.startswith("retrievalkit-")
            else ""
        )
        require(bool(normalized), f"unexpected Python wheel: {name}")
        tag = next((tag for tag in config["python"]["implementations"] if f"-{tag}-" in name), "")
        require(bool(tag), f"unexpected Python tag: {name}")
        require("macosx" in name and name.endswith("arm64.whl"), f"wheel is not macOS arm64: {name}")
        require(f"-{config['version']}-" in name, f"wheel version mismatch: {name}")
        observed.add((normalized, tag))
        with zipfile.ZipFile(path) as wheel:
            require(any(item.endswith(".dist-info/RECORD") for item in wheel.namelist()), f"wheel RECORD missing: {name}")
            validate_wheel_requires_python(
                wheel,
                name,
                config["python"]["requires_python"],
            )
            sboms = [item for item in wheel.namelist() if ".dist-info/sboms/" in item]
            require(len(sboms) == 1, f"wheel SBOM inventory mismatch: {name}")
            sbom_bytes = wheel.read(sboms[0])
            require(b"path+file:///workspace/" in sbom_bytes, f"wheel SBOM lacks canonical source paths: {name}")
            require(b"path+file:///private/" not in sbom_bytes, f"wheel SBOM leaks checkout paths: {name}")
            if normalized == "retrievalkit-embedding":
                runtime_names = [
                    item
                    for item in wheel.namelist()
                    if item.endswith(
                        "retrievalkit_embedding/runtime/libonnxruntime.1.24.3.dylib"
                    )
                ]
                require(
                    len(runtime_names) == 1,
                    f"embedding wheel runtime inventory mismatch: {name}",
                )
                runtime = wheel.read(runtime_names[0])
                validate_macho_arm64(runtime, f"{name} ONNX Runtime")
                require(
                    len(runtime) == ONNX_MACOS_SIZE
                    and hashlib.sha256(runtime).hexdigest() == ONNX_MACOS_SHA256,
                    f"embedding wheel ONNX Runtime identity mismatch: {name}",
                )
                for legal_name in ONNX_LEGAL_IDENTITIES:
                    legal = [
                        item
                        for item in wheel.namelist()
                        if item.endswith(
                            f"retrievalkit_embedding/runtime/{legal_name}"
                        )
                    ]
                    require(
                        len(legal) == 1,
                        f"embedding wheel legal inventory mismatch: {name}",
                    )
                    validate_onnx_legal(
                        wheel.read(legal[0]), legal_name, f"{name}/{legal_name}"
                    )
    expected = {(distribution, tag) for distribution in config["python"]["distributions"] for tag in config["python"]["implementations"]}
    require(observed == expected, f"Python wheel matrix mismatch: missing={sorted(expected - observed)}, extra={sorted(observed - expected)}")


def validate_wheel_requires_python(
    wheel: zipfile.ZipFile,
    name: str,
    expected: str,
) -> None:
    metadata_names = [
        item for item in wheel.namelist() if item.endswith(".dist-info/METADATA")
    ]
    require(
        len(metadata_names) == 1,
        f"wheel METADATA inventory mismatch: {name}",
    )
    metadata = BytesParser().parsebytes(wheel.read(metadata_names[0]))
    observed = metadata.get_all("Requires-Python", [])
    require(
        len(observed) == 1
        and normalize_python_specifier(observed[0])
        == normalize_python_specifier(expected),
        (
            f"wheel Requires-Python mismatch: {name}: "
            f"expected {expected}, observed {observed}"
        ),
    )


def validate_checksum_manifest(
    path: Path,
    files: dict[str, Path],
    algorithm: str,
) -> None:
    observed: dict[str, str] = {}
    for line in path.read_text(encoding="ascii").splitlines():
        checksum, name = line.split("  ", 1)
        observed[name] = checksum
    require(set(observed) == set(files), f"{path.name} inventory mismatch")
    for name, file_path in files.items():
        checksum = hashlib.new(algorithm, file_path.read_bytes()).hexdigest()
        require(observed[name] == checksum, f"{path.name} checksum mismatch: {name}")


def validate_macho_arm64(data: bytes, label: str) -> None:
    require(len(data) >= 8, f"truncated Mach-O binary: {label}")
    magic, cpu_type = struct.unpack("<II", data[:8])
    require(
        magic == 0xFEEDFACF and cpu_type == 0x0100000C,
        f"native binary is not macOS arm64: {label}",
    )


def validate_elf_arm64(data: bytes, label: str) -> None:
    require(len(data) >= 20 and data[:4] == b"\x7fELF", f"native binary is not ELF: {label}")
    byte_order = "<" if data[5] == 1 else ">" if data[5] == 2 else ""
    require(bool(byte_order), f"native binary has invalid ELF byte order: {label}")
    require(
        struct.unpack(f"{byte_order}H", data[18:20])[0] == 183,
        f"native binary is not Android arm64: {label}",
    )


def validate_onnx_legal(data: bytes, source_name: str, label: str) -> None:
    size, sha256 = ONNX_LEGAL_IDENTITIES[source_name]
    require(
        len(data) == size and hashlib.sha256(data).hexdigest() == sha256,
        f"ONNX Runtime legal identity mismatch: {label}",
    )


def validate_node_packages(root: Path, config: dict[str, Any]) -> None:
    expected_names = {
        capability: row["artifact"]
        for capability, row in config["node"]["packages"].items()
    }
    tarballs = {path.name: path for path in root.glob("*.tgz")}
    require(set(tarballs) == set(expected_names.values()), "Node tarball inventory mismatch")
    inventory = load_json(root / "inventory.json")
    require(
        inventory["kind"] == "retrievalkit-node-release"
        and inventory["artifactReady"] is True
        and inventory["publicationReady"] is False,
        "Node package inventory readiness mismatch",
    )
    inventory_rows = {row["capability"]: row for row in inventory["artifacts"]}
    require(set(inventory_rows) == set(expected_names), "Node capability inventory mismatch")
    for capability, artifact_name in expected_names.items():
        path = tarballs[artifact_name]
        row = inventory_rows[capability]
        expected_identity = config["node"]["packages"][capability]["name"]
        require(row["npmName"] == expected_identity, f"Node {capability} identity mismatch")
        require(row["file"] == artifact_name, f"Node {capability} filename mismatch")
        require(row["version"] == config["version"], f"Node {capability} version mismatch")
        require(
            row["platform"] == config["node"]["platform"],
            f"Node {capability} target mismatch",
        )
        require(row["sha256"] == digest(path), f"Node {capability} inventory hash mismatch")
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            require(
                all(
                    member.name.startswith("package/")
                    and ".." not in Path(member.name).parts
                    and not member.issym()
                    and not member.islnk()
                    for member in members
                ),
                f"unsafe Node archive inventory: {artifact_name}",
            )
            names = {
                member.name.removeprefix("package/")
                for member in members
                if member.isfile()
            }
            native_name = (
                "native/retrievalkit-embedding.node"
                if capability == "embedding"
                else "native/retrievalkit.node"
            )
            required = {
                "LICENSE",
                "NOTICE",
                "README.md",
                "dist/index.js",
                "dist/index.d.ts",
                native_name,
                "package.json",
            }
            if capability == "embedding":
                required |= {
                    "runtime/libonnxruntime.1.24.3.dylib",
                    "runtime/ONNX-Runtime-LICENSE",
                    "runtime/ONNX-Runtime-ThirdPartyNotices.txt",
                }
            require(
                required.issubset(names),
                f"Node package contents incomplete: {artifact_name}",
            )
            package_file = archive.extractfile("package/package.json")
            native_file = archive.extractfile(f"package/{native_name}")
            require(package_file is not None and native_file is not None, f"Node package payload unreadable: {artifact_name}")
            package = json.load(package_file)
            native = native_file.read()
            runtime = None
            runtime_legal: dict[str, bytes] = {}
            if capability == "embedding":
                runtime_file = archive.extractfile(
                    "package/runtime/libonnxruntime.1.24.3.dylib"
                )
                require(runtime_file is not None, "Node embedding runtime is unreadable")
                runtime = runtime_file.read()
                for packaged_name, source_name in (
                    ("ONNX-Runtime-LICENSE", "LICENSE"),
                    ("ONNX-Runtime-ThirdPartyNotices.txt", "ThirdPartyNotices.txt"),
                ):
                    legal_file = archive.extractfile(
                        f"package/runtime/{packaged_name}"
                    )
                    require(
                        legal_file is not None,
                        f"Node embedding legal file is unreadable: {packaged_name}",
                    )
                    runtime_legal[source_name] = legal_file.read()
        require(set(row["files"]) == names, f"Node {capability} file inventory mismatch")
        require(
            row["sha512"] == hashlib.sha512(path.read_bytes()).hexdigest(),
            f"Node {capability} SHA-512 inventory mismatch",
        )
        require(package["name"] == expected_identity, f"Node package.json identity mismatch: {artifact_name}")
        require(package["version"] == config["version"], f"Node package.json version mismatch: {artifact_name}")
        require("private" not in package, f"Node staged package remains private: {artifact_name}")
        require(package["license"] == "Apache-2.0", f"Node license mismatch: {artifact_name}")
        require(
            package["os"] == ["darwin"] and package["cpu"] == ["arm64"],
            f"Node package target mismatch: {artifact_name}",
        )
        validate_macho_arm64(native, artifact_name)
        if capability == "embedding":
            assert runtime is not None
            validate_macho_arm64(runtime, f"{artifact_name} ONNX Runtime")
            require(
                len(runtime) == ONNX_MACOS_SIZE
                and hashlib.sha256(runtime).hexdigest() == ONNX_MACOS_SHA256,
                "Node embedding ONNX Runtime identity mismatch",
            )
            for source_name, data in runtime_legal.items():
                validate_onnx_legal(
                    data, source_name, f"{artifact_name}/{source_name}"
                )
        if capability == "base":
            require(
                not any(
                    marker in native
                    for marker in (
                        b"retrievalkit_graph",
                        b"NativeGraphHandle",
                        b"GraphRetrievalDatabase",
                    )
                ),
                "Node base native addon contains graph symbols",
            )
    package_files = {path.name: path for path in tarballs.values()}
    validate_checksum_manifest(root / "SHA256SUMS", package_files, "sha256")
    validate_checksum_manifest(root / "SHA512SUMS", package_files, "sha512")


def validate_browser_retrieval_package(root: Path, config: dict[str, Any]) -> None:
    package_config = config["browser_retrieval"]["package"]
    tarballs = list(root.glob("*.tgz"))
    require(
        [path.name for path in tarballs] == [package_config["artifact"]],
        "browser retrieval tarball inventory mismatch",
    )
    tarball = tarballs[0]
    inventory = load_json(root / "inventory.json")
    require(
        inventory["kind"] == "retrievalkit-browser-release"
        and inventory["artifactReady"] is True
        and inventory["publicationReady"] is False,
        "browser retrieval inventory readiness mismatch",
    )
    require(len(inventory["artifacts"]) == 1, "browser retrieval inventory mismatch")
    row = inventory["artifacts"][0]
    require(
        row["capability"] == "browser-retrieval-graph"
        and row["npmName"] == package_config["name"]
        and row["file"] == package_config["artifact"]
        and row["version"] == config["version"]
        and row["runtime"] == "browser-worker-wasm"
        and row["wasmTiers"] == config["browser_retrieval"]["wasm_tiers"]
        and row["sha256"] == digest(tarball),
        "browser retrieval artifact identity mismatch",
    )
    with tarfile.open(tarball, "r:gz") as archive:
        members = archive.getmembers()
        require(
            all(
                member.name.startswith("package/")
                and ".." not in Path(member.name).parts
                and not member.issym()
                and not member.islnk()
                for member in members
            ),
            "unsafe browser retrieval archive inventory",
        )
        names = {
            member.name.removeprefix("package/")
            for member in members
            if member.isfile()
        }
        expected = {
            "LICENSE",
            "NOTICE",
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "package.json",
            *(
                f"dist/{module}{suffix}"
                for module in (
                    "adapter",
                    "databases",
                    "errors",
                    "generated-adapter",
                    "index",
                    "protocol",
                    "rpc-client",
                    "types",
                    "worker",
                )
                for suffix in (".js", ".js.map", ".d.ts", ".d.ts.map")
            ),
        }
        for tier in config["browser_retrieval"]["wasm_tiers"]:
            expected |= {
                f"dist/wasm/{tier}/retrievalkit_wasm.js",
                f"dist/wasm/{tier}/retrievalkit_wasm.d.ts",
                f"dist/wasm/{tier}/retrievalkit_wasm_bg.wasm",
                f"dist/wasm/{tier}/retrievalkit_wasm_bg.wasm.d.ts",
            }
        require(names == expected, "browser retrieval package contents are not closed")
        require(not any(name.endswith(".node") for name in names), "browser retrieval package contains native Node code")
        package_file = archive.extractfile("package/package.json")
        require(package_file is not None, "browser retrieval package metadata is unreadable")
        package = json.load(package_file)
        wasm_payloads: dict[str, bytes] = {}
        for tier in config["browser_retrieval"]["wasm_tiers"]:
            wasm_file = archive.extractfile(
                f"package/dist/wasm/{tier}/retrievalkit_wasm_bg.wasm"
            )
            require(wasm_file is not None, f"browser retrieval {tier} WASM is unreadable")
            wasm_payloads[tier] = wasm_file.read()
    require(set(row["files"]) == names, "browser retrieval file inventory mismatch")
    require(
        row["sha512"] == hashlib.sha512(tarball.read_bytes()).hexdigest(),
        "browser retrieval SHA-512 inventory mismatch",
    )
    require(
        package["name"] == package_config["name"]
        and package["version"] == config["version"]
        and "private" not in package
        and package["license"] == "Apache-2.0",
        "browser retrieval package metadata mismatch",
    )
    require(
        package.get("publishConfig")
        == {"access": "public", "registry": "https://registry.npmjs.org/"},
        "browser retrieval package publication metadata mismatch",
    )
    for tier, wasm in wasm_payloads.items():
        require(
            wasm.startswith(b"\x00asm\x01\x00\x00\x00"),
            f"browser retrieval {tier} WASM header mismatch",
        )
        require(
            package["exports"].get(f"./wasm/{tier}")
            == {
                "types": f"./dist/wasm/{tier}/retrievalkit_wasm.d.ts",
                "import": f"./dist/wasm/{tier}/retrievalkit_wasm.js",
            },
            f"browser retrieval {tier} export mismatch",
        )
    require(
        wasm_payloads["portable"] != wasm_payloads["simd128"],
        "browser retrieval WASM tiers are identical",
    )
    package_files = {tarball.name: tarball}
    validate_checksum_manifest(root / "SHA256SUMS", package_files, "sha256")
    validate_checksum_manifest(root / "SHA512SUMS", package_files, "sha512")


def validate_browser_embedding_package(root: Path, config: dict[str, Any]) -> None:
    package_config = config["browser_embedding"]["package"]
    tarballs = list(root.glob("*.tgz"))
    require(
        [path.name for path in tarballs] == [package_config["artifact"]],
        "browser embedding tarball inventory mismatch",
    )
    tarball = tarballs[0]
    inventory = load_json(root / "inventory.json")
    require(
        inventory["kind"] == "retrievalkit-browser-embedding-release"
        and inventory["artifactReady"] is True
        and inventory["publicationReady"] is False,
        "browser embedding inventory readiness mismatch",
    )
    require(len(inventory["artifacts"]) == 1, "browser embedding inventory mismatch")
    row = inventory["artifacts"][0]
    require(
        row["capability"] == "browser-embedding"
        and row["npmName"] == package_config["name"]
        and row["file"] == package_config["artifact"]
        and row["version"] == config["version"]
        and row["runtime"] == "browser-worker"
        and row["sha256"] == digest(tarball),
        "browser embedding artifact identity mismatch",
    )
    with tarfile.open(tarball, "r:gz") as archive:
        members = archive.getmembers()
        require(
            all(
                member.name.startswith("package/")
                and ".." not in Path(member.name).parts
                and not member.issym()
                and not member.islnk()
                for member in members
            ),
            "unsafe browser embedding archive inventory",
        )
        names = {
            member.name.removeprefix("package/")
            for member in members
            if member.isfile()
        }
        required = {
            "LICENSE",
            "NOTICE",
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "package.json",
            "dist/index.js",
            "dist/index.d.ts",
            "dist/worker.js",
            "dist/worker.d.ts",
            "dist/runtime/ONNXRUNTIME-LICENSE",
            "dist/runtime/ONNXRUNTIME-ThirdPartyNotices.txt",
            "dist/runtime/HUGGINGFACE-TOKENIZERS-LICENSE",
            *BROWSER_RUNTIME_IDENTITIES,
        }
        require(required.issubset(names), "browser embedding package contents incomplete")
        require(
            not any(name.endswith(".node") for name in names),
            "browser embedding package contains native Node code",
        )
        package_file = archive.extractfile("package/package.json")
        require(package_file is not None, "browser embedding package.json is unreadable")
        package = json.load(package_file)
        for name, (size, sha256) in BROWSER_RUNTIME_IDENTITIES.items():
            runtime_file = archive.extractfile(f"package/{name}")
            require(runtime_file is not None, f"browser runtime is unreadable: {name}")
            data = runtime_file.read()
            require(
                len(data) == size and hashlib.sha256(data).hexdigest() == sha256,
                f"browser runtime identity mismatch: {name}",
            )
    require(set(row["files"]) == names, "browser embedding file inventory mismatch")
    require(
        row["sha512"] == hashlib.sha512(tarball.read_bytes()).hexdigest(),
        "browser embedding SHA-512 inventory mismatch",
    )
    require(
        package["name"] == package_config["name"]
        and package["version"] == config["version"]
        and "private" not in package
        and package["license"] == "Apache-2.0",
        "browser embedding package metadata mismatch",
    )
    validate_checksum_manifest(
        root / "SHA256SUMS", {tarball.name: tarball}, "sha256"
    )
    validate_checksum_manifest(
        root / "SHA512SUMS", {tarball.name: tarball}, "sha512"
    )


def validate_maven_pom(
    path: Path,
    *,
    group: str,
    artifact_id: str,
    version: str,
    packaging: str,
) -> None:
    root = ElementTree.parse(path).getroot()

    def text(name: str) -> str:
        return root.findtext(f"m:{name}", default="", namespaces=MAVEN_NAMESPACE)

    require(
        (text("groupId"), text("artifactId"), text("version"))
        == (group, artifact_id, version),
        f"Maven POM identity mismatch: {path.name}",
    )
    if packaging == "aar":
        require(text("packaging") == "aar", f"Maven POM packaging mismatch: {path.name}")
        require(
            text("description").startswith("Preview "),
            f"Android Maven POM must declare preview status: {path.name}",
        )
    for name in ("name", "description", "url", "licenses", "developers", "scm"):
        require(root.find(f"m:{name}", MAVEN_NAMESPACE) is not None, f"Maven POM lacks {name}: {path.name}")
    license_name = root.findtext(
        "m:licenses/m:license/m:name",
        default="",
        namespaces=MAVEN_NAMESPACE,
    )
    require("Apache" in license_name, f"Maven POM license mismatch: {path.name}")
    require(root.find("m:repositories", MAVEN_NAMESPACE) is None, f"Maven POM embeds repositories: {path.name}")


def validate_kotlin_primary(path: Path, capability: str, packaging: str) -> None:
    with zipfile.ZipFile(path) as archive:
        names = set(archive.namelist())
        if packaging == "jar":
            require({"LICENSE", "NOTICE"}.issubset(names), f"Kotlin legal files missing: {path.name}")
            classes = names
            legal_payloads = {
                name: archive.read(name)
                for name in (
                    "ONNX-Runtime-LICENSE",
                    "ONNX-Runtime-ThirdPartyNotices.txt",
                )
                if name in names
            }
            base_native = "native/macos-aarch64/libretrievalkit_jni.dylib"
            graph_native = "native/macos-aarch64/libretrievalkit_jni_graph.dylib"
            native_reader = archive.read
        else:
            classes_bytes = archive.read("classes.jar")
            with zipfile.ZipFile(io.BytesIO(classes_bytes)) as classes_archive:
                classes = set(classes_archive.namelist())
                legal_payloads = {
                    name: classes_archive.read(name)
                    for name in (
                        "ONNX-Runtime-LICENSE",
                        "ONNX-Runtime-ThirdPartyNotices.txt",
                    )
                    if name in classes
                }
            require({"LICENSE", "NOTICE"}.issubset(classes), f"Android legal files missing: {path.name}")
            base_native = "jni/arm64-v8a/libretrievalkit_jni.so"
            graph_native = "jni/arm64-v8a/libretrievalkit_jni_graph.so"
            native_reader = archive.read
        if capability.endswith("embedding"):
            require(
                {
                    "ONNX-Runtime-LICENSE",
                    "ONNX-Runtime-ThirdPartyNotices.txt",
                    "runtime-identity.txt",
                }.issubset(classes),
                f"Kotlin embedding legal/runtime metadata missing: {path.name}",
            )
            validate_onnx_legal(
                legal_payloads["ONNX-Runtime-LICENSE"],
                "LICENSE",
                f"{path.name}/ONNX-Runtime-LICENSE",
            )
            validate_onnx_legal(
                legal_payloads["ONNX-Runtime-ThirdPartyNotices.txt"],
                "ThirdPartyNotices.txt",
                f"{path.name}/ONNX-Runtime-ThirdPartyNotices.txt",
            )
            if packaging == "jar":
                embedding_native = (
                    "native/macos-aarch64/libretrievalkit_embedding_jni.dylib"
                )
                runtime_name = "native/macos-aarch64/libonnxruntime.1.24.3.dylib"
            else:
                embedding_native = "jni/arm64-v8a/libretrievalkit_embedding_jni.so"
                runtime_name = "jni/arm64-v8a/libonnxruntime.so"
            require(
                embedding_native in names and runtime_name in names,
                f"Kotlin embedding native inventory mismatch: {path.name}",
            )
            require(
                not any(
                    "RetrievalDatabase" in name or "GraphDatabase" in name
                    for name in classes
                ),
                f"Kotlin embedding class isolation mismatch: {path.name}",
            )
            native = native_reader(embedding_native)
            runtime = native_reader(runtime_name)
            if packaging == "jar":
                validate_macho_arm64(native, path.name)
                validate_macho_arm64(runtime, f"{path.name} ONNX Runtime")
                require(
                    len(runtime) == ONNX_MACOS_SIZE
                    and hashlib.sha256(runtime).hexdigest() == ONNX_MACOS_SHA256,
                    f"Kotlin embedding macOS runtime mismatch: {path.name}",
                )
            else:
                validate_elf_arm64(native, path.name)
                validate_elf_arm64(runtime, f"{path.name} ONNX Runtime")
                require(
                    len(runtime) == ONNX_ANDROID_SIZE
                    and hashlib.sha256(runtime).hexdigest() == ONNX_ANDROID_SHA256,
                    f"Kotlin embedding Android runtime mismatch: {path.name}",
                )
            return
        graph = capability.endswith("graph")
        expected_native = graph_native if graph else base_native
        excluded_native = base_native if graph else graph_native
        require(expected_native in names and excluded_native not in names, f"Kotlin native isolation mismatch: {path.name}")
        require(
            graph == any("GraphDatabase" in name for name in classes if name.endswith(".class")),
            f"Kotlin class isolation mismatch: {path.name}",
        )
        native = native_reader(expected_native)
    if packaging == "jar":
        validate_macho_arm64(native, path.name)
    else:
        validate_elf_arm64(native, path.name)


def validate_kotlin_packages(root: Path, config: dict[str, Any]) -> None:
    inventory = load_json(root / "inventory.json")
    require(
        inventory["kind"] == "retrievalkit-kotlin-release"
        and inventory["group"] == config["kotlin"]["group"]
        and inventory["version"] == config["version"],
        "Kotlin package inventory identity mismatch",
    )
    require(inventory["targets"] == config["kotlin"]["targets"], "Kotlin target inventory mismatch")
    require(
        inventory["publicationReady"] is False
        and inventory["bundle"]["signed"] is config["kotlin"]["signed_in_candidate"],
        "Kotlin candidate must remain unsigned and publication-closed",
    )
    require(
        inventory["artifactBlockers"]
        == ["Central requires PGP signatures; no signing key was supplied"],
        "Kotlin candidate blocker inventory mismatch",
    )
    bundle_name = config["kotlin"]["central_bundle"]
    require(inventory["bundle"]["file"] == bundle_name, "Kotlin Central bundle name mismatch")
    bundle_path = root / bundle_name
    require(inventory["bundle"]["sha256"] == digest(bundle_path), "Kotlin Central bundle hash mismatch")
    group = config["kotlin"]["group"]
    version = config["version"]
    group_path = Path(*group.split("."))
    expected_maven_files: set[Path] = set()
    inventory_rows = {row["coordinates"].split(":")[1]: row for row in inventory["artifacts"]}
    require(set(inventory_rows) == set(config["kotlin"]["artifacts"]), "Kotlin artifact inventory mismatch")
    for artifact_id, packaging in config["kotlin"]["artifacts"].items():
        coordinate_root = root / "maven" / group_path / artifact_id / version
        primary_names = [
            f"{artifact_id}-{version}.{packaging}",
            f"{artifact_id}-{version}-sources.jar",
            f"{artifact_id}-{version}-javadoc.jar",
            f"{artifact_id}-{version}.pom",
        ]
        primary_files = [coordinate_root / name for name in primary_names]
        require(all(path.is_file() for path in primary_files), f"Kotlin Maven files missing: {artifact_id}")
        for path in primary_files:
            expected_maven_files.add(path.relative_to(root / "maven"))
            for algorithm in ("md5", "sha1", "sha256", "sha512"):
                companion = path.with_name(f"{path.name}.{algorithm}")
                require(companion.is_file(), f"Kotlin checksum companion missing: {companion.name}")
                require(
                    companion.read_text(encoding="ascii").strip()
                    == hashlib.new(algorithm, path.read_bytes()).hexdigest(),
                    f"Kotlin checksum companion mismatch: {companion.name}",
                )
                expected_maven_files.add(companion.relative_to(root / "maven"))
        primary = coordinate_root / f"{artifact_id}-{version}.{packaging}"
        capability = next(
            row["capability"]
            for row in inventory["artifacts"]
            if row["coordinates"] == f"{group}:{artifact_id}:{version}"
        )
        validate_kotlin_primary(primary, capability, packaging)
        validate_maven_pom(
            coordinate_root / f"{artifact_id}-{version}.pom",
            group=group,
            artifact_id=artifact_id,
            version=version,
            packaging=packaging,
        )
        require(
            inventory_rows[artifact_id]["primarySha256"] == digest(primary),
            f"Kotlin primary inventory hash mismatch: {artifact_id}",
        )
        require(
            set(inventory_rows[artifact_id]["files"]) == set(primary_names),
            f"Kotlin artifact file inventory mismatch: {artifact_id}",
        )
        for classifier in ("sources", "javadoc"):
            require(
                zipfile.is_zipfile(coordinate_root / f"{artifact_id}-{version}-{classifier}.jar"),
                f"Kotlin {classifier} JAR is invalid: {artifact_id}",
            )
    observed_maven_files = {
        path.relative_to(root / "maven")
        for path in (root / "maven").rglob("*")
        if path.is_file()
    }
    require(observed_maven_files == expected_maven_files, "Kotlin Maven layout is not closed")
    with zipfile.ZipFile(bundle_path) as central:
        require(
            all(info.date_time == (1980, 1, 1, 0, 0, 0) for info in central.infolist()),
            "Kotlin Central bundle timestamps are not canonical",
        )
        central_files = set(central.namelist())
        require(
            central_files == {path.as_posix() for path in expected_maven_files},
            "Kotlin Central bundle inventory mismatch",
        )
        for relative in expected_maven_files:
            require(
                central.read(relative.as_posix()) == (root / "maven" / relative).read_bytes(),
                f"Kotlin Central bundle bytes mismatch: {relative}",
            )
    release_files = {
        path.relative_to(root).as_posix(): path
        for path in root.rglob("*")
        if path.is_file() and path.name not in {"inventory.json", "SHA256SUMS"}
    }
    validate_checksum_manifest(root / "SHA256SUMS", release_files, "sha256")


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
            *(path for path in (bundle / "artifacts").rglob("*") if path.is_file()),
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
    validate_node_packages(bundle / "artifacts/node", config)
    validate_browser_retrieval_package(
        bundle / "artifacts/browser-retrieval", config
    )
    validate_browser_embedding_package(
        bundle / "artifacts/browser-embedding", config
    )
    validate_kotlin_packages(bundle / "artifacts/kotlin", config)
    sbom = load_json(bundle / "sbom.spdx.json")
    require(sbom["spdxVersion"] == "SPDX-2.3" and sbom["packages"], "SBOM is missing package inventory")
    provenance = load_json(bundle / "provenance.intoto.json")
    expected_subjects = {
        path.relative_to(bundle / "artifacts").as_posix(): digest(path)
        for path in (bundle / "artifacts").rglob("*")
        if path.is_file()
    }
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
