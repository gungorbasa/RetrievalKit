from __future__ import annotations

import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


validator = load("release_validator", REPO / "scripts/release/validate_release.py")
canonical_zip = load("canonical_zip", REPO / "scripts/release/canonical_zip.py")
compare_artifacts = load("compare_artifacts", REPO / "scripts/release/compare_artifacts.py")


class ReleaseTests(unittest.TestCase):
    def test_static_release_metadata_passes_with_explicit_blockers(self) -> None:
        result = validator.static_validation(REPO)
        self.assertEqual(result["version"], "0.1.0")
        self.assertIn("root LICENSE is absent", result["publication_blockers"])
        self.assertIn("owner publication authorization is absent", result["publication_blockers"])

    def test_publication_fails_closed_without_license_and_authorization(self) -> None:
        result = validator.static_validation(REPO)
        with self.assertRaises(validator.ValidationError):
            validator.require(not result["publication_blockers"], "publication blocked")

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
            archive = Path(directory) / "VectorKitFFI.xcframework.zip"
            archive.write_bytes(b"altered")
            with self.assertRaisesRegex(validator.ValidationError, "checksum mismatch"):
                validator.validate_xcframework_archive(archive, "0.1.0", "0" * 64)

    def test_mismatched_wheel_version_is_rejected(self) -> None:
        config = validator.load_json(REPO / "release/release-v0.1.0.json")
        with tempfile.TemporaryDirectory() as directory:
            wheel = Path(directory) / "vectorkit-9.9.9-cp310-cp310-macosx_11_0_arm64.whl"
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("vectorkit-9.9.9.dist-info/RECORD", "")
            with self.assertRaisesRegex(validator.ValidationError, "version mismatch"):
                validator.validate_wheels([wheel], config)

    def test_bundle_without_provenance_attestation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(validator.ValidationError, "root inventory mismatch"):
                validator.bundle_validation(REPO, Path(directory))

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


if __name__ == "__main__":
    unittest.main()
