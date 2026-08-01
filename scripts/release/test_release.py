from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


REPO = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


validator = load("release_validator", REPO / "scripts/release/validate_release.py")
assembler = load("release_assembler", REPO / "scripts/release/assemble_release.py")
canonical_zip = load("canonical_zip", REPO / "scripts/release/canonical_zip.py")
compare_artifacts = load("compare_artifacts", REPO / "scripts/release/compare_artifacts.py")
canonicalize_wheel = load("canonicalize_wheel", REPO / "scripts/release/canonicalize_wheel.py")
publication_authorization = load(
    "publication_authorization",
    REPO / "scripts/release/publication_authorization.py",
)


class ReleaseTests(unittest.TestCase):
    def test_static_release_metadata_passes_with_explicit_blockers(self) -> None:
        result = validator.static_validation(REPO)
        self.assertEqual(result["version"], "0.1.0")
        self.assertNotIn("root LICENSE is absent", result["publication_blockers"])
        self.assertNotIn("owner-approved NOTICE is absent", result["publication_blockers"])
        self.assertNotIn("owner publication authorization is absent", result["publication_blockers"])
        self.assertNotIn("standalone graph Swift package repository", " ".join(result["publication_blockers"]))

    def test_static_candidate_validation_does_not_claim_runtime_authority(self) -> None:
        result = validator.static_validation(REPO)
        self.assertNotIn(REPO / "release/publication-authorization-v1.json", REPO.glob("release/*"))
        self.assertNotIn("publication_ready", result)

    def test_publication_workflow_imports_the_pinned_tag_verification_key(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory)
            shutil.copytree(REPO / ".github", fake / ".github")
            validator.validate_workflows(fake)
            publication = fake / ".github/workflows/publish-release.yml"
            publication.write_text(
                publication.read_text(encoding="utf-8").replace(
                    'GNUPGHOME="$verification_home" gpg --batch --import "$RELEASE_SIGNING_KEY"',
                    'GNUPGHOME="$verification_home" gpg --batch --list-keys "$RELEASE_SIGNING_KEY"',
                    1,
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "clean-keyring signed-tag",
            ):
                validator.validate_workflows(fake)

    def test_swift_package_exposes_base_and_graph_through_one_aggregate(self) -> None:
        package = (REPO / "Package.swift").read_text()
        self.assertIn('.library(name: "RetrievalKit"', package)
        self.assertIn('.library(name: "RetrievalKitGraph"', package)
        self.assertIn("RetrievalKitGraphFFI.xcframework.zip", package)
        self.assertNotIn("RetrievalKitFFI.xcframework.zip", package)
        self.assertNotIn('.library(name: "RetrievalKitIngest"', package)
        self.assertFalse((REPO / "Package.graph.swift").exists())

    def test_release_identities_are_fixed_for_node_and_kotlin(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        self.assertEqual(config["python"]["requires_python"], ">=3.10,<3.15")
        self.assertEqual(
            config["persistence"],
            {
                "base_write_format": 4,
                "base_readable_formats": [1, 2, 3, 4],
            },
        )
        self.assertNotIn(
            "npm trusted publishing configured",
            " ".join(config["publication_blockers"]),
        )
        self.assertNotIn(
            "Maven Central namespace verification",
            " ".join(config["publication_blockers"]),
        )
        self.assertNotIn(
            "retrievalkit-embedding PyPI project bootstrapped",
            " ".join(config["publication_blockers"]),
        )
        self.assertNotIn(
            "retrievalkit-browser npm package bootstrapped",
            " ".join(config["publication_blockers"]),
        )
        self.assertEqual(
            config["release_freeze"],
            {
                "status": "frozen",
                "frozen_on": "2026-08-01",
                "revision_binding": "commit-containing-this-record",
                "post_freeze_change_policy": "new-freeze-commit-required",
            },
        )
        self.assertEqual(
            config["node"]["packages"],
            {
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
        )
        self.assertEqual(
            config["browser_retrieval"],
            {
                "engines": "^22.13.0 || ^24.0.0",
                "runtime": "dedicated-worker-wasm",
                "wasm_tiers": ["portable", "simd128"],
                "package": {
                    "name": "@gungorbasa/retrievalkit-browser",
                    "artifact": "gungorbasa-retrievalkit-browser-0.1.0.tgz",
                },
            },
        )
        self.assertEqual(
            config["browser_embedding"]["package"],
            {
                "name": "@gungorbasa/retrievalkit-browser-embedding",
                "artifact": "gungorbasa-retrievalkit-browser-embedding-0.1.0.tgz",
            },
        )
        self.assertEqual(config["kotlin"]["group"], "io.github.gungorbasa")
        self.assertEqual(
            set(config["kotlin"]["artifacts"]),
            {
                "retrievalkit",
                "retrievalkit-graph",
                "retrievalkit-android",
                "retrievalkit-graph-android",
                "retrievalkit-embedding",
                "retrievalkit-embedding-android",
            },
        )
        self.assertEqual(
            config["kotlin"]["android_preview"],
            {
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
        )
        self.assertFalse(
            any(
                "android" in blocker.lower()
                and (
                    "device inference" in blocker.lower()
                    or "physical device" in blocker.lower()
                )
                for blocker in config["publication_blockers"]
            )
        )
        signing = config["kotlin"]["signing"]
        self.assertEqual(
            signing["fingerprint"],
            "0E82F1A5487A4EF3CCF1ED6C393266CD4DD158ED",
        )
        self.assertEqual(
            validator.digest(REPO / signing["public_key"]),
            signing["sha256"],
        )

    def test_python_metadata_rejects_open_ended_support(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        validator.validate_python_release_metadata(REPO, config)
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory)
            for relative in (
                Path("wrappers/python/pyproject.toml"),
                Path("wrappers/python-graph/pyproject.toml"),
                Path("wrappers/python-embedding/pyproject.toml"),
            ):
                target = fake / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(REPO / relative, target)
            graph_project = fake / "wrappers/python-graph/pyproject.toml"
            graph_project.write_text(
                graph_project.read_text(encoding="utf-8").replace(
                    'requires-python = ">=3.10,<3.15"',
                    'requires-python = ">=3.10"',
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "Python requires-python mismatch: wrappers/python-graph",
            ):
                validator.validate_python_release_metadata(fake, config)
            graph_project.write_text(
                graph_project.read_text(encoding="utf-8").replace(
                    'requires-python = ">=3.10"',
                    "",
                )
                + '\n[tool.release-test]\nrequires-python = ">=3.10,<3.15"\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "Python requires-python missing: wrappers/python-graph",
            ):
                validator.validate_python_release_metadata(fake, config)

    def test_android_maven_pom_requires_preview_description(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pom = Path(directory) / "retrievalkit-android-0.1.0.pom"
            preview = """\
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <groupId>io.github.gungorbasa</groupId>
  <artifactId>retrievalkit-android</artifactId>
  <version>0.1.0</version>
  <packaging>aar</packaging>
  <name>RetrievalKit Android arm64-v8a</name>
  <description>Preview AAR for Android arm64-v8a</description>
  <url>https://retrievalkit-docs.gungorbasa.chatgpt.site</url>
  <licenses><license><name>Apache License, Version 2.0</name></license></licenses>
  <developers><developer><name>RetrievalKit</name></developer></developers>
  <scm><url>https://github.com/gungorbasa/RetrievalKit</url></scm>
</project>
"""
            pom.write_text(preview, encoding="utf-8")
            arguments = {
                "group": "io.github.gungorbasa",
                "artifact_id": "retrievalkit-android",
                "version": "0.1.0",
                "packaging": "aar",
            }
            validator.validate_maven_pom(pom, **arguments)
            pom.write_text(
                preview.replace("Preview AAR", "Production AAR"),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "must declare preview status",
            ):
                validator.validate_maven_pom(pom, **arguments)

    def test_persistence_documentation_rejects_v3_as_current(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        validator.validate_persistence_release_contract(REPO, config)
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory)
            for relative in (
                Path("CHANGELOG.md"),
                Path("crates/retrievalkit-core/src/index.rs"),
                Path("docs/product/compatibility-policy.md"),
                Path("docs/product/retrievalkit-product-spec.md"),
                Path("docs/product/v0.1.0-migration.md"),
                Path("wrappers/python/README.md"),
                Path("wrappers/swift/RetrievalKit/README.md"),
            ):
                target = fake / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(REPO / relative, target)
            python_readme = fake / "wrappers/python/README.md"
            python_readme.write_text(
                python_readme.read_text(encoding="utf-8").replace(
                    "New saves use a checksummed V4 manifest",
                    "New saves use a checksummed V3 manifest",
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "base persistence documentation mismatch: wrappers/python/README.md",
            ):
                validator.validate_persistence_release_contract(fake, config)
            shutil.copy2(REPO / "wrappers/python/README.md", python_readme)
            core = fake / "crates/retrievalkit-core/src/index.rs"
            core.write_text(
                core.read_text(encoding="utf-8").replace(
                    "const LEGACY_FORMAT_VERSION: u32 = 1;",
                    "const LEGACY_FORMAT_VERSION: u32 = 0;",
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "Rust base readable persistence formats differ",
            ):
                validator.validate_persistence_release_contract(fake, config)
            shutil.copy2(REPO / "crates/retrievalkit-core/src/index.rs", core)
            compatibility = fake / "docs/product/compatibility-policy.md"
            compatibility.write_text(
                compatibility.read_text(encoding="utf-8").replace(
                    "Graph capability formats are validated independently.",
                    "",
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                (
                    "base persistence documentation mismatch: "
                    "docs/product/compatibility-policy.md"
                ),
            ):
                validator.validate_persistence_release_contract(fake, config)

    def test_active_release_claims_reject_obsolete_node_identity(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        validator.validate_active_release_claims(REPO, config)
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory)
            for relative in (
                Path("docs/product/retrievalkit-product-spec.md"),
                Path(
                    "docs/product/reports/"
                    "cross-language-wrapper-parity-audit.md"
                ),
            ):
                target = fake / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(REPO / relative, target)
            product_spec = fake / "docs/product/retrievalkit-product-spec.md"
            product_spec.write_text(
                product_spec.read_text(encoding="utf-8").replace(
                    "`@gungorbasa/retrievalkit`",
                    "`@gungorbasa/retrievalkit-graph`",
                    1,
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "active product spec lacks fixed Node or Maven release identities",
            ):
                validator.validate_active_release_claims(fake, config)

    def test_wheel_requires_python_matches_qualified_range(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            valid = root / "valid.whl"
            invalid = root / "invalid.whl"
            with zipfile.ZipFile(valid, "w") as wheel:
                wheel.writestr(
                    "retrievalkit-0.1.0.dist-info/METADATA",
                    "Metadata-Version: 2.4\n"
                    "Name: retrievalkit\n"
                    "Requires-Python: >=3.10,\n"
                    " <3.15\n",
                )
            with zipfile.ZipFile(valid) as wheel:
                validator.validate_wheel_requires_python(
                    wheel,
                    valid.name,
                    ">=3.10,<3.15",
                )
            with zipfile.ZipFile(invalid, "w") as wheel:
                wheel.writestr(
                    "retrievalkit-0.1.0.dist-info/METADATA",
                    "Metadata-Version: 2.4\n"
                    "Name: retrievalkit\n"
                    "Requires-Python: >=3.10\n",
                )
            with zipfile.ZipFile(invalid) as wheel, self.assertRaisesRegex(
                validator.ValidationError,
                "wheel Requires-Python mismatch",
            ):
                validator.validate_wheel_requires_python(
                    wheel,
                    invalid.name,
                    ">=3.10,<3.15",
                )
            integration = (
                root
                / "retrievalkit-0.1.0-cp310-cp310-macosx_11_0_arm64.whl"
            )
            with zipfile.ZipFile(integration, "w") as wheel:
                wheel.writestr(
                    "retrievalkit-0.1.0.dist-info/RECORD",
                    "",
                )
                wheel.writestr(
                    "retrievalkit-0.1.0.dist-info/METADATA",
                    "Metadata-Version: 2.4\n"
                    "Name: retrievalkit\n"
                    "Requires-Python: >=3.10\n",
                )
            config = validator.load_json(REPO / "release/release-v0.1.0.json")
            with self.assertRaisesRegex(
                validator.ValidationError,
                "wheel Requires-Python mismatch",
            ):
                validator.validate_wheels([integration], config)

    def test_release_bundle_includes_license_and_notice(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / "staging"
            staging.mkdir()
            (staging / "demo.zip").write_bytes(b"artifact")
            output = root / "bundle"
            assembler.assemble(REPO, staging, output, "a" * 40)
            for legal_name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
                self.assertEqual(
                    (output / legal_name).read_bytes(),
                    (REPO / legal_name).read_bytes(),
                )
                self.assertIn(legal_name, validator.BUNDLE_LEGAL_FILES)
                self.assertIn(
                    legal_name,
                    validator.load_json(output / "inventory.json")["files"],
                )

    def test_wrapper_legal_files_match_root(self) -> None:
        result = validator.static_validation(REPO)
        self.assertNotIn("third-party notices are absent", result["publication_blockers"])
        for wrapper in (
            "wrappers/python",
            "wrappers/python-graph",
            "wrappers/python-embedding",
        ):
            for legal_name in ("LICENSE", "NOTICE"):
                self.assertNotIn(
                    f"wrapper legal file out of sync: {wrapper}/{legal_name}",
                    result["publication_blockers"],
                )
                self.assertEqual(
                    (REPO / wrapper / legal_name).read_bytes(),
                    (REPO / legal_name).read_bytes(),
                )

    def test_out_of_sync_wrapper_legal_file_is_a_blocker(self) -> None:
        blockers = validator.publication_blockers(
            REPO, validator.load_json(REPO / "release/release-v0.1.0.json")
        )
        self.assertNotIn("wrapper legal file out of sync: wrappers/python/LICENSE", blockers)
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory)
            for name in ("Cargo.toml", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
                shutil.copy2(REPO / name, fake / name)
            for wrapper in (
                "wrappers/python",
                "wrappers/python-graph",
                "wrappers/python-embedding",
            ):
                (fake / wrapper).mkdir(parents=True)
                shutil.copy2(REPO / wrapper / "pyproject.toml", fake / wrapper / "pyproject.toml")
                shutil.copy2(REPO / "NOTICE", fake / wrapper / "NOTICE")
            (fake / "wrappers/python/LICENSE").write_text("stale", encoding="utf-8")
            shutil.copy2(REPO / "LICENSE", fake / "wrappers/python-graph/LICENSE")
            shutil.copy2(REPO / "LICENSE", fake / "wrappers/python-embedding/LICENSE")
            config = validator.load_json(REPO / "release/release-v0.1.0.json")
            blockers = validator.publication_blockers(fake, config)
            self.assertIn("wrapper legal file out of sync: wrappers/python/LICENSE", blockers)

    def test_canonical_xcframework_zip_is_byte_identical(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            framework = root / "Demo.xcframework"
            (framework / "slice").mkdir(parents=True)
            (framework / "slice/value.bin").write_bytes(b"same")
            first = root / "first.zip"
            second = root / "second.zip"
            canonical_zip.archive(framework, first)
            canonical_zip.archive(framework, second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                self.assertTrue(all(item.date_time == (1980, 1, 1, 0, 0, 0) for item in archive.infolist()))

    def test_wheel_matrix_rejects_missing_artifacts(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        with self.assertRaisesRegex(validator.ValidationError, "wheel matrix mismatch"):
            validator.validate_wheels([], config)

    def test_altered_apple_checksum_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "RetrievalKitGraphFFI.xcframework.zip"
            archive.write_bytes(b"altered")
            with self.assertRaisesRegex(validator.ValidationError, "checksum mismatch"):
                validator.validate_xcframework_archive(archive, "0.1.0", "0" * 64)

    def test_mismatched_wheel_version_is_rejected(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        with tempfile.TemporaryDirectory() as directory:
            wheel = Path(directory) / "retrievalkit-9.9.9-cp310-cp310-macosx_11_0_arm64.whl"
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("retrievalkit-9.9.9.dist-info/RECORD", "")
            with self.assertRaisesRegex(validator.ValidationError, "version mismatch"):
                validator.validate_wheels([wheel], config)

    def test_bundle_without_provenance_attestation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(validator.ValidationError, "root inventory mismatch"):
                validator.bundle_validation(REPO, Path(directory))

    def test_nested_package_artifacts_are_closed_and_provenanced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / "staging"
            staging.mkdir()
            (staging / "RetrievalKitGraphFFI.xcframework.zip").write_bytes(b"apple")
            (staging / "node").mkdir()
            (staging / "node/gungorbasa-retrievalkit-0.1.0.tgz").write_bytes(
                b"node-base"
            )
            (staging / "node/inventory.json").write_text("{}", encoding="utf-8")
            (staging / "browser-retrieval").mkdir()
            (
                staging
                / "browser-retrieval/gungorbasa-retrievalkit-browser-0.1.0.tgz"
            ).write_bytes(b"browser-retrieval")
            (staging / "browser-retrieval/inventory.json").write_text(
                "{}", encoding="utf-8"
            )
            kotlin_coordinate = (
                staging
                / "kotlin/maven/io/github/gungorbasa/retrievalkit/0.1.0"
            )
            kotlin_coordinate.mkdir(parents=True)
            (kotlin_coordinate / "retrievalkit-0.1.0.jar").write_bytes(b"kotlin-base")
            (staging / "kotlin/inventory.json").write_text("{}", encoding="utf-8")
            output = root / "bundle"
            assembler.assemble(REPO, staging, output, "a" * 40)

            with (
                mock.patch.object(
                    validator,
                    "static_validation",
                    return_value={
                        "version": "0.1.0",
                        "publication_blockers": ["candidate remains closed"],
                    },
                ),
                mock.patch.object(validator, "validate_xcframework_archive"),
                mock.patch.object(validator, "validate_wheels"),
                mock.patch.object(validator, "validate_node_packages") as node_validation,
                mock.patch.object(
                    validator, "validate_browser_retrieval_package"
                ) as browser_retrieval_validation,
                mock.patch.object(
                    validator, "validate_browser_embedding_package"
                ) as browser_embedding_validation,
                mock.patch.object(validator, "validate_kotlin_packages") as kotlin_validation,
            ):
                result = validator.bundle_validation(REPO, output)

            self.assertEqual(result["artifact_count"], 7)
            node_validation.assert_called_once_with(
                output / "artifacts/node",
                validator.load_json(REPO / "release/release-v0.1.0.json"),
            )
            browser_embedding_validation.assert_called_once_with(
                output / "artifacts/browser-embedding",
                validator.load_json(REPO / "release/release-v0.1.0.json"),
            )
            browser_retrieval_validation.assert_called_once_with(
                output / "artifacts/browser-retrieval",
                validator.load_json(REPO / "release/release-v0.1.0.json"),
            )
            kotlin_validation.assert_called_once_with(
                output / "artifacts/kotlin",
                validator.load_json(REPO / "release/release-v0.1.0.json"),
            )
            subjects = {
                row["name"]
                for row in validator.load_json(output / "provenance.intoto.json")["subject"]
            }
            self.assertEqual(
                subjects,
                {
                    "RetrievalKitGraphFFI.xcframework.zip",
                    "node/inventory.json",
                    "node/gungorbasa-retrievalkit-0.1.0.tgz",
                    "browser-retrieval/inventory.json",
                    (
                        "browser-retrieval/"
                        "gungorbasa-retrievalkit-browser-0.1.0.tgz"
                    ),
                    "kotlin/inventory.json",
                    (
                        "kotlin/maven/io/github/gungorbasa/retrievalkit/0.1.0/"
                        "retrievalkit-0.1.0.jar"
                    ),
                },
            )
            (
                output / "artifacts/node/gungorbasa-retrievalkit-0.1.0.tgz"
            ).write_bytes(b"changed")
            with self.assertRaisesRegex(validator.ValidationError, "checksum mismatch"):
                validator.bundle_validation(REPO, output)

    def test_two_root_comparison_rejects_changed_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "first"
            second = root / "second"
            first.mkdir()
            second.mkdir()
            (first / "artifact").write_bytes(b"a")
            (second / "artifact").write_bytes(b"b")
            with self.assertRaisesRegex(ValueError, "bytes differ"):
                compare_artifacts.compare(first, second)

    def test_wheel_canonicalization_remaps_checkout_and_is_stable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wheel = root / "retrievalkit-0.1.0-cp310-cp310-macosx_11_0_arm64.whl"
            sbom_name = "retrievalkit-0.1.0.dist-info/sboms/test.json"
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr(sbom_name, '{"ref":"path+file://' + root.resolve().as_posix() + '/crates/retrievalkit-core#0.1.0"}')
                archive.writestr("retrievalkit-0.1.0.dist-info/RECORD", "")
            canonicalize_wheel.canonicalize(root, wheel)
            first = wheel.read_bytes()
            canonicalize_wheel.canonicalize(root, wheel)
            self.assertEqual(first, wheel.read_bytes())
            with zipfile.ZipFile(wheel) as archive:
                self.assertIn(b"path+file:///workspace/crates/", archive.read(sbom_name))


class PublicationAuthorizationTests(unittest.TestCase):
    repository = "gungorbasa/RetrievalKit"
    tag = "v0.1.0"
    revision = "a" * 40

    def write_json(self, path: Path, value: object) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def run_payload(
        self,
        run_id: int,
        workflow_path: str,
        *,
        event: str = "workflow_dispatch",
        status: str = "completed",
        conclusion: str | None = "success",
        revision: str | None = None,
    ) -> dict[str, object]:
        return {
            "id": run_id,
            "run_attempt": 1,
            "head_sha": revision or self.revision,
            "path": workflow_path,
            "event": event,
            "status": status,
            "conclusion": conclusion,
            "run_started_at": "2026-07-26T06:59:00Z",
            "html_url": f"https://github.com/{self.repository}/actions/runs/{run_id}",
            "repository": {"full_name": self.repository},
        }

    def fixture(self, root: Path) -> tuple[SimpleNamespace, Path]:
        bundle = root / "bundle"
        bundle.mkdir()
        inventory = self.write_json(bundle / "inventory.json", {"schema_version": 1, "files": {}})
        (bundle / "checksums.sha256").write_text("candidate\n", encoding="utf-8")
        self.write_json(
            bundle / "release-manifest.json",
            {
                "tag": self.tag,
                "source_revision": self.revision,
                "publication_ready": False,
                "artifact_count": 11,
                "inventory_sha256": publication_authorization.sha256(inventory),
            },
        )
        candidate_run = self.write_json(
            root / "candidate-run.json",
            self.run_payload(101, publication_authorization.WORKFLOW_PATHS["candidate"]),
        )
        scheduled_run = self.write_json(
            root / "scheduled-run.json",
            self.run_payload(
                102,
                publication_authorization.WORKFLOW_PATHS["scheduled_gate"],
                event="schedule",
            ),
        )
        release_run = self.write_json(
            root / "release-run.json",
            self.run_payload(103, publication_authorization.WORKFLOW_PATHS["release_gate"]),
        )
        scheduled_result = self.write_json(
            root / "scheduled-result.json",
            {
                "tier": "scheduled_full",
                "overall_status": "passed",
                "source_revision": self.revision,
            },
        )
        release_result = self.write_json(
            root / "release-result.json",
            {
                "tier": "release",
                "overall_status": "passed",
                "source_revision": self.revision,
            },
        )
        output = root / "candidate-evidence.json"
        args = SimpleNamespace(
            repository=self.repository,
            tag=self.tag,
            source_revision=self.revision,
            candidate_run_id=101,
            candidate_run_json=candidate_run,
            scheduled_run_id=102,
            scheduled_run_json=scheduled_run,
            release_gate_run_id=103,
            release_gate_run_json=release_run,
            bundle=bundle,
            scheduled_result=scheduled_result,
            release_gate_result=release_result,
            output=output,
        )
        return args, output

    def authorize_args(self, root: Path, candidate_path: Path) -> SimpleNamespace:
        publication_run = self.write_json(
            root / "publication-run.json",
            self.run_payload(
                104,
                publication_authorization.WORKFLOW_PATHS["publication"],
                status="in_progress",
                conclusion=None,
            ),
        )
        approvals = self.write_json(
            root / "approvals.json",
            [
                {
                    "environments": [{"name": "release"}],
                    "user": {"login": "release-owner"},
                    "comment": "Approved exact v0.1.0 candidate",
                    "state": "approved",
                    "created_at": "2026-07-26T07:00:00Z",
                }
            ],
        )
        return SimpleNamespace(
            repository=self.repository,
            tag=self.tag,
            source_revision=self.revision,
            candidate_run_id=101,
            scheduled_run_id=102,
            release_gate_run_id=103,
            candidate_evidence=candidate_path,
            publication_run_id=104,
            publication_run_attempt=1,
            publication_run_json=publication_run,
            approvals_json=approvals,
            workflow_ref=(
                f"{self.repository}/.github/workflows/publish-release.yml"
                f"@refs/tags/{self.tag}"
            ),
            actor="release-operator",
            triggering_actor="release-operator",
            environment="release",
            generated_at="2026-07-26T07:00:01Z",
            output=root / "authorization.json",
        )

    def test_protected_environment_approval_closes_exact_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            candidate = publication_authorization.build_candidate_evidence(candidate_args)
            publication_authorization.write_object(candidate_path, candidate)
            authorization_args = self.authorize_args(root, candidate_path)
            record = publication_authorization.build_authorization_record(authorization_args)
            publication_authorization.write_object(authorization_args.output, record)
            publication_authorization.validate_authorization_record(
                record,
                candidate_path=candidate_path,
                repository=self.repository,
                tag=self.tag,
                revision=self.revision,
                candidate_run_id=101,
                scheduled_run_id=102,
                release_gate_run_id=103,
                publication_run_id=104,
                publication_run_attempt=1,
            )
            validator.validate_runtime_authorization(
                REPO,
                candidate_args.bundle,
                authorization_args.output,
                candidate_path,
                candidate_args.scheduled_result,
                candidate_args.release_gate_result,
                self.repository,
                self.revision,
                101,
                102,
                103,
                104,
                1,
            )
            self.assertEqual(
                record["authority"]["type"],
                "github_environment_required_reviewer",
            )
            self.assertEqual(
                record["candidate_evidence"]["runs"]["release_gate"]["run_id"],
                103,
            )

    def test_runtime_validation_rejects_bundle_changed_after_approval(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            publication_authorization.write_object(
                candidate_path,
                publication_authorization.build_candidate_evidence(candidate_args),
            )
            authorization_args = self.authorize_args(root, candidate_path)
            publication_authorization.write_object(
                authorization_args.output,
                publication_authorization.build_authorization_record(authorization_args),
            )
            (candidate_args.bundle / "checksums.sha256").write_text(
                "changed after approval\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                validator.ValidationError,
                "authorized bundle differs",
            ):
                validator.validate_runtime_authorization(
                    REPO,
                    candidate_args.bundle,
                    authorization_args.output,
                    candidate_path,
                    candidate_args.scheduled_result,
                    candidate_args.release_gate_result,
                    self.repository,
                    self.revision,
                    101,
                    102,
                    103,
                    104,
                    1,
                )

    def test_candidate_rejects_gate_result_from_another_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args, _ = self.fixture(root)
            self.write_json(
                args.release_gate_result,
                {
                    "tier": "release",
                    "overall_status": "passed",
                    "source_revision": "b" * 40,
                },
            )
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "source revision mismatch",
            ):
                publication_authorization.build_candidate_evidence(args)

    def test_candidate_rejects_successful_run_from_another_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args, _ = self.fixture(root)
            self.write_json(
                args.candidate_run_json,
                self.run_payload(
                    101,
                    publication_authorization.WORKFLOW_PATHS["candidate"],
                    revision="b" * 40,
                ),
            )
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "run revision mismatch",
            ):
                publication_authorization.build_candidate_evidence(args)

    def test_unprotected_environment_without_approval_event_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            publication_authorization.write_object(
                candidate_path,
                publication_authorization.build_candidate_evidence(candidate_args),
            )
            authorization_args = self.authorize_args(root, candidate_path)
            self.write_json(authorization_args.approvals_json, [])
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "no approved required-reviewer event",
            ):
                publication_authorization.build_authorization_record(authorization_args)

    def test_approval_from_an_earlier_run_attempt_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            publication_authorization.write_object(
                candidate_path,
                publication_authorization.build_candidate_evidence(candidate_args),
            )
            authorization_args = self.authorize_args(root, candidate_path)
            publication_run = publication_authorization.load_object(
                authorization_args.publication_run_json
            )
            publication_run["run_started_at"] = "2026-07-26T07:01:00Z"
            self.write_json(authorization_args.publication_run_json, publication_run)
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "no approved required-reviewer event",
            ):
                publication_authorization.build_authorization_record(authorization_args)

    def test_publication_workflow_must_run_from_exact_signed_tag_ref(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            publication_authorization.write_object(
                candidate_path,
                publication_authorization.build_candidate_evidence(candidate_args),
            )
            authorization_args = self.authorize_args(root, candidate_path)
            authorization_args.workflow_ref = (
                f"{self.repository}/.github/workflows/publish-release.yml@refs/heads/main"
            )
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "exact signed tag",
            ):
                publication_authorization.build_authorization_record(authorization_args)

    def test_authorization_rejects_modified_candidate_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_args, candidate_path = self.fixture(root)
            publication_authorization.write_object(
                candidate_path,
                publication_authorization.build_candidate_evidence(candidate_args),
            )
            authorization_args = self.authorize_args(root, candidate_path)
            record = publication_authorization.build_authorization_record(authorization_args)
            candidate = publication_authorization.load_object(candidate_path)
            candidate["bundle"]["artifact_count"] = 12
            publication_authorization.write_object(candidate_path, candidate)
            with self.assertRaisesRegex(
                publication_authorization.AuthorizationError,
                "digest mismatch",
            ):
                publication_authorization.validate_authorization_record(
                    record,
                    candidate_path=candidate_path,
                    repository=self.repository,
                    tag=self.tag,
                    revision=self.revision,
                    candidate_run_id=101,
                    scheduled_run_id=102,
                    release_gate_run_id=103,
                    publication_run_id=104,
                    publication_run_attempt=1,
                )


if __name__ == "__main__":
    unittest.main()
