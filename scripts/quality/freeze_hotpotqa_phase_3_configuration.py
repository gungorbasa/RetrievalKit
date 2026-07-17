#!/usr/bin/env python3
"""Replay HotpotQA Phase 3 development selection and verify the immutable lock."""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SEARCH_SPACE = (
    ROOT
    / "benchmarks/retrieval-quality/hotpotqa/phase-3-development-search-space.json"
)
DEFAULT_TUNING_ROOT = ROOT / "target/benchmarks/hotpotqa-phase-3a/tuning"
DEFAULT_LOCK = (
    ROOT
    / "benchmarks/retrieval-quality/hotpotqa/phase-3-selected-configuration.json"
)
SEARCH_SPACE_SHA256 = (
    "30a93141c0b36d446617342ae846ff4174ff1f8b0f0f9cf008882ed6f3cbdeca"
)
ADAPTER_MANIFEST_SHA256 = (
    "8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6"
)
DEVELOPMENT_POPULATION_SHA256 = (
    "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f"
)
DEVELOPMENT_COLLECTION_SHA256 = (
    "4ec8a04401149b04718f28b465809bd788a170c1089df5fe5e68e1ca991d633d"
)
OBJECTIVE = (
    "complete_evidence_recall_at_10",
    "ndcg_at_10",
    "map",
    "recall_at_10",
    "mrr_at_10",
)


class LockError(RuntimeError):
    pass


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()
        + b"\n"
    )


def canonical_preimage(value: Any) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json(path: Path) -> Any:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise LockError(f"read '{path}': {error}") from error
    try:
        value = json.loads(data)
    except json.JSONDecodeError as error:
        raise LockError(f"parse '{path}': {error}") from error
    if data != canonical_bytes(value):
        raise LockError(f"'{path}' is not compact canonical JSON plus LF")
    return value


def verify_inventory(root: Path, manifest: dict[str, Any]) -> None:
    expected = {
        row["path"]: (row["bytes"], row["sha256"]) for row in manifest["files"]
    }
    actual: dict[str, tuple[int, str]] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path == root / "manifest.json":
            continue
        relative = path.relative_to(root).as_posix()
        data = path.read_bytes()
        actual[relative] = (len(data), sha256(data))
    if expected != actual:
        raise LockError(f"artifact inventory mismatch beneath '{root}'")


def candidate_sort_key(row: dict[str, Any]) -> tuple[Any, ...]:
    candidate = row["candidate"]
    vector = candidate["vector_candidate_limit"]
    keyword = candidate["keyword_candidate_limit"]
    return (
        *(-row["aggregate"][name] for name in OBJECTIVE),
        vector + keyword,
        max(vector, keyword),
        canonical_preimage(candidate),
    )


def first_decisive_trace(
    winner: dict[str, Any], runner_up: dict[str, Any]
) -> dict[str, Any]:
    criteria = [
        (name, "descending", winner["aggregate"][name], runner_up["aggregate"][name])
        for name in OBJECTIVE
    ]
    winner_candidate = winner["candidate"]
    runner_candidate = runner_up["candidate"]
    criteria.extend(
        [
            (
                "total_candidate_count",
                "ascending",
                winner_candidate["vector_candidate_limit"]
                + winner_candidate["keyword_candidate_limit"],
                runner_candidate["vector_candidate_limit"]
                + runner_candidate["keyword_candidate_limit"],
            ),
            (
                "maximum_component_candidate_count",
                "ascending",
                max(
                    winner_candidate["vector_candidate_limit"],
                    winner_candidate["keyword_candidate_limit"],
                ),
                max(
                    runner_candidate["vector_candidate_limit"],
                    runner_candidate["keyword_candidate_limit"],
                ),
            ),
            (
                "canonical_configuration_bytes",
                "ascending",
                canonical_preimage(winner_candidate).decode(),
                canonical_preimage(runner_candidate).decode(),
            ),
        ]
    )
    compared = []
    for name, direction, selected_value, runner_value in criteria:
        decisive = selected_value != runner_value
        compared.append(
            {
                "criterion": name,
                "decisive": decisive,
                "direction": direction,
                "runner_up_value": runner_value,
                "selected_value": selected_value,
            }
        )
        if decisive:
            break
    return {
        "candidate_count": 36,
        "compared_with_runner_up": compared,
        "runner_up": runner_candidate,
        "selected_rank": 1,
    }


