#!/usr/bin/env python3
"""Measure clean-source wrapper time-to-first-success for release qualification.

This is an onboarding measurement, not a runtime performance benchmark. Each
wrapper runs from an independent ``git archive`` export so repository build
outputs cannot leak between wrappers. External dependency caches are observed
and reported, but deliberately neither cleared nor pre-warmed by this script.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shlex
import subprocess
import sys
import tarfile
import tempfile
import time
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Sequence

SCHEMA_VERSION = 1
WRAPPERS = ("python", "node", "kotlin")


@dataclass(frozen=True)
class Phase:
    name: str
    command: tuple[str, ...]
    cwd: str
    expected_output: str | None = None
    env: tuple[tuple[str, str], ...] = ()


@dataclass
class PhaseResult:
    name: str
    command: list[str]
    duration_seconds: float
    status: str
    output_tail: str
    error: str | None = None


def run_capture(
    command: Sequence[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=check,
    )


def version_output(command: Sequence[str], *, cwd: Path) -> str | None:
    try:
        completed = run_capture(command, cwd=cwd)
    except (OSError, subprocess.CalledProcessError):
        return None
    output = completed.stdout.strip()
    return output if output else None


def cache_observations() -> dict[str, dict[str, Any]]:
    home = Path.home()
    defaults = {
        "cargo": Path(os.environ.get("CARGO_HOME", home / ".cargo")),
        "gradle": Path(os.environ.get("GRADLE_USER_HOME", home / ".gradle")),
        "npm": Path(os.environ.get("npm_config_cache", home / ".npm")),
        "pip": Path(os.environ.get("PIP_CACHE_DIR", home / ".cache" / "pip")),
    }
    return {
        name: {
            "configured_by_environment": (
                (name == "cargo" and "CARGO_HOME" in os.environ)
                or (name == "gradle" and "GRADLE_USER_HOME" in os.environ)
                or (name == "npm" and "npm_config_cache" in os.environ)
                or (name == "pip" and "PIP_CACHE_DIR" in os.environ)
            ),
            "existed_before_measurement": path.exists(),
        }
        for name, path in defaults.items()
    }


def machine_metadata(repo: Path, python_bin: str) -> dict[str, Any]:
    cpu_model = None
    if sys.platform == "darwin":
        cpu_model = version_output(
            ["sysctl", "-n", "machdep.cpu.brand_string"], cwd=repo
        )
    elif Path("/proc/cpuinfo").is_file():
        for line in Path("/proc/cpuinfo").read_text(errors="replace").splitlines():
            if line.lower().startswith("model name"):
                cpu_model = line.partition(":")[2].strip()
                break

    return {
        "os": platform.platform(),
        "system": platform.system(),
        "release": platform.release(),
        "architecture": platform.machine(),
        "cpu_model": cpu_model,
        "logical_cpu_count": os.cpu_count(),
        "ci": os.environ.get("CI", "").lower() == "true",
        "github_actions": os.environ.get("GITHUB_ACTIONS", "").lower() == "true",
        "toolchains": {
            "python": version_output([python_bin, "--version"], cwd=repo),
            "rustc": version_output(["rustc", "--version"], cwd=repo),
            "cargo": version_output(["cargo", "--version"], cwd=repo),
            "node": version_output(["node", "--version"], cwd=repo),
            "npm": version_output(["npm", "--version"], cwd=repo),
            "java": version_output(["java", "-version"], cwd=repo),
        },
    }


def source_revision(repo: Path) -> str:
    return run_capture(["git", "rev-parse", "HEAD"], cwd=repo).stdout.strip()


def working_tree_clean(repo: Path) -> bool:
    return not run_capture(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=repo
    ).stdout.strip()


def export_source(repo: Path, destination: Path) -> None:
    archive = destination.parent / f"{destination.name}.tar"
    subprocess.run(
        ["git", "archive", "--format=tar", f"--output={archive}", "HEAD"],
        cwd=repo,
        check=True,
    )
    destination.mkdir(parents=True)
    with tarfile.open(archive) as source:
        source.extractall(destination, filter="data")


def wrapper_phases(wrapper: str, python_bin: str) -> tuple[Phase, ...]:
    if wrapper == "python":
        return (
            Phase(
                "create-environment",
                (python_bin, "-m", "venv", ".tthw-venv"),
                ".",
            ),
            Phase(
                "install-build-frontend",
                (
                    ".tthw-venv/bin/python",
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "maturin==1.14.1",
                ),
                ".",
            ),
            Phase(
                "build-and-install",
                (
                    "../../.tthw-venv/bin/python",
                    "-m",
                    "maturin",
                    "develop",
                    "--locked",
                    "--release",
                ),
                "wrappers/python",
                env=(("VIRTUAL_ENV", "../../.tthw-venv"),),
            ),
            Phase(
                "first-result",
                (
                    "../../.tthw-venv/bin/python",
                    "examples/database_quickstart.py",
                ),
                "wrappers/python",
                expected_output="hybrid=decision-swift",
            ),
        )
    if wrapper == "node":
        return (
            Phase("install-dependencies", ("npm", "ci"), "wrappers/typescript"),
            Phase(
                "build-native",
                (
                    "npm",
                    "run",
                    "build:native",
                    "--workspace",
                    "retrievalkit-node-local",
                ),
                "wrappers/typescript",
            ),
            Phase(
                "build-package",
                (
                    "npm",
                    "run",
                    "build",
                    "--workspace",
                    "retrievalkit-node-local",
                ),
                "wrappers/typescript",
            ),
            Phase(
                "first-result",
                ("node", "base/examples/retrieval.mjs"),
                "wrappers/typescript",
                expected_output="documentId: 'two'",
            ),
        )
    if wrapper == "kotlin":
        return (
            Phase(
                "build-native",
                ("./scripts/build-native.sh", "jvm"),
                "wrappers/kotlin",
            ),
            Phase(
                "first-result",
                (
                    "./gradlew",
                    "--no-daemon",
                    "--console=plain",
                    ":example-retrieval:run",
                ),
                "wrappers/kotlin",
                expected_output="kotlin: Kotlin calls the local Rust retrieval core",
            ),
        )
    raise ValueError(f"unsupported wrapper: {wrapper}")


def run_phase(root: Path, phase: Phase) -> PhaseResult:
    command_display = shlex.join(phase.command)
    print(f"::group::{phase.name}: {command_display}", flush=True)
    started = time.monotonic()
    phase_env = os.environ.copy()
    phase_env.update(dict(phase.env))
    try:
        completed = run_capture(
            phase.command,
            cwd=root / phase.cwd,
            env=phase_env,
        )
        output = completed.stdout
        if phase.expected_output and phase.expected_output not in output:
            status = "failed"
            error = f"expected output marker not found: {phase.expected_output!r}"
        else:
            status = "passed"
            error = None
    except subprocess.CalledProcessError as failure:
        output = failure.stdout or ""
        status = "failed"
        error = f"command exited with status {failure.returncode}"
    except (OSError, RuntimeError) as failure:
        output = getattr(failure, "stdout", "") or ""
        status = "failed"
        error = str(failure)
    duration = round(time.monotonic() - started, 3)
    if output:
        print(output, end="" if output.endswith("\n") else "\n")
    print("::endgroup::", flush=True)
    return PhaseResult(
        name=phase.name,
        command=list(phase.command),
        duration_seconds=duration,
        status=status,
        output_tail=output[-4000:],
        error=error,
    )


def measure_wrapper(
    wrapper: str,
    *,
    export_root: Path,
    python_bin: str,
) -> dict[str, Any]:
    wrapper_root = export_root / wrapper
    export_source(Path.cwd(), wrapper_root)
    phases: list[PhaseResult] = []
    started = time.monotonic()
    for phase in wrapper_phases(wrapper, python_bin):
        result = run_phase(wrapper_root, phase)
        phases.append(result)
        if result.status != "passed":
            break
    total = round(time.monotonic() - started, 3)
    status = (
        "passed"
        if len(phases) == len(wrapper_phases(wrapper, python_bin))
        else "failed"
    )
    if any(phase.status != "passed" for phase in phases):
        status = "failed"
    return {
        "wrapper": wrapper,
        "status": status,
        "duration_seconds": total,
        "phases": [asdict(phase) for phase in phases],
    }


def build_report(
    *,
    repo: Path,
    python_bin: str,
    selected_wrappers: Sequence[str],
    results: list[dict[str, Any]],
    caches_before: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    clean = working_tree_clean(repo)
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "wrapper-onboarding-tthw",
        "recorded_at": datetime.now(UTC).isoformat(),
        "source": {
            "revision": source_revision(repo),
            "working_tree_clean": clean,
            "isolation": "independent git archive export per wrapper",
        },
        "measurement": {
            "definition": (
                "Wall-clock time from wrapper-specific environment/dependency setup "
                "inside a clean source export through the first successful quickstart result."
            ),
            "excluded": [
                "toolchain provisioning before the harness starts",
                "git checkout and source export",
                "runtime retrieval performance",
            ],
            "cache_policy": (
                "Repository build outputs are isolated per wrapper. Global dependency "
                "and toolchain caches are neither cleared nor pre-warmed by the harness; "
                "their pre-run presence is recorded below."
            ),
            "caveats": [
                "Uncached pip, npm, Cargo, or Gradle dependencies may require network access.",
                "Shared-runner load and dependency-service latency can change wall-clock time.",
                "Compare results only when machine, toolchain, and cache observations are equivalent.",
            ],
            "selected_wrappers": list(selected_wrappers),
        },
        "machine": machine_metadata(repo, python_bin),
        "caches": caches_before,
        "results": results,
        "status": (
            "passed"
            if results and all(result["status"] == "passed" for result in results)
            else "failed"
        ),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Measure clean-source Python, Node, and Kotlin onboarding time. "
            "This is not a runtime performance benchmark."
        )
    )
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--wrapper",
        action="append",
        choices=WRAPPERS,
        dest="wrappers",
        help="wrapper to measure; repeat to select multiple (default: all)",
    )
    parser.add_argument("--python-bin", default=sys.executable)
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="measure committed HEAD while ignoring working-tree changes",
    )
    parser.add_argument(
        "--print-plan",
        action="store_true",
        help="print the fixed phase plan as JSON without running it",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo = args.repo.resolve()
    selected = tuple(args.wrappers or WRAPPERS)
    if args.print_plan:
        print(
            json.dumps(
                {
                    wrapper: [
                        asdict(phase)
                        for phase in wrapper_phases(wrapper, args.python_bin)
                    ]
                    for wrapper in selected
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if not working_tree_clean(repo) and not args.allow_dirty:
        print(
            "working tree is not clean; commit the release candidate or pass "
            "--allow-dirty to measure committed HEAD only",
            file=sys.stderr,
        )
        return 2

    original_cwd = Path.cwd()
    results: list[dict[str, Any]] = []
    caches_before = cache_observations()
    try:
        os.chdir(repo)
        with tempfile.TemporaryDirectory(prefix="retrievalkit-tthw-") as temporary:
            export_root = Path(temporary)
            for wrapper in selected:
                print(
                    f"Measuring {wrapper} onboarding from clean source...", flush=True
                )
                results.append(
                    measure_wrapper(
                        wrapper,
                        export_root=export_root,
                        python_bin=args.python_bin,
                    )
                )
    finally:
        os.chdir(original_cwd)

    report = build_report(
        repo=repo,
        python_bin=args.python_bin,
        selected_wrappers=selected,
        results=results,
        caches_before=caches_before,
    )
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"Wrote wrapper onboarding evidence to {output}")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
