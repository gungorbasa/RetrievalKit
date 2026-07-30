import importlib.util
import json
import math
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-python-node-wrapper-conformance.py")
SPEC = importlib.util.spec_from_file_location("wrapper_conformance", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def basis(index: int) -> list[float]:
    vector = [0.0] * MODULE.DIMENSION
    vector[index] = 1.0
    return vector


def structured_input(corpus_count: int = 11, query_count: int = 2) -> dict:
    items = [
        {"id": f"corpus-{index}", "role": "corpus", "text": f"corpus {index}"}
        for index in range(corpus_count)
    ]
    items.extend(
        {"id": f"query-{index}", "role": "query", "text": f"query {index}"}
        for index in range(query_count)
    )
    items.append({"id": "unicode", "role": "diagnostic", "text": "İstanbul 東京"})
    return {"schema_version": 1, "items": items}


def reference_for(input_document: dict) -> list[list[float]]:
    corpus_count = sum(item["role"] == "corpus" for item in input_document["items"])
    vectors = [basis(index) for index in range(corpus_count)]
    query_count = sum(item["role"] == "query" for item in input_document["items"])
    vectors.extend(basis(index) for index in range(query_count))
    vectors.append(basis(20))
    return vectors


def output_for(input_document: dict, vectors: list[list[float]]) -> dict:
    return {
        "schema_version": MODULE.SCHEMA_VERSION,
        "model": {
            "identifier": MODULE.MODEL_IDENTIFIER,
            "revision": MODULE.MODEL_REVISION,
            "profile": MODULE.PROFILE,
            "dtype": MODULE.DTYPE,
            "dimension": MODULE.DIMENSION,
            "max_input_tokens": MODULE.MAX_INPUT_TOKENS,
            "normalized": MODULE.NORMALIZED,
        },
        "items": [
            {"id": item["id"], "embedding": vector}
            for item, vector in zip(input_document["items"], vectors, strict=True)
        ],
    }


class WrapperConformanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.input_document = structured_input()
        self.input_items = MODULE.load_input_items(self.input_document)
        self.reference = reference_for(self.input_document)
        self.candidate = output_for(self.input_document, self.reference)

    def validate(self, candidate: dict | list) -> dict:
        return MODULE.validate_candidate(
            "fixture", candidate, self.input_items, self.reference
        )

    def test_matching_output_passes_cosine_and_ranking_gates(self) -> None:
        report = self.validate(self.candidate)
        self.assertTrue(report["passed"])
        self.assertEqual(report["metrics"]["cosine"]["median"], 1.0)
        self.assertEqual(
            report["metrics"]["cosine"]["lowest_vectors"][0]["id"], "corpus-0"
        )
        self.assertEqual(report["metrics"]["ranking"]["mean_top_k_overlap"], 1.0)
        self.assertEqual(report["metrics"]["ranking"]["exact_top_k_fraction"], 1.0)

    def test_legacy_input_and_reference_arrays_remain_supported(self) -> None:
        items = MODULE.load_input_items(["first", "second"])
        reference = MODULE.load_reference_vectors([basis(0), basis(1)], 2)
        candidate = output_for(
            {
                "items": [
                    {"id": "0", "text": "first"},
                    {"id": "1", "text": "second"},
                ]
            },
            reference,
        )
        report = MODULE.validate_candidate("legacy", candidate, items, reference)
        self.assertTrue(report["passed"])
        self.assertFalse(report["metrics"]["ranking"]["evaluated"])

    def test_metadata_is_exact(self) -> None:
        self.candidate["model"]["revision"] = "main"
        report = self.validate(self.candidate)
        self.assertFalse(report["passed"])
        self.assertEqual(
            report["diagnostics"]["items"][0]["path"], "model.revision"
        )

    def test_count_and_order_are_checked(self) -> None:
        self.candidate["items"][0], self.candidate["items"][1] = (
            self.candidate["items"][1],
            self.candidate["items"][0],
        )
        report = self.validate(self.candidate)
        self.assertFalse(report["passed"])
        order_issues = [
            item
            for item in report["diagnostics"]["items"]
            if item["code"] == "item_order"
        ]
        self.assertEqual(len(order_issues), 2)

    def test_dimension_finite_and_normalized_contract_is_checked(self) -> None:
        cases = ([0.0] * 383, [math.inf] + [0.0] * 383, [0.0] * 384)
        expected = ("vector_dimension", "vector_value", "vector_norm")
        for vector, code in zip(cases, expected, strict=True):
            with self.subTest(code=code):
                candidate = output_for(self.input_document, self.reference)
                candidate["items"][0]["embedding"] = vector
                report = self.validate(candidate)
                self.assertFalse(report["passed"])
                self.assertIn(
                    code, {item["code"] for item in report["diagnostics"]["items"]}
                )

    def test_cosine_gate_rejects_wrong_but_normalized_vectors(self) -> None:
        candidate = output_for(self.input_document, self.reference)
        for item in candidate["items"]:
            item["embedding"] = basis(100)
        report = self.validate(candidate)
        self.assertFalse(report["passed"])
        self.assertLess(
            report["metrics"]["cosine"]["median"], MODULE.COSINE_MEDIAN_GATE
        )
        self.assertIn(
            "cosine_gate",
            {item["code"] for item in report["diagnostics"]["items"]},
        )

    def test_ranking_gate_reports_exact_bounded_worst_queries(self) -> None:
        candidate = output_for(self.input_document, self.reference)
        candidate["items"][11]["embedding"] = basis(10)
        report = MODULE.validate_candidate(
            "fixture",
            candidate,
            self.input_items,
            self.reference,
            diagnostic_limit=1,
        )
        self.assertFalse(report["passed"])
        ranking = report["metrics"]["ranking"]
        self.assertLess(ranking["minimum_top_k_overlap"], 1.0)
        self.assertEqual(ranking["worst_queries"][0]["query_id"], "query-0")

    def test_diagnostics_are_bounded_and_count_remains_exact(self) -> None:
        candidate = output_for(self.input_document, self.reference)
        for item in candidate["items"]:
            item["id"] = "wrong"
        report = MODULE.validate_candidate(
            "fixture",
            candidate,
            self.input_items,
            self.reference,
            diagnostic_limit=3,
        )
        diagnostics = report["diagnostics"]
        self.assertEqual(diagnostics["total"], len(self.input_items))
        self.assertEqual(diagnostics["reported"], 3)
        self.assertTrue(diagnostics["truncated"])

    def test_cli_validates_two_candidates_and_writes_deterministic_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_path = root / "input.json"
            reference_path = root / "reference.json"
            python_path = root / "python.json"
            node_path = root / "node.json"
            output_path = root / "report.json"
            input_path.write_text(json.dumps(self.input_document), encoding="utf-8")
            reference_path.write_text(json.dumps(self.reference), encoding="utf-8")
            python_path.write_text(json.dumps(self.candidate), encoding="utf-8")
            node_path.write_text(json.dumps(self.candidate), encoding="utf-8")

            command = [
                sys.executable,
                str(SCRIPT),
                "--input",
                str(input_path),
                "--reference",
                str(reference_path),
                "--candidate",
                f"python={python_path}",
                "--candidate",
                f"node={node_path}",
                "--output",
                str(output_path),
            ]
            first = subprocess.run(command, check=False, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            first_bytes = output_path.read_bytes()
            second = subprocess.run(command, check=False, capture_output=True, text=True)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(output_path.read_bytes(), first_bytes)
            report = json.loads(first_bytes)
            self.assertTrue(report["passed"])
            self.assertEqual(
                [candidate["label"] for candidate in report["candidates"]],
                ["python", "node"],
            )

    def test_invalid_reference_is_a_contract_error(self) -> None:
        with self.assertRaisesRegex(MODULE.ContractError, "invalid reference"):
            MODULE.load_reference_vectors([[0.0] * 384], 1)


if __name__ == "__main__":
    unittest.main()
