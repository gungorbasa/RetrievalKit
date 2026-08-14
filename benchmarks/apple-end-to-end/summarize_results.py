#!/usr/bin/env python3
"""Render median-session P95 stage summaries from validated Apple reports."""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any


STAGES = (
    ("embedding", "embedding_total"),
    ("retrieval", "retrieval_total"),
    ("total", "end_to_end_text_search"),
)


def median_session_p95(items: list[dict[str, Any]], stage: str) -> int:
    return int(statistics.median(item["summaries"][stage]["p95_ns"] for item in items))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--json-output", type=Path)
    args = parser.parse_args()

    groups: dict[tuple[str, str, str, str], list[dict[str, Any]]] = defaultdict(list)
    for path in args.reports:
        report = json.loads(path.read_text(encoding="utf-8"))
        key = (
            report["environment"]["platform"],
            report["profile_id"],
            report["workload_id"],
            report["search_mode"],
        )
        groups[key].append(report)

    rows: list[dict[str, Any]] = []
    for key, items in sorted(groups.items()):
        if len(items) != 3:
            raise SystemExit(f"configuration {key} has {len(items)} sessions; expected exactly 3")
        row: dict[str, Any] = {
            "platform": key[0],
            "profile_id": key[1],
            "workload_id": key[2],
            "search_mode": key[3],
            "session_count": len(items),
        }
        for label, stage in STAGES:
            value = median_session_p95(items, stage)
            row[f"{label}_median_session_p95_ns"] = value
            row[f"{label}_median_session_p95_ms"] = round(value / 1_000_000, 3)
        rows.append(row)

    document = {"aggregation": "median of three fresh-session P95 values", "rows": rows}
    if args.json_output:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")

    print("| Platform | Profile | Workload | Mode | Embed P95 | Retrieval P95 | Total P95 |")
    print("|---|---|---:|---|---:|---:|---:|")
    for row in rows:
        profile = "FP32" if row["profile_id"] == "coreml-fp32-production-v1" else "Q8 experimental"
        workload = row["workload_id"].split("-")[2].upper()
        print(
            f"| {row['platform']} | {profile} | {workload} | {row['search_mode']} | "
            f"{row['embedding_median_session_p95_ms']:.3f} ms | "
            f"{row['retrieval_median_session_p95_ms']:.3f} ms | "
            f"{row['total_median_session_p95_ms']:.3f} ms |"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
