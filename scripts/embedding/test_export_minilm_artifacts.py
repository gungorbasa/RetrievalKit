from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/embedding/export-minilm-artifacts.py"


def load_exporter():
    spec = importlib.util.spec_from_file_location("minilm_artifact_exporter", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    # Python 3.14 dataclasses consult sys.modules while processing annotations.
    import sys

    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


exporter = load_exporter()


class MiniLMArtifactExportTests(unittest.TestCase):
    def make_complete_fixture(self, root: Path, *, flexible: bool = False):
        specs = exporter.artifact_plan(
            include_flexible_coreml_candidate=flexible,
        )
        for spec in specs:
            path = root / spec.relative_path
            if path.suffix == ".mlpackage":
                payload = path / "Data" / "com.apple.CoreML" / "model.mlmodel"
                payload.parent.mkdir(parents=True)
                payload.write_bytes(spec.name.encode())
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(spec.name.encode())
        tokenizer = root / "tokenizer"
        tokenizer.mkdir()
        for filename in exporter.TOKENIZER_FILES:
            (tokenizer / filename).write_bytes(f"pinned:{filename}\n".encode())
        return specs

    def test_default_plan_has_three_onnx_and_three_coreml_artifacts(self) -> None:
        specs = exporter.artifact_plan()
        self.assertEqual(len(specs), 6)
        self.assertEqual(
            [item.name for item in specs],
            [
                "onnx-fp32",
                "onnx-fp16",
                "onnx-dynamic-q8",
                "coreml-fp32",
                "coreml-fp16",
                "coreml-weight-only-q8",
            ],
        )
        self.assertEqual(
            [Path(item.relative_path).name for item in specs],
            [
                "all-MiniLM-L6-v2-fp32.onnx",
                "all-MiniLM-L6-v2-fp16.onnx",
                "all-MiniLM-L6-v2-q8.onnx",
                "all-MiniLM-L6-v2-fp32.mlpackage",
                "all-MiniLM-L6-v2-fp16.mlpackage",
                "all-MiniLM-L6-v2-q8.mlpackage",
            ],
        )
        onnx = [item for item in specs if item.format == "onnx"]
        self.assertTrue(all(item.sequence_shape == "dynamic" for item in onnx))

    def test_q8_recipe_preserves_quality_sensitive_matmuls(self) -> None:
        self.assertEqual(len(exporter.ONNX_Q8_NODES_TO_EXCLUDE), 7)
        self.assertTrue(
            all(name.endswith("/MatMul") for name in exporter.ONNX_Q8_NODES_TO_EXCLUDE)
        )
        self.assertEqual(
            sum("/model/encoder/layer.0/" in name for name in exporter.ONNX_Q8_NODES_TO_EXCLUDE),
            6,
        )

    def test_flexible_coreml_candidate_is_explicitly_additive(self) -> None:
        default = exporter.artifact_plan()
        candidate = exporter.artifact_plan(
            include_flexible_coreml_candidate=True,
        )
        self.assertEqual(candidate[:-1], default)
        self.assertTrue(candidate[-1].candidate)
        self.assertEqual(candidate[-1].sequence_shape, "flexible")

    def test_cli_rejects_generated_files_outside_target(self) -> None:
        with self.assertRaisesRegex(SystemExit, "must be inside"):
            exporter.parse_options(["--output-dir", "/tmp/retrievalkit-model"])

    def test_cli_rejects_empty_and_conflicting_export_plans(self) -> None:
        with self.assertRaisesRegex(SystemExit, "cannot be used together"):
            exporter.parse_options(["--skip-onnx", "--skip-coreml"])
        with self.assertRaisesRegex(SystemExit, "cannot be used with"):
            exporter.parse_options(
                ["--skip-coreml", "--include-flexible-coreml-candidate"]
            )

    def test_main_writes_v1_manifest_and_compatibility_copy(self) -> None:
        with tempfile.TemporaryDirectory(dir=exporter.TARGET_DIR) as directory:
            root = Path(directory)
            output = root / "output"

            def fake_export(options, specs):
                self.make_complete_fixture(options.output_dir)
                self.assertEqual(tuple(specs), exporter.artifact_plan())

            with mock.patch.object(exporter, "export_artifacts", side_effect=fake_export):
                result = exporter.main(
                    [
                        "--output-dir",
                        str(output),
                        "--cache-dir",
                        str(root / "cache"),
                    ]
                )
            self.assertEqual(result, 0)
            self.assertEqual(
                (output / "manifest-v1.json").read_bytes(),
                (output / "manifest.json").read_bytes(),
            )
            self.assertTrue((output / "README.md").is_file())
            self.assertEqual(
                (output / "LICENSE").read_bytes(),
                (exporter.ROOT_DIR / "LICENSE").read_bytes(),
            )
            self.assertEqual(
                (output / "NOTICE").read_bytes(),
                (exporter.ROOT_DIR / "NOTICE").read_bytes(),
            )

    def test_manifest_records_pinned_source_hashes_sizes_and_license(self) -> None:
        with tempfile.TemporaryDirectory(dir=exporter.TARGET_DIR) as directory:
            root = Path(directory)
            specs = self.make_complete_fixture(root, flexible=True)
            manifest_path = root / "manifest-v1.json"
            exporter.write_manifest(root, specs, manifest_path)
            exporter.validate_manifest(manifest_path)
            document = json.loads(manifest_path.read_text())

            self.assertEqual(document["schema_version"], 1)
            self.assertEqual(document["model"]["revision"], exporter.MODEL_REVISION)
            self.assertEqual(document["model"]["maximum_sequence_length"], 256)
            self.assertEqual(document["license"]["spdx_id"], "Apache-2.0")
            self.assertEqual(len(document["tokenizer"]["files"]), 4)
            self.assertTrue(
                all(len(item["sha256"]) == 64 for item in document["artifacts"])
            )
            candidate = document["artifacts"][-1]
            self.assertEqual(
                candidate["sequence_length"],
                {"shape": "flexible", "minimum": 1, "maximum": 256},
            )

    def test_manifest_validation_detects_artifact_tampering(self) -> None:
        with tempfile.TemporaryDirectory(dir=exporter.TARGET_DIR) as directory:
            root = Path(directory)
            specs = self.make_complete_fixture(root)
            manifest_path = root / "manifest-v1.json"
            exporter.write_manifest(root, specs, manifest_path)
            (root / specs[0].relative_path).write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "byte size|SHA-256"):
                exporter.validate_manifest(manifest_path)

    def test_directory_digest_is_independent_of_creation_order(self) -> None:
        with tempfile.TemporaryDirectory(dir=exporter.TARGET_DIR) as first_directory:
            with tempfile.TemporaryDirectory(dir=exporter.TARGET_DIR) as second_directory:
                first = Path(first_directory)
                second = Path(second_directory)
                for root, names in ((first, ("b", "a")), (second, ("a", "b"))):
                    for name in names:
                        (root / name).write_bytes(name.encode())
                self.assertEqual(
                    exporter.path_stats(first),
                    exporter.path_stats(second),
                )

    def test_coreml_manifest_identifiers_are_canonical(self) -> None:
        def package_manifest(model_id: str, weights_id: str):
            return {
                "fileFormatVersion": "1.0.0",
                "itemInfoEntries": {
                    model_id: {"path": "com.apple.CoreML/model.mlmodel"},
                    weights_id: {"path": "com.apple.CoreML/weights"},
                },
                "rootModelIdentifier": model_id,
            }

        first = exporter.canonicalize_coreml_manifest(package_manifest("random-a", "random-b"))
        second = exporter.canonicalize_coreml_manifest(package_manifest("random-c", "random-d"))
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
