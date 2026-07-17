#!/usr/bin/env python3
"""Focused synthetic tests for the frozen HotpotQA source corpus."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from collections.abc import Iterator, Mapping
from pathlib import Path
from typing import Any


SCRIPT = Path(__file__).with_name("build_hotpotqa_graph_collection.py")
SPEC = importlib.util.spec_from_file_location("hotpot_builder", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
hotpot = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = hotpot
SPEC.loader.exec_module(hotpot)


def abstract(
    source_id: str,
    title: str,
    text: str,
    links: list[str] | None = None,
) -> dict[str, Any]:
    anchors = "".join(f'<a href="{link}">{link}</a>' for link in (links or []))
    return {
        "charoffset": [],
        "charoffset_with_links": [],
        "id": source_id,
        "text": [text],
        "text_with_links": [text + anchors],
        "title": title,
        "url": f"https://example.test/{source_id}",
    }


class GoldTrap(Mapping[str, Any]):
    def __init__(self) -> None:
        self.source = {
            "id": "q1",
            "question": "Where is Alpha?",
            "type": "bridge",
            "level": "easy",
        }

    def __getitem__(self, key: str) -> Any:
        if key in {"answer", "context", "supporting_facts", "qrels", "evidence"}:
            raise AssertionError(f"gold field was accessed: {key}")
        return self.source[key]

    def __iter__(self) -> Iterator[str]:
        return iter((*self.source, "answer", "context", "supporting_facts"))

    def __len__(self) -> int:
        return 7


class HotpotSourceCorpusTests(unittest.TestCase):
    def test_source_parser_cannot_reach_judgments(self) -> None:
        parsed = hotpot.parse_source_query(GoldTrap(), "train")
        self.assertEqual(parsed.upstream_id, "q1")
        self.assertEqual(
            set(parsed.__dataclass_fields__),
            {"upstream_id", "split", "question_text", "query_type", "level"},
        )

    def test_sampling_uses_frozen_salted_hash_order(self) -> None:
        rows = [
            {"id": value, "question": value, "type": "bridge", "level": "easy"}
            for value in ["q3", "q1", "q2"]
        ]
        sampled = hotpot.sample_source_queries(rows, "train", 2)
        expected = sorted(
            [hotpot.parse_source_query(row, "train") for row in rows],
            key=hotpot.sampling_key,
        )[:2]
        self.assertEqual(sampled, tuple(expected))

    def test_normalization_boundaries_and_longest_resolution(self) -> None:
        self.assertEqual(hotpot.normalize("  STRAßE\t Café  "), "strasse café")
        substrings = hotpot.alias_substrings("Is Alpha-Beta related?")
        self.assertIn("alpha-beta", substrings)
        self.assertNotIn("pha", substrings)
        query = hotpot.SourceQuery("q", "train", "Alpha Beta", "bridge", "easy")
        candidates = {
            "q": [
                hotpot.SeedCandidate("r1", "1", "Alpha", "alpha", ()),
                hotpot.SeedCandidate("r2", "2", "Alpha Beta", "alpha beta", ()),
            ]
        }
        resolution = hotpot.resolve_seed_candidates([query], candidates)[0]
        self.assertEqual(resolution.status, "resolved")
        self.assertEqual(resolution.selected_record_id, "r2")

    def test_ambiguity_and_no_match(self) -> None:
        queries = [
            hotpot.SourceQuery("a", "train", "Shared", "bridge", "easy"),
            hotpot.SourceQuery("n", "train", "Nothing", "bridge", "easy"),
        ]
        candidates = {
            "a": [
                hotpot.SeedCandidate("r1", "1", "Shared", "shared", ()),
                hotpot.SeedCandidate("r2", "2", "Shared", "shared", ()),
            ]
        }
        resolutions = hotpot.resolve_seed_candidates(queries, candidates)
        self.assertEqual([row.status for row in resolutions], ["ambiguous", "no_match"])

    def test_conflict_winner_links_missing_dedup_and_identity_order(self) -> None:
        query = hotpot.SourceQuery("q", "train", "Alpha", "bridge", "easy")
        rows = [
            abstract("1", "Alpha", "alpha", ["Beta", "Beta", "Missing"]),
            abstract("20", "Beta", "new", ["Alpha", "Alpha"]),
            abstract("10", "Beta", "old", ["Alpha", "Alpha"]),
        ]
        first = hotpot.freeze_source_corpus([query], lambda: iter(rows))
        second = hotpot.freeze_source_corpus([query], lambda: iter(rows))
        self.assertEqual(first, second)
        self.assertEqual([row.record_id for row in first.records], ["hotpotqa:wiki:1", "hotpotqa:wiki:10"])
        alpha = first.records[0]
        self.assertEqual(first.records[1].text, "old")
        self.assertEqual(alpha.outgoing_record_ids, ("hotpotqa:wiki:10",))
        self.assertEqual(first.selected_missing_titles, ("missing",))
        self.assertEqual(first.selected_conflicting_titles, ("beta",))

    def test_count_or_hash_mismatch_is_rejected(self) -> None:
        query = hotpot.SourceQuery("q", "train", "Alpha", "bridge", "easy")
        corpus = hotpot.freeze_source_corpus(
            [query], lambda: iter([abstract("1", "Alpha", "text")])
        )
        with self.assertRaisesRegex(hotpot.AdapterError, "count/hash mismatch"):
            hotpot.validate_frozen_corpus(corpus, {"records": 2})


if __name__ == "__main__":
    unittest.main()
