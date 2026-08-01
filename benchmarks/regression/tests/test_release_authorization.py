from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VALIDATOR = ROOT / "benchmarks/regression/validate_release_authorization.py"


class ReleaseAuthorizationTests(unittest.TestCase):
    def run_validator(self, observation: dict[str, object]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            observation_path = root / "observation.json"
            observation_path.write_text(
                json.dumps(observation, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            authorization = {
                "artifact_type": "phase7_release_qualification_authorization",
                "authorized": True,
                "authorized_encodings": ["f32", "i8"],
                "authorized_workloads": [
                    "10k-384d-v3",
                    "25k-384d-v3",
                    "50k-384d-v3",
                ],
                "device_commands_authorized": False,
                "evidence_only": True,
                "expires_on": "2099-01-01",
                "observation_sha256": hashlib.sha256(
                    observation_path.read_bytes()
                ).hexdigest(),
                "owner": "release owner",
                "schema_version": 1,
            }
            authorization_path = root / "authorization.json"
            authorization_path.write_text(
                json.dumps(authorization, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            return subprocess.run(
                [
                    "python3",
                    str(VALIDATOR),
                    "--authorization",
                    str(authorization_path),
                    "--observation",
                    str(observation_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )

    def observation(self) -> dict[str, object]:
        return {
            "metrics": {"physical_device_100k_violation_count": 0},
            "platform": {
                "device_identifier": "iPhone18,2",
                "os": "iOS 26.5.1/26.5.2",
                "sample_count": "frozen supported matrix",
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "toolchain": "frozen release toolchain",
            },
        }

    def test_required_zero_violation_metric_is_not_mistaken_for_evidence(self) -> None:
        result = self.run_validator(self.observation())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_excluded_physical_device_evidence_remains_rejected(self) -> None:
        observation = self.observation()
        observation["proposed_workload"] = "100k physical-device support"
        result = self.run_validator(observation)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("permanently excluded", result.stderr)

    def test_nonzero_excluded_lane_violation_remains_rejected(self) -> None:
        observation = self.observation()
        metrics = observation["metrics"]
        assert isinstance(metrics, dict)
        metrics["physical_device_100k_violation_count"] = 1
        result = self.run_validator(observation)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("violation count must be zero", result.stderr)


if __name__ == "__main__":
    unittest.main()
