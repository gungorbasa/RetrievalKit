from __future__ import annotations

import inspect
import unittest

from scripts.quality import inspect_public_graph_collections as probe


class PublicGraphCollectionProbeTests(unittest.TestCase):
    def test_source_parser_ignores_every_gold_field(self) -> None:
        row = {
            "answer": object(),
            "context": object(),
            "id": "q1",
            "level": "hard",
            "question": "Which Alpha article links to Beta?",
            "supporting_facts": object(),
            "type": "bridge",
        }
        self.assertEqual(
            probe.parse_hotpot_source_row(row, "train"),
            probe.SourceQuery(
                query_id="q1",
                split="train",
                text="Which Alpha article links to Beta?",
                category="bridge",
                level="hard",
            ),
        )

    def test_corpus_builder_has_no_judgment_or_path_argument(self) -> None:
        parameters = inspect.signature(probe.freeze_hotpot_corpus).parameters
        self.assertEqual(set(parameters), {"source_queries", "abstracts_dir"})
        self.assertNotIn(
            "parse_hotpot_judgment_row(",
            inspect.getsource(probe.freeze_hotpot_corpus),
        )

    def test_post_freeze_eligibility_needs_a_frozen_corpus(self) -> None:
        corpus = probe.FrozenCorpus(
            records=(
                probe.CorpusRecord("a", "1", "Alpha", "A", ()),
                probe.CorpusRecord("b", "2", "Beta", "B", ()),
            ),
            resolutions=(),
            preimage_sha256="0" * 64,
            source_conflicting_titles=0,
            source_records=2,
            source_unique_titles=2,
            selected_conflicting_titles=0,
            selected_missing_titles=0,
        )
        judgments = (
            probe.Judgment("q1", "x", (("Alpha", 0), ("Beta", 1))),
            probe.Judgment("q2", "y", (("Alpha", 0), ("Gamma", 0))),
        )
        eligible, reasons = probe.post_freeze_eligibility(corpus, judgments)
        self.assertEqual(eligible, ("q1",))
        self.assertEqual(reasons, {"not_in_frozen_corpus": 1})

    def test_alias_matching_preserves_punctuation_and_uses_boundaries(self) -> None:
        aliases = probe.alias_substrings("Was Alpha-film linked to Beta?")
        self.assertIn("alpha-film", aliases)
        self.assertIn("beta", aliases)
        self.assertNotIn("lpha", aliases)

    def test_seed_resolution_is_deterministic(self) -> None:
        query = probe.SourceQuery("q", "train", "Alpha", "bridge", "hard")
        candidate = probe.SeedCandidate("r", "1", "Alpha", "alpha", ("beta",))
        first = probe.resolve_seed_candidates((query,), {"q": (candidate,)})
        second = probe.resolve_seed_candidates((query,), {"q": (candidate,)})
        self.assertEqual(first, second)
        self.assertEqual(first[0].status, "resolved")

    def test_hard_corpus_bound_is_below_50k(self) -> None:
        self.assertEqual(probe.MAX_CORPUS_RECORDS, 48_000)
        self.assertLess(probe.MAX_CORPUS_RECORDS, 50_000)

    def test_streaming_json_array_parser(self) -> None:
        import io

        rows = list(probe.iter_json_array(io.BytesIO(b'[ {"a":1}, {"a":2} ]'), 3))
        self.assertEqual(rows, [{"a": 1}, {"a": 2}])


if __name__ == "__main__":
    unittest.main()
