import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


generator = load_module("generate_inputs", HERE / "generate_inputs.py")


class GenerateInputsTests(unittest.TestCase):
    def test_queries_match_frozen_populations_and_schedule(self) -> None:
        queries = generator.make_queries()
        self.assertEqual(len(queries), 100)
        counts: dict[str, int] = {}
        for query in queries:
            counts[query["category"]] = counts.get(query["category"], 0) + 1
        self.assertEqual(counts, {
            "semantic_paraphrase": 40,
            "exact_name_or_identifier": 30,
            "semantic_plus_keyword": 20,
            "near_distractor_or_no_natural_match": 10,
        })
        schedule = generator.make_schedule([query["id"] for query in queries])
        self.assertEqual(len(schedule), 750)
        self.assertEqual(schedule, generator.make_schedule([query["id"] for query in queries]))
        self.assertGreater(len(set(schedule)), 1)

    def test_corpus_generation_is_byte_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "first.jsonl"
            second = root / "second.jsonl"
            first_stats = generator.write_corpus(first, 5)
            second_stats = generator.write_corpus(second, 5)
            self.assertEqual(first_stats, second_stats)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            records = [json.loads(line) for line in first.read_text().splitlines()]
            self.assertEqual(len(records), 5)
            self.assertTrue(all(len(record["chunks"]) == 4 for record in records))


if __name__ == "__main__":
    unittest.main()
