#!/usr/bin/env python3
"""Run and collect the frozen physical-iPhone Apple end-to-end matrix."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BENCHMARK = ROOT / "benchmarks/apple-end-to-end"
BUNDLE_ID = "dev.retrievalkit.RetrievalKitAppleE2EIOSBench"
PROFILES = {
    "coreml-fp32-production-v1": {
        "classification": "production_control",
        "asset_slug": "fp32",
    },
    "coreml-weight-only-q8-experimental-v1": {
        "classification": "experimental_candidate",
        "asset_slug": "q8",
    },
}


def run(command: list[str], *, timeout: int | None = None) -> None:
    print(" ".join(command), flush=True)
    subprocess.run(command, check=True, timeout=timeout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--device", required=True, help="CoreDevice UUID, UDID, or device name")
    parser.add_argument("--attempt-id", required=True)
    parser.add_argument("--retrievalkit-revision", required=True)
    parser.add_argument("--assets", type=Path, default=ROOT / "target/apple-end-to-end")
    parser.add_argument("--bundle-id", default=BUNDLE_ID)
    parser.add_argument("--device-assets", default="BenchAssets")
    parser.add_argument("--launch-timeout-seconds", type=int, default=900)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--inter-session-cooldown-seconds", type=int, default=30)
    args = parser.parse_args()

    assets = args.assets.resolve()
    output_root = assets / "results/iphone" / args.attempt_id
    output_root.mkdir(parents=True, exist_ok=args.resume)
    quality = json.loads((assets / "quality/q8-vs-fp32-provider-v1.json").read_text())
    if quality.get("passed") is not True:
        raise SystemExit("Q8 provider prerequisite has not passed")
    descriptor = json.loads((BENCHMARK / "workloads-v2.json").read_text())

    reports: list[Path] = []
    ran_session = False
    for workload in descriptor["workloads"]:
        size = f"{workload['active_chunks'] // 1000}k"
        for profile_id, profile in PROFILES.items():
            asset_root = f"{args.device_assets}/{profile['asset_slug']}-{size}"
            for mode in ("vector", "weighted_hybrid"):
                for session in range(1, 4):
                    session_id = f"iphone-{profile['asset_slug']}-{size}-{mode}-{session}"
                    remote_report = f"BenchResults/{args.attempt_id}/{session_id}.json"
                    local_report = (
                        output_root / profile["asset_slug"] / size / mode / f"session-{session}.json"
                    )
                    local_report.parent.mkdir(parents=True, exist_ok=True)
                    if local_report.exists():
                        if not args.resume:
                            raise SystemExit(f"report already exists: {local_report}")
                        print(f"resuming past {session_id}", flush=True)
                        reports.append(local_report)
                        continue
                    if ran_session and args.inter_session_cooldown_seconds:
                        print(
                            f"cooling for {args.inter_session_cooldown_seconds}s before {session_id}",
                            flush=True,
                        )
                        time.sleep(args.inter_session_cooldown_seconds)
                    launch = [
                        "xcrun", "devicectl", "device", "process", "launch",
                        "--device", args.device,
                        "--console", "--terminate-existing",
                        "--timeout", str(args.launch_timeout_seconds),
                        args.bundle_id,
                        "--asset-root", asset_root,
                        "--contract-version", "apple-end-to-end-v2",
                        "--output", remote_report,
                        "--workload-id", workload["id"],
                        "--workload-classification", workload["classification"],
                        "--profile-id", profile_id,
                        "--profile-classification", profile["classification"],
                        "--session-id", session_id,
                        "--mode", mode,
                        "--retrievalkit-revision", args.retrievalkit_revision,
                        "--network-disabled", "true",
                    ]
                    print(f"running {session_id}", flush=True)
                    run(launch, timeout=args.launch_timeout_seconds + 30)
                    receive = [
                        "xcrun", "devicectl", "device", "copy", "from",
                        "--device", args.device,
                        "--domain-type", "appDataContainer",
                        "--domain-identifier", args.bundle_id,
                        "--source", f"Documents/{remote_report}",
                        "--destination", str(local_report),
                        "--timeout", "120",
                    ]
                    run(receive, timeout=150)
                    reports.append(local_report)
                    ran_session = True

    validation_path = output_root / "validation.json"
    validation = [
        "python3", str(BENCHMARK / "validate_results.py"),
        "--queries", str(assets / "source-10k-a/queries.json"),
        "--descriptor-version", "v2",
        "--require-complete-sessions",
        "--q8-quality", str(assets / "quality/q8-vs-fp32-provider-v1.json"),
        *map(str, reports),
    ]
    result = subprocess.run(validation, check=True, text=True, capture_output=True)
    validation_path.write_text(result.stdout, encoding="utf-8")
    print(result.stdout, end="")
    print(f"complete: {output_root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
