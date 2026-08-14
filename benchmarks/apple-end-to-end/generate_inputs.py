#!/usr/bin/env python3
"""Generate the deterministic graph-free Apple end-to-end benchmark inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path


SCHEMA_VERSION = 1
SEED = 0xA11E2E
CHUNK_KEYS = ("overview", "procedure", "diagnostics", "reference")
DOMAINS = (
    "account security", "offline notes", "photo organization", "travel planning",
    "expense tracking", "team projects", "customer support", "device setup",
    "health records", "course materials", "legal documents", "home inventory",
)
LOCATIONS = ("Ankara", "Berlin", "Boston", "London", "Paris", "Seattle", "Tokyo", "Toronto")
TEAMS = ("Atlas", "Beacon", "Cedar", "Delta", "Ember", "Falcon", "Harbor", "Juniper")
STATES = ("draft", "active", "review", "archived")


def compact_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def chunk_text(record_number: int, chunk_number: int) -> str:
    domain = DOMAINS[record_number % len(DOMAINS)]
    location = LOCATIONS[(record_number // len(DOMAINS)) % len(LOCATIONS)]
    team = TEAMS[(record_number // 3) % len(TEAMS)]
    state = STATES[(record_number // 7) % len(STATES)]
    identifier = f"RK-{record_number:05d}-{chunk_number + 1}"
    key = CHUNK_KEYS[chunk_number]
    bodies = (
        f"This {key} explains the {domain} workspace owned by team {team} in {location}. "
        f"The item is {state} and its reference is {identifier}. It summarizes goals, owners, "
        "important dates, and the information a person usually needs when searching the app.",
        f"Use this {key} when completing a {domain} task. Open the local workspace, confirm team "
        f"{team}, select the {location} collection, and follow reference {identifier}. The steps "
        "include verification, a safe fallback, and the expected completion state.",
        f"Troubleshooting for {domain}: if the {state} item cannot be found, check spelling, the "
        f"team {team} filter, the {location} collection, and identifier {identifier}. Review the "
        "offline copy before changing or deleting any stored information.",
        f"Reference details for {identifier}. Domain: {domain}. Team: {team}. Location: {location}. "
        f"State: {state}. This entry contains searchable names, exact identifiers, related terms, "
        "and enough surrounding text to hydrate a realistic local search result.",
    )
    return bodies[chunk_number]


def write_corpus(path: Path, active_records: int) -> tuple[int, str]:
    digest = hashlib.sha256()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as output:
        for number in range(active_records):
            record = {
                "chunks": [
                    {"chunk_key": key, "text": chunk_text(number, index)}
                    for index, key in enumerate(CHUNK_KEYS)
                ],
                "metadata": {
                    "domain": DOMAINS[number % len(DOMAINS)],
                    "state": STATES[(number // 7) % len(STATES)],
                    "team": TEAMS[(number // 3) % len(TEAMS)],
                },
                "record_id": f"record-{number:05d}",
                "text": f"Local application record {number:05d}",
            }
            line = (compact_json(record) + "\n").encode("utf-8")
            output.write(line)
            digest.update(line)
    return path.stat().st_size, digest.hexdigest()


def padded_query(prefix: str, target_words: int, salt: int) -> str:
    # Every filler is a common uncased MiniLM vocabulary word. The Swift input
    # validator records the authoritative WordPiece count before any benchmark.
    fillers = ("search", "local", "document", "information", "project", "result", "details")
    words = prefix.split()
    while len(words) < target_words:
        words.append(fillers[(len(words) + salt) % len(fillers)])
    return " ".join(words[:target_words])


def make_queries() -> list[dict[str, object]]:
    categories = (
        ["semantic_paraphrase"] * 40
        + ["exact_name_or_identifier"] * 30
        + ["semantic_plus_keyword"] * 20
        + ["near_distractor_or_no_natural_match"] * 10
    )
    # Desired counts after the tokenizer adds CLS and SEP. Prefixes may split
    # into additional pieces, so Swift validates the normative bucket, not this
    # word-count construction hint.
    buckets = (
        [(1, 16, 20, 10)]
        + [(17, 32, 35, 24)]
        + [(33, 64, 25, 46)]
        + [(65, 128, 15, 92)]
        + [(129, 256, 5, 184)]
    )
    bucket_rows: list[tuple[int, int, int]] = []
    for minimum, maximum, count, target_words in buckets:
        bucket_rows.extend([(minimum, maximum, target_words)] * count)

    queries: list[dict[str, object]] = []
    for index, (category, bucket) in enumerate(zip(categories, bucket_rows, strict=True)):
        record = index % 90
        domain = DOMAINS[record % len(DOMAINS)]
        team = TEAMS[(record // 3) % len(TEAMS)]
        location = LOCATIONS[(record // len(DOMAINS)) % len(LOCATIONS)]
        if category == "semantic_paraphrase":
            prefix = f"find the offline instructions about {domain} for the {team} group in {location}"
        elif category == "exact_name_or_identifier":
            prefix = f"RK-{record:05d}-4 {team} {location}"
        elif category == "semantic_plus_keyword":
            prefix = f"how do I troubleshoot {domain} RK-{record:05d}-3 for team {team}"
        else:
            prefix = f"unrelated lunar shipping policy for a missing violet archive number {90000 + index}"
        minimum, maximum, target_words = bucket
        queries.append({
            "category": category,
            "expected_token_bucket": {"maximum": maximum, "minimum": minimum},
            "id": f"query-{index:03d}",
            "target_record_id": None if category.startswith("near_distractor") else f"record-{record:05d}",
            "text": padded_query(prefix, target_words, index),
        })
    return queries


def make_schedule(query_ids: list[str]) -> list[str]:
    randomizer = random.Random(SEED)
    schedule: list[str] = []
    while len(schedule) < 750:
        cycle = list(query_ids)
        randomizer.shuffle(cycle)
        schedule.extend(cycle)
    return schedule[:750]


def write_queries(path: Path) -> tuple[int, str]:
    queries = make_queries()
    document = {
        "queries": queries,
        "schedule": make_schedule([str(query["id"]) for query in queries]),
        "schema_version": SCHEMA_VERSION,
        "seed": SEED,
    }
    data = (compact_json(document) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return len(data), hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--active-records", type=int, required=True)
    args = parser.parse_args()
    if args.active_records not in (2_500, 12_500, 25_000):
        parser.error("--active-records must be 2500, 12500, or 25000")

    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    corpus_bytes, corpus_sha = write_corpus(output / "corpus.jsonl", args.active_records)
    query_bytes, query_sha = write_queries(output / "queries.json")
    manifest = {
        "active_chunks": args.active_records * 4,
        "active_records": args.active_records,
        "chunks_per_record": 4,
        "files": {
            "corpus.jsonl": {"bytes": corpus_bytes, "sha256": corpus_sha},
            "queries.json": {"bytes": query_bytes, "sha256": query_sha},
        },
        "generator": "benchmarks/apple-end-to-end/generate_inputs.py",
        "schema_version": SCHEMA_VERSION,
        "seed": SEED,
    }
    (output / "source-manifest.json").write_text(compact_json(manifest) + "\n", encoding="utf-8")
    print(compact_json(manifest))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
