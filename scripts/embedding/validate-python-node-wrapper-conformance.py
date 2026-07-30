#!/usr/bin/env python3
"""Validate Python and Node embedding-wrapper output against frozen FP32 vectors.

The validator is deliberately offline and dependency-free. It accepts the
legacy frozen input/reference arrays as well as the versioned wrapper output
contract documented by ``--help``.
"""

from __future__ import annotations

import argparse
import json
import math
import statistics
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

SCHEMA_VERSION = 1
MODEL_IDENTIFIER = "sentence-transformers/all-MiniLM-L6-v2"
MODEL_REVISION = "c9745ed1d9f207416be6d2e6f8de32d1f16199bf"
PROFILE = "fp32"
DTYPE = "float32"
DIMENSION = 384
MAX_INPUT_TOKENS = 256
NORMALIZED = True

COSINE_MEDIAN_GATE = 0.9999
TOP_K = 10
MEAN_TOP_K_OVERLAP_GATE = 0.99
EXACT_TOP_K_FRACTION_GATE = 0.90
MIN_TOP_K_OVERLAP_GATE = 0.90
NORM_TOLERANCE = 1e-4
DEFAULT_DIAGNOSTIC_LIMIT = 25


class ContractError(ValueError):
    """The fixture or reference is not valid enough to run qualification."""


@dataclass(frozen=True)
class InputItem:
    identifier: str
    text: str
    role: str | None


class Diagnostics:
    """Collect exact diagnostics while bounding report size."""

    def __init__(self, limit: int) -> None:
        if limit < 1:
            raise ValueError("diagnostic limit must be at least one")
        self.limit = limit
        self.total = 0
        self.items: list[dict[str, str]] = []

    def add(self, code: str, path: str, message: str) -> None:
        self.total += 1
        if len(self.items) < self.limit:
            self.items.append({"code": code, "path": path, "message": message})

    def as_json(self) -> dict[str, Any]:
        return {
            "total": self.total,
            "reported": len(self.items),
            "truncated": self.total > len(self.items),
            "items": self.items,
        }


def _reject_json_constant(value: str) -> None:
    raise ContractError(f"non-standard JSON numeric constant is forbidden: {value}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"), parse_constant=_reject_json_constant)
    except (OSError, UnicodeError, json.JSONDecodeError, ContractError) as error:
        raise ContractError(f"cannot read valid JSON from {path}: {error}") from error


def load_input_items(document: Any) -> list[InputItem]:
    if isinstance(document, list):
        items: list[InputItem] = []
        for index, text in enumerate(document):
            if not isinstance(text, str):
                raise ContractError(f"input[{index}] must be a string")
            items.append(InputItem(str(index), text, None))
        if not items:
            raise ContractError("input must contain at least one item")
        return items

    if not isinstance(document, dict):
        raise ContractError("input must be a string array or a versioned object")
    if document.get("schema_version") != SCHEMA_VERSION:
        raise ContractError(f"input.schema_version must equal {SCHEMA_VERSION}")
    raw_items = document.get("items")
    if not isinstance(raw_items, list) or not raw_items:
        raise ContractError("input.items must be a non-empty array")

    items = []
    seen: set[str] = set()
    valid_roles = {"corpus", "query", "diagnostic"}
    for index, raw in enumerate(raw_items):
        if not isinstance(raw, dict):
            raise ContractError(f"input.items[{index}] must be an object")
        identifier = raw.get("id")
        text = raw.get("text")
        role = raw.get("role")
        if not isinstance(identifier, str) or not identifier:
            raise ContractError(f"input.items[{index}].id must be a non-empty string")
        if identifier in seen:
            raise ContractError(f"input item id is duplicated: {identifier!r}")
        if not isinstance(text, str):
            raise ContractError(f"input.items[{index}].text must be a string")
        if role is not None and role not in valid_roles:
            raise ContractError(
                f"input.items[{index}].role must be corpus, query, diagnostic, or absent"
            )
        seen.add(identifier)
        items.append(InputItem(identifier, text, role))
    return items


def _to_f32(value: Any) -> float | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    number = float(value)
    if not math.isfinite(number):
        return None
    try:
        return struct.unpack("<f", struct.pack("<f", number))[0]
    except (OverflowError, struct.error):
        return None


