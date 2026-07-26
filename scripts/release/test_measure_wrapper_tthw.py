from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("measure_wrapper_tthw.py")
SPEC = importlib.util.spec_from_file_location("measure_wrapper_tthw", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class WrapperTthwPlanTests(unittest.TestCase):
    def test_each_wrapper_ends_with_a_checked_first_result(self) -> None:
        for wrapper in MODULE.WRAPPERS:
            with self.subTest(wrapper=wrapper):
                phases = MODULE.wrapper_phases(wrapper, "python3")
                self.assertEqual(phases[-1].name, "first-result")
                self.assertIsNotNone(phases[-1].expected_output)

    def test_dependency_and_runner_commands_are_fixed(self) -> None:
        python_commands = [
            item
            for phase in MODULE.wrapper_phases("python", "python3")
            for item in phase.command
        ]
        node_commands = [
            item
            for phase in MODULE.wrapper_phases("node", "python3")
            for item in phase.command
        ]
        swift_commands = [
            item
            for phase in MODULE.wrapper_phases("swift", "python3")
            for item in phase.command
        ]
        kotlin_commands = [
            item
            for phase in MODULE.wrapper_phases("kotlin", "python3")
            for item in phase.command
        ]
        self.assertIn("--locked", python_commands)
        self.assertIn("ci", node_commands)
        self.assertIn("--graph", swift_commands)
        self.assertIn("graph-retrieval", swift_commands)
        self.assertIn("--no-daemon", kotlin_commands)

    def test_unknown_wrapper_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "unsupported wrapper"):
            MODULE.wrapper_phases("browser", "python3")

    def test_result_schema_declares_phase_success_evidence(self) -> None:
        self.assertEqual(MODULE.SCHEMA_VERSION, 2)
        self.assertIn(
            "expected_output",
            MODULE.RESULT_SCHEMA["phase_required_fields"],
        )
        self.assertEqual(
            MODULE.RESULT_SCHEMA["status_values"],
            ["passed", "failed"],
        )

    def test_swift_entrypoint_reports_exact_missing_artifact_recovery(self) -> None:
        script = SCRIPT.parents[1] / "run-swift-quickstart.sh"
        with tempfile.TemporaryDirectory() as directory:
            environment = os.environ.copy()
            environment["RETRIEVALKIT_APPLE_ARTIFACT_DIR"] = directory
            completed = subprocess.run(
                [script, "graph-retrieval"],
                cwd=SCRIPT.parents[2],
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        self.assertEqual(completed.returncode, 1)
        self.assertIn(
            "scripts/build-xcframework.sh --macos-only --graph",
            completed.stdout,
        )
        self.assertIn(
            "scripts/run-swift-quickstart.sh graph-retrieval",
            completed.stdout,
        )


if __name__ == "__main__":
    unittest.main()
