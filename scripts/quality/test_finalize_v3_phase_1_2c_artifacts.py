from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.quality import finalize_v3_phase_1_2c_artifacts as finalizer


class Phase12cArtifactFinalizerTests(unittest.TestCase):
    def make_inventory(self, root: Path) -> None:
        for relative in finalizer.EXPECTED_FILES:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"{relative}\n".encode())

    def test_accepts_the_exact_56_file_inventory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_inventory(root)
            index = finalizer.build_index(root)
            self.assertEqual(index["file_count"], 56)
            self.assertEqual(
                {entry["path"] for entry in index["files"]},
                set(finalizer.EXPECTED_FILES),
            )

    def test_rejects_a_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_inventory(root)
            (root / "qrels.tsv").unlink()
            with self.assertRaisesRegex(finalizer.ValidationError, "missing.*qrels.tsv"):
                finalizer.build_index(root)

    def test_rejects_an_unexpected_or_incorrectly_named_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_inventory(root)
            (root / "runs" / "duplicate-logical-run.trec").write_text("duplicate\n")
            with self.assertRaisesRegex(finalizer.ValidationError, "unexpected.*duplicate"):
                finalizer.build_index(root)

    def test_check_only_detects_modified_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_inventory(root)
            finalizer.write_index(root)
            (root / "qrels.tsv").write_bytes(b"modified\n")
            with self.assertRaisesRegex(finalizer.ValidationError, "does not match"):
                finalizer.check_index(root)

    def test_check_only_accepts_a_freshly_rebuilt_stored_index(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_inventory(root)
            written = finalizer.write_index(root)
            self.assertEqual(finalizer.check_index(root), written)


if __name__ == "__main__":
    unittest.main()
