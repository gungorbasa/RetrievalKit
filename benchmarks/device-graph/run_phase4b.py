#!/usr/bin/env python3
"""Fail-closed physical-device collector for the frozen Phase 4b matrix."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import plistlib
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

DEVICES = {
    "iphone17-pro-max": "E342200A-C959-5384-A846-24F4163E5722",
}
SUPPORTED = ("10k-384d-v3", "25k-384d-v3", "50k-384d-v3")
STRESS = "100k-384d-v3-stress"
ENCODINGS = ("f32", "i8")
LIFECYCLE_OPERATIONS = (
    "build",
    "save",
    "read_only_validation",
    "cold_load",
    "warm_load",
    "replay",
)


class CollectorError(RuntimeError):
    pass


@dataclass(frozen=True)
class Product:
    role: str
    app: Path
    bundle_id: str
    executable_sha256: str
    framework_sha256: str


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def atomic_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def parse_app_json(output: str) -> dict[str, Any]:
    decoder = json.JSONDecoder()
    candidates: list[dict[str, Any]] = []
    for offset, character in enumerate(output):
        if character != "{":
            continue
        try:
            value, _ = decoder.raw_decode(output[offset:])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "ok" in value:
            candidates.append(value)
    if not candidates:
        raise CollectorError("device console contained no benchmark JSON object")
    return candidates[-1]


def product(path: Path, framework: Path, expected_role: str) -> Product:
    info_path = path / "Info.plist"
    if not info_path.is_file():
        raise CollectorError(f"missing app Info.plist: {info_path}")
    with info_path.open("rb") as stream:
        info = plistlib.load(stream)
    executable = path / info["CFBundleExecutable"]
    bundle_id = info["CFBundleIdentifier"]
    if not framework.is_file():
        raise CollectorError(f"missing frozen framework binary: {framework}")
    return Product(
        role=expected_role,
        app=path,
        bundle_id=bundle_id,
        executable_sha256=sha256_file(executable),
        framework_sha256=sha256_file(framework),
    )


class Collector:
    def __init__(self, args: argparse.Namespace) -> None:
        self.root = args.artifact_root.resolve()
        self.authorization_path = args.authorization.resolve()
        self.authorization = json.loads(self.authorization_path.read_text(encoding="utf-8"))
        self.authorization_sha256 = sha256_file(self.authorization_path)
        self.base = product(args.base_app.resolve(), args.base_framework.resolve(), "baseline")
        self.graph = product(args.graph_app.resolve(), args.graph_framework.resolve(), "candidate")
        expected = self.authorization.get("products", {})
        for item in (self.base, self.graph):
            registered = expected.get(item.role, {})
            if registered.get("app_executable_sha256") != item.executable_sha256:
                raise CollectorError(f"{item.role} executable differs from authorization")
            if registered.get("framework_binary_sha256") != item.framework_sha256:
                raise CollectorError(f"{item.role} framework differs from authorization")

    def install(self) -> None:
        for device in DEVICES.values():
            for item in (self.base, self.graph):
                subprocess.run(
                    ["xcrun", "devicectl", "device", "install", "app", "--quiet",
                     "--device", device, str(item.app)],
                    check=True,
                )

    def launch(
        self,
        device_role: str,
        item: Product,
        arguments: list[str],
        destination: Path,
        scenario_id: str,
        timeout: int = 3_600,
    ) -> dict[str, Any]:
        device = DEVICES[device_role]
        command = [
            "xcrun", "devicectl", "device", "process", "launch", "--quiet", "--console",
            "--terminate-existing", "--timeout", str(timeout), "--device", device,
            item.bundle_id, *arguments,
        ]
        started = dt.datetime.now(dt.timezone.utc)
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
        finished = dt.datetime.now(dt.timezone.utc)
        try:
            response = parse_app_json(completed.stdout)
        except CollectorError as error:
            response = {"ok": False, "error": str(error)}
        response.update({
            "collector_schema_version": 1,
            "scenario_id": scenario_id,
            "host_device_identifier": device,
            "authorization_sha256": self.authorization_sha256,
            "app_executable_sha256": item.executable_sha256,
            "framework_binary_sha256": item.framework_sha256,
            "product_role": item.role,
            "started_at_utc": started.isoformat(),
            "finished_at_utc": finished.isoformat(),
            "collector_exit_code": completed.returncode,
            "atomic_write_completed": True,
        })
        if completed.returncode != 0:
            response["ok"] = False
            response["collector_error"] = completed.stderr.strip()[-4_000:]
        atomic_json(destination, response)
        if response.get("ok") is not True:
            rejected = self.root / "rejected" / device_role / destination.relative_to(self.root)
            atomic_json(rejected, response)
            raise CollectorError(f"{scenario_id} failed; evidence preserved at {destination}")
        return response

    def query_matrix(self, device_role: str, stress: bool = False) -> None:
        workloads = (STRESS,) if stress else SUPPORTED
        lane = "stress" if stress else "supported"
        for workload in workloads:
            for encoding in ENCODINGS:
                common = [
                    "--phase4-query-session", "--physical-device-required",
                    "--phase4-device-role", device_role, "--phase4-workload", workload,
                    "--phase4-encoding", encoding,
                ]
                if stress:
                    common.append("--phase4-100k-preflight-safe")
                    preflight = (
                        self.root / "devices" / device_role / lane / workload / encoding /
                        "preflight.json"
                    )
                    self.launch(
                        device_role, self.graph,
                        ["--phase4-graph-preflight", "--physical-device-required",
                         "--phase4-device-role", device_role, "--phase4-workload", workload,
                         "--phase4-encoding", encoding, "--phase4-100k-preflight-safe"],
                        preflight, f"{device_role}/{workload}/{encoding}/preflight",
                    )
                for session in range(5):
                    session_id = f"query-{session:02}"
                    destination = (
                        self.root / "devices" / device_role / lane / workload / encoding /
                        "query" / f"session-{session:02}.json"
                    )
                    self.launch(
                        device_role, self.graph, common + ["--phase4-session", session_id],
                        destination, f"{device_role}/{lane}/{workload}/{encoding}/{session_id}",
                    )

    def lifecycle_matrix(self, device_role: str, stress: bool = False) -> None:
        workloads = (STRESS,) if stress else SUPPORTED
        lane = "stress" if stress else "supported"
        for workload in workloads:
            for encoding in ENCODINGS:
                common = [
                    "--phase4-lifecycle-sample", "--physical-device-required",
                    "--phase4-device-role", device_role, "--phase4-workload", workload,
                    "--phase4-encoding", encoding,
                ]
                if stress:
                    common.append("--phase4-100k-preflight-safe")
                prepare = self.root / "devices" / device_role / lane / workload / encoding / "lifecycle"
                self.launch(
                    device_role, self.graph,
                    common + ["--phase4-operation", "prepare", "--phase4-sample", "prepare"],
                    prepare / "prepare.json", f"{device_role}/{workload}/{encoding}/prepare",
                )
                for operation in LIFECYCLE_OPERATIONS:
                    warmup_count = 0 if operation == "cold_load" else 3
                    for sample in range(warmup_count):
                        sample_id = f"{operation}-warmup-{sample:02}"
                        self.launch(
                            device_role, self.graph,
                            common + ["--phase4-operation", operation, "--phase4-sample", sample_id],
                            prepare / operation / f"warmup-{sample:02}.json",
                            f"{device_role}/{workload}/{encoding}/{sample_id}",
                        )
                    for sample in range(20):
                        sample_id = f"{operation}-sample-{sample:02}"
                        self.launch(
                            device_role, self.graph,
                            common + ["--phase4-operation", operation, "--phase4-sample", sample_id],
                            prepare / operation / f"sample-{sample:02}.json",
                            f"{device_role}/{workload}/{encoding}/{sample_id}",
                        )

    def graph_free(self, device_role: str) -> None:
        for encoding in ENCODINGS:
            for session in range(3):
                for item in (self.base, self.graph):
                    session_id = f"{item.role}-{session:02}"
                    destination = (
                        self.root / "devices" / device_role / "graph-free" / encoding /
                        item.role / f"session-{session:02}.json"
                    )
                    self.launch(
                        device_role, item,
                        ["--phase4-graph-free-regression", "--physical-device-required",
                         "--phase4-device-role", device_role, "--phase4-encoding", encoding,
                         "--phase4-product", item.role, "--phase4-session", session_id],
                        destination, f"{device_role}/graph-free/{encoding}/{session_id}",
                    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("install", "supported", "graph-free", "stress", "all"))
    parser.add_argument("--device-role", choices=tuple(DEVICES))
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--authorization", type=Path, required=True)
    parser.add_argument("--base-app", type=Path, required=True)
    parser.add_argument("--graph-app", type=Path, required=True)
    parser.add_argument("--base-framework", type=Path, required=True)
    parser.add_argument("--graph-framework", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        collector = Collector(args)
        if args.action == "install":
            collector.install()
            return 0
        if args.device_role is None:
            raise CollectorError("--device-role is required for measurement actions")
        if args.action in ("supported", "all"):
            collector.query_matrix(args.device_role)
            collector.lifecycle_matrix(args.device_role)
        if args.action in ("graph-free", "all"):
            collector.graph_free(args.device_role)
        if args.action in ("stress", "all"):
            if args.device_role != "iphone17-pro-max":
                raise CollectorError("100K stress is iPhone-17-only")
            collector.query_matrix(args.device_role, stress=True)
            collector.lifecycle_matrix(args.device_role, stress=True)
    except (CollectorError, OSError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print(json.dumps({"ok": False, "error": str(error)}, sort_keys=True), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