def build_lock(search_space_path: Path, tuning_root: Path) -> dict[str, Any]:
    search_bytes = search_space_path.read_bytes()
    if sha256(search_bytes) != SEARCH_SPACE_SHA256:
        raise LockError("pre-registered search-space SHA-256 changed")
    search = read_json(search_space_path)
    if len(search["candidates"]) != 36:
        raise LockError("pre-registered search space does not contain 36 candidates")

    root_manifest = read_json(tuning_root / "manifest.json")
    verify_inventory(tuning_root, root_manifest)
    summary = read_json(tuning_root / "tuning-summary.json")
    provisional = read_json(tuning_root / "selected-configuration-provisional.json")
    rows = summary["candidates"]
    if len(rows) != 36 or root_manifest["candidate_count"] != 36:
        raise LockError("tuning artifacts do not contain all 36 candidates")
    registered = {
        canonical_preimage(candidate) for candidate in search["candidates"]
    }
    observed = {canonical_preimage(row["candidate"]) for row in rows}
    if registered != observed:
        raise LockError("tuning candidate set is not exactly the registered search space")

    for row in rows:
        candidate_root = tuning_root / "candidates" / row["run_id"]
        candidate_manifest = read_json(candidate_root / "manifest.json")
        verify_inventory(candidate_root, candidate_manifest)
        configuration = read_json(candidate_root / "configuration.json")
        metrics = read_json(candidate_root / "metrics.json")
        persistence = read_json(candidate_root / "persistence.json")
        if (
            configuration["candidate"] != row["candidate"]
            or metrics["aggregate"] != row["aggregate"]
            or candidate_manifest["status"] != "valid"
            or persistence["save_validate_load_equivalent"] is not True
            or persistence["deterministic_repeat_equal"] is not True
            or persistence["ranking_equal_after_reload"] is not True
        ):
            raise LockError(f"candidate artifact mismatch for '{row['run_id']}'")

    ordered = sorted(rows, key=candidate_sort_key)
    winner = ordered[0]
    if provisional["selected"]["candidate"] != winner["candidate"]:
        raise LockError("provisional winner differs from mechanical replay")
    runner_up = ordered[1]
    selected_root = tuning_root / "candidates" / winner["run_id"]
    selected_configuration = read_json(selected_root / "configuration.json")
    run_configuration = selected_configuration["configuration"]
    candidate = winner["candidate"]
    candidate_preimage = canonical_preimage(candidate)
    alpha_f32 = struct.unpack(">f", struct.pack(">f", candidate["fusion_alpha"]))[0]
    alpha_bits = struct.pack(">f", alpha_f32).hex()
    objective_hash = sha256(canonical_preimage(search["selection_objective"]))
    bm25_hash = sha256(canonical_preimage(run_configuration["bm25_policy"]))
    normalization_hash = sha256(
        canonical_preimage(run_configuration["normalization_policy"])
    )

    return {
        "adapter_manifest_sha256": ADAPTER_MANIFEST_SHA256,
        "bm25_policy_sha256": bm25_hash,
        "development_collection": {
            "collection_id": "hotpotqa-linked-abstracts-graph-v1-development",
            "collection_json_sha256": DEVELOPMENT_COLLECTION_SHA256,
            "collection_version": "1",
            "corpus_id": "hotpotqa-linked-abstracts-corpus-v1",
            "identity": "hotpotqa-linked-abstracts-graph-v1/development@1",
        },
        "development_population_sha256": DEVELOPMENT_POPULATION_SHA256,
        "normalization_policy_sha256": normalization_hash,
        "protocol_schema": "hotpotqa-phase-3-selected-configuration-v1",
        "quantization_policy_sha256": run_configuration[
            "quantization_policy_sha256"
        ],
        "search_space_sha256": SEARCH_SPACE_SHA256,
        "selected_candidate": {
            "fusion_alpha": candidate["fusion_alpha"],
            "fusion_alpha_f32_bits": alpha_bits,
            "keyword_candidate_limit": candidate["keyword_candidate_limit"],
            "vector_candidate_limit": candidate["vector_candidate_limit"],
        },
        "selected_configuration_preimage_sha256": sha256(candidate_preimage),
        "selected_development_aggregate": winner["aggregate"],
        "selection_objective": search["selection_objective"],
        "selection_objective_sha256": objective_hash,
        "selection_source": "development Run C alone",
        "shared_reporting_requirement": "Runs C and G must use this exact candidate",
        "test_results_status": "unavailable and not inspected",
        "tie_break_trace": first_decisive_trace(winner, runner_up),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--search-space", type=Path, default=DEFAULT_SEARCH_SPACE)
    parser.add_argument("--tuning-root", type=Path, default=DEFAULT_TUNING_ROOT)
    parser.add_argument("--lock", type=Path, default=DEFAULT_LOCK)
    parser.add_argument("--check", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        lock = build_lock(args.search_space.resolve(), args.tuning_root.resolve())
        expected = canonical_bytes(lock)
        if args.check:
            actual = args.lock.read_bytes()
            if actual != expected:
                raise LockError(
                    f"selected-configuration lock '{args.lock}' is not byte-identical to replay"
                )
            print(
                json.dumps(
                    {
                        "lock_sha256": sha256(actual),
                        "status": "valid",
                        "winner_replayed": True,
                    },
                    sort_keys=True,
                )
            )
        else:
            sys.stdout.buffer.write(expected)
    except (LockError, OSError, KeyError, TypeError, ValueError) as error:
        print(f"HotpotQA Phase 3 configuration freeze failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