def _check_vector(
    raw: Any,
    path: str,
    diagnostics: Diagnostics,
    *,
    check_norm: bool,
) -> list[float] | None:
    if not isinstance(raw, list):
        diagnostics.add("vector_type", path, "must be an array")
        return None
    if len(raw) != DIMENSION:
        diagnostics.add(
            "vector_dimension",
            path,
            f"must contain exactly {DIMENSION} values; found {len(raw)}",
        )
        return None

    vector: list[float] = []
    for index, value in enumerate(raw):
        converted = _to_f32(value)
        if converted is None:
            diagnostics.add(
                "vector_value",
                f"{path}[{index}]",
                "must be a finite float32-representable JSON number",
            )
            return None
        vector.append(converted)

    norm = math.sqrt(math.fsum(value * value for value in vector))
    if check_norm and abs(norm - 1.0) > NORM_TOLERANCE:
        diagnostics.add(
            "vector_norm",
            path,
            f"L2 norm must be within {NORM_TOLERANCE:g} of 1.0; found {norm:.9f}",
        )
        return None
    return vector


def load_reference_vectors(document: Any, expected_count: int) -> list[list[float]]:
    raw_vectors: Any
    if isinstance(document, list):
        raw_vectors = document
    elif isinstance(document, dict):
        raw_items = document.get("items")
        if not isinstance(raw_items, list):
            raise ContractError("reference.items must be an array")
        raw_vectors = [
            item.get("embedding") if isinstance(item, dict) else None for item in raw_items
        ]
    else:
        raise ContractError("reference must be a vector array or wrapper-output object")

    if len(raw_vectors) != expected_count:
        raise ContractError(
            f"reference vector count must equal input count {expected_count}; "
            f"found {len(raw_vectors)}"
        )
    diagnostics = Diagnostics(DEFAULT_DIAGNOSTIC_LIMIT)
    vectors = [
        _check_vector(raw, f"reference[{index}]", diagnostics, check_norm=True)
        for index, raw in enumerate(raw_vectors)
    ]
    if diagnostics.total:
        first = diagnostics.items[0]
        raise ContractError(
            f"invalid reference ({diagnostics.total} issue(s)); "
            f"{first['path']}: {first['message']}"
        )
    return [vector for vector in vectors if vector is not None]


def _validate_model_metadata(document: dict[str, Any], diagnostics: Diagnostics) -> None:
    if document.get("schema_version") != SCHEMA_VERSION:
        diagnostics.add(
            "schema_version",
            "schema_version",
            f"must equal {SCHEMA_VERSION}",
        )
    model = document.get("model")
    if not isinstance(model, dict):
        diagnostics.add("model_type", "model", "must be an object")
        return
    expected = {
        "identifier": MODEL_IDENTIFIER,
        "revision": MODEL_REVISION,
        "profile": PROFILE,
        "dtype": DTYPE,
        "dimension": DIMENSION,
        "max_input_tokens": MAX_INPUT_TOKENS,
        "normalized": NORMALIZED,
    }
    for key, expected_value in expected.items():
        if model.get(key) != expected_value:
            diagnostics.add(
                "model_metadata",
                f"model.{key}",
                f"must equal {expected_value!r}; found {model.get(key)!r}",
            )


def _cosine(left: Sequence[float], right: Sequence[float]) -> float:
    numerator = math.fsum(a * b for a, b in zip(left, right, strict=True))
    left_norm = math.sqrt(math.fsum(value * value for value in left))
    right_norm = math.sqrt(math.fsum(value * value for value in right))
    return numerator / (left_norm * right_norm)


def _rank(query: Sequence[float], corpus: Sequence[tuple[int, Sequence[float]]]) -> list[int]:
    scored = [
        (math.fsum(a * b for a, b in zip(query, vector, strict=True)), item_index)
        for item_index, vector in corpus
    ]
    scored.sort(key=lambda pair: (-pair[0], pair[1]))
    return [item_index for _, item_index in scored[: min(TOP_K, len(scored))]]


