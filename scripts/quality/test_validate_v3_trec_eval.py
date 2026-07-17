from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.quality import bootstrap_v3_trec_eval as bootstrap
from scripts.quality import validate_v3_trec_eval as validator
from scripts.quality.validate_v3_phase_1_2a import ValidationError


def rust_run(value: float = 0.5) -> dict[str, object]:
    metrics = {mapping[0]: value for mapping in validator.SUPPORTED_MAPPINGS}
    return {
        "macro": metrics.copy(),
        "queries": [{"metrics": metrics.copy(), "query_id": "q1"}],
    }


def observed(value: float = 0.5) -> dict[tuple[str, str], float]:
    rows: dict[tuple[str, str], float] = {}
    for _, official, _, _ in validator.SUPPORTED_MAPPINGS:
        rows[(official, "q1")] = value
        rows[(official, "all")] = value
    return rows


class OfficialTrecEvalValidatorTests(unittest.TestCase):
    def test_darwin_build_strips_debug_metadata_and_normalizes_uuid(self) -> None:
        with mock.patch.object(bootstrap.sys, "platform", "darwin"):
            with mock.patch.object(
                bootstrap.shutil,
                "which",
                side_effect=lambda tool: f"/usr/bin/{tool}",
            ):
                compiler_arguments, post_link = bootstrap.reproducible_build_settings()
        self.assertEqual(compiler_arguments, [])
        self.assertEqual(post_link["arguments"], ["-S"])
        self.assertEqual(post_link["executable"], "/usr/bin/strip")
        self.assertEqual(
            post_link["macho_uuid_normalization"],
            "sha256_with_lc_uuid_zeroed_first_16_bytes",
        )
        self.assertEqual(
            post_link["codesign"],
            {
                "executable": "/usr/bin/codesign",
                "remove_arguments": ["--remove-signature"],
                "sign_arguments": ["--force", "--sign", "-", "--timestamp=none"],
            },
        )

    def test_macho_uuid_normalization_is_content_derived(self) -> None:
        header = bytearray(32)
        header[:4] = b"\xcf\xfa\xed\xfe"
        header[16:20] = (1).to_bytes(4, "little")
        header[20:24] = (24).to_bytes(4, "little")
        command = (0x1B).to_bytes(4, "little") + (24).to_bytes(4, "little")
        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "first"
            second = Path(directory) / "second"
            first.write_bytes(header + command + (b"a" * 16) + b"payload")
            second.write_bytes(header + command + (b"b" * 16) + b"payload")
            bootstrap.normalize_macho_uuid(first)
            bootstrap.normalize_macho_uuid(second)
            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_successful_supported_metric_comparison(self) -> None:
        queries, aggregates, query_max, aggregate_max = validator.compare_run_metrics(
            "run", rust_run(), ["q1"], observed()
        )
        self.assertEqual(len(queries), len(validator.SUPPORTED_MAPPINGS))
        self.assertEqual(len(aggregates), len(validator.SUPPORTED_MAPPINGS))
        self.assertEqual(query_max, 0.0)
        self.assertEqual(aggregate_max, 0.0)

    def test_per_query_mismatch(self) -> None:
        values = observed()
        values[("ndcg_cut_5", "q1")] = 0.6
        with self.assertRaisesRegex(ValidationError, "q1/ndcg_at_5 differs"):
            validator.compare_run_metrics("run", rust_run(), ["q1"], values)

    def test_aggregate_mismatch(self) -> None:
        values = observed()
        values[("ndcg_cut_5", "all")] = 0.6
        with self.assertRaisesRegex(ValidationError, "ndcg_at_5 aggregate differs"):
            validator.compare_run_metrics("run", rust_run(), ["q1"], values)

    def test_missing_query(self) -> None:
        with self.assertRaisesRegex(ValidationError, "missing rows"):
            validator.parse_trec_eval_output("map all 0.5\n", {"map"}, {"q1"})

    def test_unexpected_query(self) -> None:
        output = "map q1 0.5\nmap q2 0.5\nmap all 0.5\n"
        with self.assertRaisesRegex(ValidationError, "unexpected trec_eval output"):
            validator.parse_trec_eval_output(output, {"map"}, {"q1"})

    def test_missing_run(self) -> None:
        with self.assertRaisesRegex(ValidationError, r"missing \['run-b'\]"):
            validator.validate_run_inventory(["run-a"], ["run-a", "run-b"])

    def test_duplicate_row(self) -> None:
        data = b"q1 Q0 d1 1 10 run-a\nq1 Q0 d1 2 9 run-a\n"
        with self.assertRaisesRegex(ValidationError, "duplicate row"):
            validator.parse_run(data, "run-a")

    def test_unsupported_metric_handling(self) -> None:
        unsupported = {row["metric"] for row in validator.UNSUPPORTED_METRICS}
        self.assertEqual(
            unsupported, {"judged_at_5", "judged_at_10", "graph_and_evidence_metrics"}
        )
        supported = {row[0] for row in validator.SUPPORTED_MAPPINGS}
        self.assertTrue(unsupported.isdisjoint(supported))

    def test_wrong_executable_or_source_checksum(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "source.tar.gz").write_bytes(b"wrong")
            (root / "source").mkdir()
            binary = root / "trec_eval"
            binary.write_bytes(b"binary")
            identity = {
                "archive_sha256": bootstrap.ARCHIVE_SHA256,
                "archive_url": bootstrap.ARCHIVE_URL,
                "compiler": {"executable": "/cc", "version": "cc"},
                "executable_sha256": "0" * 64,
                "source_file_count": 0,
                "source_tree_sha256": bootstrap.SOURCE_TREE_SHA256,
                "upstream_commit": bootstrap.UPSTREAM_COMMIT,
                "upstream_url": bootstrap.UPSTREAM_URL,
                "version": bootstrap.UPSTREAM_VERSION,
            }
            identity_path = root / "identity.json"
            identity_path.write_text(json.dumps(identity), encoding="utf-8")
            with self.assertRaisesRegex(ValidationError, "source archive checksum mismatch"):
                validator.load_dependency_identity(identity_path, binary)

            (root / "source.tar.gz").write_bytes(b"archive")
            with mock.patch.object(validator.bootstrap, "ARCHIVE_SHA256", validator.sha256(b"archive")):
                with mock.patch.object(
                    validator.bootstrap,
                    "source_tree_identity",
                    return_value=(bootstrap.SOURCE_TREE_SHA256, []),
                ):
                    with mock.patch.object(
                        validator.bootstrap, "verify_executable", return_value="1" * 64
                    ):
                        identity["archive_sha256"] = validator.sha256(b"archive")
                        identity_path.write_text(json.dumps(identity), encoding="utf-8")
                        with self.assertRaisesRegex(
                            ValidationError, "executable checksum mismatch"
                        ):
                            validator.load_dependency_identity(identity_path, binary)

    def test_nonzero_trec_eval_exit_status(self) -> None:
        def failing_runner(*_args: object, **_kwargs: object) -> subprocess.CompletedProcess[str]:
            return subprocess.CompletedProcess([], 2, "", "failure")

        with self.assertRaisesRegex(ValidationError, "status 2"):
            validator.run_official(["trec_eval"], failing_runner)


if __name__ == "__main__":
    unittest.main()
