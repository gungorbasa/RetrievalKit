from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).parents[1] / "authorization_lineage.py"
SPEC = importlib.util.spec_from_file_location("phase4_authorization_lineage", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
lineage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(lineage)


class AuthorizationLineageTests(unittest.TestCase):
    def test_preserved_set_is_closed_and_byte_sensitive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = root / (
                "devices/iphone17-pro-max/supported/10k-384d-v3/f32/"
                "lifecycle/prepare.json"
            )
            preserved.parent.mkdir(parents=True)
            preserved.write_text("{\"authorization\":\"v3\"}\n", encoding="utf-8")
            unfinished = root / (
                "devices/iphone17-pro-max/supported/10k-384d-v3/f32/"
                "lifecycle/read_only_validation/warmup-00.json"
            )
            unfinished.parent.mkdir(parents=True)
            unfinished.write_text("{\"authorization\":\"v4\"}\n", encoding="utf-8")

            entries = lineage.preserved_artifact_entries(root)
            self.assertEqual([item["path"] for item in entries], [preserved.relative_to(root).as_posix()])
            authorization = {
                "evidence_lineage": {
                    "prior_authorization_sha256": "3" * 64,
                    "preserved_path_patterns": list(lineage.PRESERVED_V3_PATH_PATTERNS),
                    "preserved_artifact_count": 1,
                    "preserved_artifact_set_sha256": lineage.artifact_set_sha256(entries),
                    "prior_allowed_os_builds": ["23F81", "23F84"],
                    "preserve_prior_artifact_bytes": True,
                    "current_authorization_covers_unmatched_required_paths": True,
                }
            }
            lineage.validate_lineage(authorization, "3" * 64, root)

            preserved.write_text("{\"authorization\":\"tampered\"}\n", encoding="utf-8")
            with self.assertRaises(lineage.LineageError):
                lineage.validate_lineage(authorization, "3" * 64, root)


if __name__ == "__main__":
    unittest.main()
