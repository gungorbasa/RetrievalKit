from __future__ import annotations

import importlib.util
import sys
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
        kotlin_commands = [
            item
            for phase in MODULE.wrapper_phases("kotlin", "python3")
            for item in phase.command
        ]
        self.assertIn("--locked", python_commands)
        self.assertIn("ci", node_commands)
        self.assertIn("--no-daemon", kotlin_commands)

    def test_unknown_wrapper_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "unsupported wrapper"):
            MODULE.wrapper_phases("swift", "python3")


if __name__ == "__main__":
    unittest.main()