def _ranking_metrics(
    input_items: Sequence[InputItem],
    reference: Sequence[Sequence[float]],
    candidate: Sequence[Sequence[float] | None],
) -> dict[str, Any] | None:
    corpus_indices = [
        index for index, item in enumerate(input_items) if item.role == "corpus"
    ]
    query_indices = [
        index for index, item in enumerate(input_items) if item.role == "query"
    ]
    if not corpus_indices or not query_indices:
        return None
    if any(candidate[index] is None for index in corpus_indices + query_indices):
        return {
            "evaluated": False,
            "reason": "candidate corpus or query vector failed structural validation",
            "passed": False,
        }

    reference_corpus = [(index, reference[index]) for index in corpus_indices]
    candidate_corpus = [
        (index, candidate[index])
        for index in corpus_indices
        if candidate[index] is not None
    ]
    overlaps: list[float] = []
    exact = 0
    worst: list[dict[str, Any]] = []
    for query_index in query_indices:
        reference_top = _rank(reference[query_index], reference_corpus)
        candidate_query = candidate[query_index]
        assert candidate_query is not None
        candidate_top = _rank(candidate_query, candidate_corpus)
        denominator = len(reference_top)
        overlap = (
            len(set(reference_top).intersection(candidate_top)) / denominator
            if denominator
            else 1.0
        )
        overlaps.append(overlap)
        if set(reference_top) == set(candidate_top):
            exact += 1
        if overlap < 1.0:
            worst.append(
                {
                    "query_id": input_items[query_index].identifier,
                    "overlap": overlap,
                    "reference_top": [
                        input_items[index].identifier for index in reference_top
                    ],
                    "candidate_top": [
                        input_items[index].identifier for index in candidate_top
                    ],
                }
            )

    mean_overlap = statistics.fmean(overlaps)
    exact_fraction = exact / len(overlaps)
    minimum_overlap = min(overlaps)
    passed = (
        mean_overlap >= MEAN_TOP_K_OVERLAP_GATE
        and exact_fraction >= EXACT_TOP_K_FRACTION_GATE
        and minimum_overlap >= MIN_TOP_K_OVERLAP_GATE
    )
    worst.sort(key=lambda item: (item["overlap"], item["query_id"]))
    return {
        "evaluated": True,
        "query_count": len(query_indices),
        "corpus_count": len(corpus_indices),
        "top_k": min(TOP_K, len(corpus_indices)),
        "mean_top_k_overlap": mean_overlap,
        "exact_top_k_fraction": exact_fraction,
        "minimum_top_k_overlap": minimum_overlap,
        "gates": {
            "mean_top_k_overlap": MEAN_TOP_K_OVERLAP_GATE,
            "exact_top_k_fraction": EXACT_TOP_K_FRACTION_GATE,
            "minimum_top_k_overlap": MIN_TOP_K_OVERLAP_GATE,
        },
        "worst_queries": worst[:DEFAULT_DIAGNOSTIC_LIMIT],
        "worst_queries_truncated": len(worst) > DEFAULT_DIAGNOSTIC_LIMIT,
        "passed": passed,
    }


def validate_candidate(
    label: str,
    document: Any,
    input_items: Sequence[InputItem],
    reference: Sequence[Sequence[float]],
    diagnostic_limit: int = DEFAULT_DIAGNOSTIC_LIMIT,
) -> dict[str, Any]:
    diagnostics = Diagnostics(diagnostic_limit)
    if not isinstance(document, dict):
        diagnostics.add("document_type", "$", "wrapper output must be an object")
        return _candidate_report(label, diagnostics, [], None)

    _validate_model_metadata(document, diagnostics)
    raw_items = document.get("items")
    if not isinstance(raw_items, list):
        diagnostics.add("items_type", "items", "must be an array")
        return _candidate_report(label, diagnostics, [], None)
    if len(raw_items) != len(input_items):
        diagnostics.add(
            "item_count",
            "items",
            f"must contain {len(input_items)} items; found {len(raw_items)}",
        )

    vectors: list[list[float] | None] = [None] * len(input_items)
    for index in range(min(len(raw_items), len(input_items))):
        raw_item = raw_items[index]
        path = f"items[{index}]"
        if not isinstance(raw_item, dict):
            diagnostics.add("item_type", path, "must be an object")
            continue
        actual_id = raw_item.get("id")
        expected_id = input_items[index].identifier
        if actual_id != expected_id:
            diagnostics.add(
                "item_order",
                f"{path}.id",
                f"must equal input id {expected_id!r}; found {actual_id!r}",
            )
        vectors[index] = _check_vector(
            raw_item.get("embedding"),
            f"{path}.embedding",
            diagnostics,
            check_norm=True,
        )

    cosines = [
        _cosine(reference[index], vector)
        for index, vector in enumerate(vectors)
        if vector is not None
    ]
    cosine_metrics: dict[str, Any] | None
    if len(cosines) == len(input_items):
        median = statistics.median(cosines)
        lowest = sorted(
            (
                {
                    "id": input_items[index].identifier,
                    "index": index,
                    "cosine": cosine,
                }
                for index, cosine in enumerate(cosines)
            ),
            key=lambda item: (item["cosine"], item["index"]),
        )
        cosine_metrics = {
            "count": len(cosines),
            "median": median,
            "minimum": min(cosines),
            "maximum": max(cosines),
            "gate": COSINE_MEDIAN_GATE,
            "lowest_vectors": lowest[:diagnostic_limit],
            "lowest_vectors_truncated": len(lowest) > diagnostic_limit,
            "passed": median >= COSINE_MEDIAN_GATE,
        }
        if median < COSINE_MEDIAN_GATE:
            diagnostics.add(
                "cosine_gate",
                "metrics.cosine.median",
                f"must be at least {COSINE_MEDIAN_GATE}; found {median:.9f}",
            )
    else:
        cosine_metrics = {
            "count": len(cosines),
            "expected_count": len(input_items),
            "passed": False,
        }

    ranking = _ranking_metrics(input_items, reference, vectors)
    if ranking is not None and not ranking["passed"]:
        diagnostics.add(
            "ranking_gate",
            "metrics.ranking",
            "one or more Top-10 ranking gates failed",
        )
    return _candidate_report(label, diagnostics, cosines, ranking, cosine_metrics)


