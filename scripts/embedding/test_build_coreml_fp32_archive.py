from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/embedding/build-coreml-fp32-archive.py"


def load_builder():
    spec = importlib.util.spec_from_file_location("coreml_archive_builder", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


builder = load_builder()


class CoreMLFP32ArchiveTests(unittest.TestCase):
    def fixture(self, root: Path) -> Path:
        for relative in builder.PAYLOAD_PATHS:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"fixture:{relative}\n".encode())
        return root

    def records(self, root: Path):
        return [
            {
                "path": relative,
                "size": (root / relative).stat().st_size,
                "sha256": builder.sha256_file(root / relative),
            }
            for relative in builder.PAYLOAD_PATHS
        ]

    def build_with_fixture(self, source: Path, output: Path):
        records = self.records(source)
        with mock.patch.object(builder, "validate_source", return_value=records):
            return builder.build_archive(source, output)

    def test_deterministic_rebuild_and_clean_tree_match(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.TARGET_DIR) as temporary:
            root = Path(temporary)
            source = self.fixture(root / "source")
            first, first_manifest = self.build_with_fixture(source, root / "first")
            second, second_manifest = self.build_with_fixture(source, root / "second")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            self.assertEqual(first_manifest.read_bytes(), second_manifest.read_bytes())

            extracted = root / "extracted"
            builder.extract_archive(first, first_manifest, extracted)
            with mock.patch.object(builder, "validate_source", return_value=self.records(source)):
                builder.compare_source_tree(source, extracted)
            self.assertEqual(
                {
                    path.relative_to(extracted).as_posix()
                    for path in extracted.rglob("*")
                    if path.is_file()
                },
                {builder.ARCHIVE_MANIFEST_NAME, *builder.PAYLOAD_PATHS},
            )

    def test_manifest_records_exact_sizes_hashes_and_tree_digest(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.TARGET_DIR) as temporary:
            root = Path(temporary)
            source = self.fixture(root / "source")
            archive, manifest = self.build_with_fixture(source, root / "output")
            document = builder.validate_archive(archive, manifest)
            self.assertEqual(document["schemaVersion"], 1)
            self.assertEqual(document["modelPath"], builder.MODEL_DIRECTORY)
            self.assertEqual(document["tokenizerPath"], builder.TOKENIZER_DIRECTORY)
            self.assertEqual(
                document["canonicalTreeSHA256"],
                builder.canonical_tree_sha256(document["files"]),
            )
            self.assertEqual(
                [item["path"] for item in document["files"]],
                list(builder.PAYLOAD_PATHS),
            )

    def malicious_archive(self, names_and_types):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name, entry_type in names_and_types:
                info = builder.tar_info(name, 1 if entry_type == tarfile.REGTYPE else 0)
                info.type = entry_type
                archive.addfile(info, io.BytesIO(b"x") if info.isreg() else None)
        buffer.seek(0)
        return tarfile.open(fileobj=buffer, mode="r:")

    def test_rejects_traversal_and_absolute_paths(self) -> None:
        for unsafe in ("../escape", "safe/../../escape", "/absolute", "safe\\escape"):
            with self.subTest(unsafe=unsafe):
                archive = self.malicious_archive([(unsafe, tarfile.REGTYPE)])
                with archive, self.assertRaisesRegex(ValueError, "unsafe|absolute|backslash"):
                    builder.validate_members(archive, {})

    def test_rejects_links(self) -> None:
        for entry_type in (tarfile.SYMTYPE, tarfile.LNKTYPE):
            with self.subTest(entry_type=entry_type):
                archive = self.malicious_archive([("link", entry_type)])
                with archive, self.assertRaisesRegex(ValueError, "not a regular file"):
                    builder.validate_members(archive, {"link": {"size": 0, "sha256": ""}})

    def test_rejects_duplicate_entries(self) -> None:
        archive = self.malicious_archive(
            [("duplicate", tarfile.REGTYPE), ("duplicate", tarfile.REGTYPE)]
        )
        with archive, self.assertRaisesRegex(ValueError, "duplicate archive entry"):
            builder.validate_members(
                archive,
                {"duplicate": {"size": 1, "sha256": hashlib.sha256(b"x").hexdigest()}},
            )

    def test_rejects_unexpected_entries(self) -> None:
        archive = self.malicious_archive([("unexpected", tarfile.REGTYPE)])
        with archive, self.assertRaisesRegex(ValueError, "unexpected archive entry"):
            builder.validate_members(archive, {})

    def test_rejects_manifest_with_duplicate_or_unexpected_files(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.TARGET_DIR) as temporary:
            source = self.fixture(Path(temporary) / "source")
            records = self.records(source)
            document = builder.manifest_document(records)
            document["files"].append(dict(document["files"][0]))
            with self.assertRaisesRegex(ValueError, "duplicate manifest file path"):
                builder.parse_manifest(json.dumps(document).encode())

            document = builder.manifest_document(records)
            document["files"][0]["path"] = "unexpected"
            document["canonicalTreeSHA256"] = builder.canonical_tree_sha256(
                document["files"]
            )
            with self.assertRaisesRegex(ValueError, "unexpected file set"):
                builder.parse_manifest(json.dumps(document).encode())

    def test_rejects_source_symlink(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.TARGET_DIR) as temporary:
            root = Path(temporary)
            package = root / builder.MODEL_DIRECTORY
            package.mkdir(parents=True)
            target = root / "target"
            target.write_bytes(b"x")
            (package / "link").symlink_to(target)
            with self.assertRaisesRegex(ValueError, "symbolic link"):
                builder.package_tree_stats(package)

    def test_https_download_rejects_non_https(self) -> None:
        with tempfile.TemporaryDirectory(dir=builder.TARGET_DIR) as temporary:
            with self.assertRaisesRegex(ValueError, "must use HTTPS"):
                builder.verified_https_download(
                    "http://example.invalid/artifact.tar",
                    Path(temporary) / "artifact.tar",
                )


if __name__ == "__main__":
    unittest.main()