def _candidate_report(
    label: str,
    diagnostics: Diagnostics,
    cosines: Sequence[float],
    ranking: dict[str, Any] | None,
    cosine_metrics: dict[str, Any] | None = None,
) -> dict[str, Any]:
    del cosines
    metrics: dict[str, Any] = {"cosine": cosine_metrics}
    metrics["ranking"] = (
        ranking
        if ranking is not None
        else {"evaluated": False, "reason": "input has no corpus/query roles"}
    )
    return {
        "label": label,
        "passed": diagnostics.total == 0,
        "metrics": metrics,
        "diagnostics": diagnostics.as_json(),
    }


def build_report(
    input_items: Sequence[InputItem],
    reference: Sequence[Sequence[float]],
    candidates: Sequence[tuple[str, Any]],
    diagnostic_limit: int,
) -> dict[str, Any]:
    reports = [
        validate_candidate(label, document, input_items, reference, diagnostic_limit)
        for label, document in candidates
    ]
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "retrievalkit_python_node_embedding_wrapper_conformance",
        "contract": {
            "model_identifier": MODEL_IDENTIFIER,
            "model_revision": MODEL_REVISION,
            "profile": PROFILE,
            "dtype": DTYPE,
            "dimension": DIMENSION,
            "max_input_tokens": MAX_INPUT_TOKENS,
            "normalized": NORMALIZED,
        },
        "input_count": len(input_items),
        "reference_count": len(reference),
        "ranking_roles_present": bool(
            any(item.role == "corpus" for item in input_items)
            and any(item.role == "query" for item in input_items)
        ),
        "passed": bool(reports) and all(report["passed"] for report in reports),
        "candidates": reports,
    }


def _parse_candidate(value: str) -> tuple[str, Path]:
    label, separator, raw_path = value.partition("=")
    if not separator or not label or not raw_path:
        raise argparse.ArgumentTypeError("candidate must use LABEL=PATH")
    return label, Path(raw_path)


def parse_args(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Offline validation of Python/Node FP32 MiniLM wrapper JSON against "
            "frozen Rust CPU FP32 vectors."
        ),
        epilog=(
            "Wrapper output schema: "
            '{"schema_version":1,"model":{"identifier":"'
            + MODEL_IDENTIFIER
            + '","revision":"'
            + MODEL_REVISION
            + '","profile":"fp32","dtype":"float32","dimension":384,'
            '"max_input_tokens":256,"normalized":true},'
            '"items":[{"id":"<input id>","embedding":[384 finite numbers]}]}. '
            "For a legacy string-array input, item IDs are decimal indices."
        ),
    )
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument(
        "--candidate",
        action="append",
        required=True,
        type=_parse_candidate,
        metavar="LABEL=PATH",
        help="wrapper output to validate; repeat for Python and Node",
    )
    parser.add_argument("--output", type=Path, help="write the deterministic report here")
    parser.add_argument(
        "--diagnostic-limit",
        type=int,
        default=DEFAULT_DIAGNOSTIC_LIMIT,
        help=f"maximum detailed issues per candidate (default: {DEFAULT_DIAGNOSTIC_LIMIT})",
    )
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    args = parse_args(arguments)
    try:
        if args.diagnostic_limit < 1:
            raise ContractError("--diagnostic-limit must be at least one")
        input_items = load_input_items(load_json(args.input))
        reference = load_reference_vectors(load_json(args.reference), len(input_items))
        candidates = [(label, load_json(path)) for label, path in args.candidate]
        report = build_report(input_items, reference, candidates, args.diagnostic_limit)
    except ContractError as error:
        print(f"conformance input error: {error}", file=sys.stderr)
        return 2

    rendered = json.dumps(report, indent=2, sort_keys=True, allow_nan=False) + "\n"
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
