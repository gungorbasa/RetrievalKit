#!/usr/bin/env python3
"""Inspect pinned public graph-quality candidates without leaking judgments.

All downloads and generated reports belong under the ignored
``target/benchmarks/public-collections`` directory.  The HotpotQA corpus
planner accepts source-only query objects and processed Wikipedia rows.  Gold
answers and supporting facts are parsed by a separate post-freeze function.
"""

from __future__ import annotations

import argparse
import bz2
import hashlib
import html
import json
import re
import shutil
import sys
import unicodedata
import urllib.parse
import urllib.request
import zipfile
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, BinaryIO, Iterable, Iterator, Mapping, Sequence


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CACHE = ROOT / "target/benchmarks/public-collections"
HOTPOT_SELECTION_SALT = "vectorkit-hotpotqa-linked-abstracts-v1"
HOTPOT_TRAIN_LIMIT = 2_000
HOTPOT_REPORTING_LIMIT = 1_000
HOTPOT_NEIGHBOR_LIMIT = 15
MAX_CORPUS_RECORDS = (HOTPOT_TRAIN_LIMIT + HOTPOT_REPORTING_LIMIT) * (
    HOTPOT_NEIGHBOR_LIMIT + 1
)
HREF_RE = re.compile(r'<a\s+[^>]*href="([^"]+)"', re.IGNORECASE)


@dataclass(frozen=True)
class Artifact:
    filename: str
    url: str
    bytes: int
    sha256: str
    md5: str | None = None


HOTPOT_ARTIFACTS = (
    Artifact(
        filename="enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2",
        url=(
            "https://nlp.stanford.edu/projects/hotpotqa/"
            "enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2"
        ),
        bytes=1_553_565_403,
        sha256="1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729",
        md5="01edf64cd120ecc03a2745352779514c",
    ),
    Artifact(
        filename="hotpotqa-distractor-train-00000-1908d6af.parquet",
        url=(
            "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/"
            "1908d6afbbead072334abe2965f91bd2709910ab/"
            "distractor/train-00000-of-00002.parquet?download=true"
        ),
        bytes=165_624_177,
        sha256="76d3bb3048a7cc73c1958107c0c5872a00d7e7d00c105b81e92f6769e7822e68",
    ),
    Artifact(
        filename="hotpotqa-distractor-train-00001-1908d6af.parquet",
        url=(
            "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/"
            "1908d6afbbead072334abe2965f91bd2709910ab/"
            "distractor/train-00001-of-00002.parquet?download=true"
        ),
        bytes=166_162_479,
        sha256="713661628434fbb19fff7392e2e321e4ed107e3c7c7784d0690946e5f722763f",
    ),
    Artifact(
        filename="hotpotqa-distractor-validation-1908d6af.parquet",
        url=(
            "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/"
            "1908d6afbbead072334abe2965f91bd2709910ab/"
            "distractor/validation-00000-of-00001.parquet?download=true"
        ),
        bytes=27_452_575,
        sha256="c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6",
    ),
)

TWOWIKI_ARTIFACTS = (
    Artifact(
        filename="data_ids_april7.zip",
        url=(
            "https://www.dropbox.com/scl/fi/32t7pv1dyf3o2pp0dl25u/"
            "data_ids_april7.zip?rlkey=u868q6h0jojw4djjg7ea65j46&dl=1"
        ),
        bytes=258_968_175,
        sha256="95df2bf56fdabe034e27aebc580e02264232203cf52552f9efe8a919e5529eef",
        md5="cbbd9f09448eae46b172929292e06471",
    ),
    Artifact(
        filename="para_with_hyperlink.zip",
        url=(
            "https://www.dropbox.com/scl/fi/p6xcpt4a7wxzqsa58kkko/"
            "para_with_hyperlink.zip?rlkey=tzei8xc346a8e2dx8h934p7t1&dl=1"
        ),
        bytes=1_900_740_270,
        sha256="a585bdc3c39425446e4b2701a5f7f30051cb6c100d179322055d53dc0a71a723",
        md5="21ed32d980cd93a09e96ace91c219c8c",
    ),
)

EXPECTED_TWOWIKI_MEMBERS = {
    "dev.json": 57_614_142,
    "id_aliases.json": 17_501_406,
    "test.json": 53_838_398,
    "train.json": 707_810_660,
}
EXPECTED_TWOWIKI_SPLITS = {"dev": 12_576, "test": 12_576, "train": 167_454}
EXPECTED_TWOWIKI_DATASET_COUNTS_SHA256 = (
    "cdcbfc9f67507bdaab8a3f42164c0b591a8a0f9b39436a2a9522c4fad47fd3cd"
)
EXPECTED_TWOWIKI_GLOBAL_CORPUS = {
    "conflicting_titles": 2,
    "duplicate_ids": 0,
    "mentions": 35_724_975,
    "mentions_without_ref_ids": 6_194_218,
    "records": 5_989_847,
    "sentences": 23_245_689,
    "unique_ids": 5_989_847,
    "unique_titles": 5_989_845,
}
EXPECTED_HOTPOT_SPLITS = {"dev_distractor": 7_405, "train": 90_447}
EXPECTED_HOTPOT_SOURCE_COUNTS = {
    "abstract_conflicting_titles": 2_619,
    "abstract_records": 5_233_329,
    "abstract_unique_titles": 5_230_693,
    "dev_distractor": 7_405,
    "train": 90_447,
}
EXPECTED_HOTPOT_CORPUS_COUNTS = {
    "chunks": 12_670,
    "directed_edges": 43_737,
    "maximum_records": 48_000,
    "preimage_sha256": "a59dd4edc535abde55d27aa8262d64b99d7a25c05754cd0724fef5216c5204c6",
    "records": 12_670,
    "selected_conflicting_titles": 59,
    "selected_missing_titles": 1_776,
}
EXPECTED_HOTPOT_SEED_RESOLUTION = {
    "ambiguous": 235,
    "no_match": 2,
    "resolved": 2_763,
}
EXPECTED_HOTPOT_ROW_FIELDS = {
    "answer",
    "context",
    "id",
    "level",
    "question",
    "supporting_facts",
    "type",
}
EXPECTED_HOTPOT_STRUCTURE = {
    "dev_distractor": {
        "conflicting_titles": 54,
        "contexts": 73_700,
        "questions": 7_405,
        "sentences": 306_487,
        "support_document_count_distribution": {2: 7_405},
        "supporting_sentences": 18_005,
        "unique_titles": 66_581,
    },
    "train": {
        "conflicting_titles": 1_675,
        "contexts": 899_667,
        "questions": 90_447,
        "sentences": 3_703_344,
        "support_document_count_distribution": {2: 90_447},
        "supporting_sentences": 215_684,
        "unique_titles": 482_021,
    },
}
EXPECTED_TWOWIKI_ROW_FIELDS = {
    "_id",
    "answer",
    "answer_id",
    "context",
    "entity_ids",
    "evidences",
    "evidences_id",
    "question",
    "supporting_facts",
    "type",
}
EXPECTED_TWOWIKI_CORPUS_FIELDS = {"id", "mentions", "sentences", "title"}
EXPECTED_TWOWIKI_MENTION_FIELDS = {
    "end",
    "id",
    "ref_ids",
    "ref_url",
    "sent_idx",
    "start",
}


class InspectionError(RuntimeError):
    """A pinned source or deterministic inspection invariant failed."""


@dataclass(frozen=True)
class SourceQuery:
    query_id: str
    split: str
    text: str
    category: str
    level: str


@dataclass(frozen=True)
class Judgment:
    query_id: str
    answer: str
    supporting_facts: tuple[tuple[str, int], ...]

    @property
    def supporting_titles(self) -> tuple[str, ...]:
        return tuple(sorted({title for title, _ in self.supporting_facts}))


@dataclass(frozen=True)
class SeedCandidate:
    record_id: str
    source_id: str
    title: str
    normalized_title: str
    outgoing_aliases: tuple[str, ...]


@dataclass(frozen=True)
class SeedResolution:
    query_id: str
    status: str
    candidates: tuple[str, ...]
    selected_record_id: str | None
    selected_title: str | None


@dataclass(frozen=True)
class CorpusRecord:
    record_id: str
    source_id: str
    title: str
    text: str
    outgoing_record_ids: tuple[str, ...]


@dataclass(frozen=True)
class FrozenCorpus:
    records: tuple[CorpusRecord, ...]
    resolutions: tuple[SeedResolution, ...]
    preimage_sha256: str
    source_conflicting_titles: int
    source_records: int
    source_unique_titles: int
    selected_conflicting_titles: int
    selected_missing_titles: int

    @property
    def normalized_title_to_record_id(self) -> dict[str, str]:
        return {normalize(row.title): row.record_id for row in self.records}


def canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_digest(path: Path, algorithm: str = "sha256") -> str:
    digest = hashlib.new(algorithm)
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_artifact(path: Path, artifact: Artifact) -> None:
    if not path.is_file():
        raise InspectionError(f"missing pinned artifact: {path}")
    actual_bytes = path.stat().st_size
    if actual_bytes != artifact.bytes:
        raise InspectionError(
            f"byte-count mismatch for {path}: expected {artifact.bytes}, "
            f"found {actual_bytes}"
        )
    actual_sha256 = file_digest(path)
    if actual_sha256 != artifact.sha256:
        raise InspectionError(
            f"SHA-256 mismatch for {path}: expected {artifact.sha256}, "
            f"found {actual_sha256}"
        )
    if artifact.md5 is not None:
        actual_md5 = file_digest(path, "md5")
        if actual_md5 != artifact.md5:
            raise InspectionError(
                f"MD5 mismatch for {path}: expected {artifact.md5}, "
                f"found {actual_md5}"
            )


def download_artifact(downloads: Path, artifact: Artifact) -> Path:
    downloads.mkdir(parents=True, exist_ok=True)
    destination = downloads / artifact.filename
    if destination.exists():
        verify_artifact(destination, artifact)
        return destination
    temporary = destination.with_suffix(destination.suffix + ".download")
    temporary.unlink(missing_ok=True)
    try:
        with urllib.request.urlopen(artifact.url, timeout=120) as response:
            with temporary.open("wb") as target:
                shutil.copyfileobj(response, target)
        verify_artifact(temporary, artifact)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    return destination


def normalize(value: str) -> str:
    folded = unicodedata.normalize("NFC", value).casefold()
    collapsed = " ".join(folded.split())
    return unicodedata.normalize("NFC", collapsed)


def is_alnum(value: str) -> bool:
    return bool(value) and (value[0].isalpha() or value[0].isnumeric())


def boundary_offsets(value: str) -> tuple[int, ...]:
    offsets = [0]
    for offset in range(1, len(value)):
        if is_alnum(value[offset - 1]) != is_alnum(value[offset]):
            offsets.append(offset)
    offsets.append(len(value))
    return tuple(offsets)


def alias_substrings(value: str) -> set[str]:
    normalized = normalize(value)
    offsets = boundary_offsets(normalized)
    return {
        normalized[start:end]
        for start in offsets[:-1]
        for end in offsets[1:]
        if start < end and normalized[start:end].strip()
    }


def parse_hotpot_source_row(row: Mapping[str, Any], split: str) -> SourceQuery:
    """Read only source fields; gold fields are deliberately ignored."""
    required = {"id", "question", "type", "level"}
    missing = sorted(required - row.keys())
    if missing:
        raise InspectionError(f"HotpotQA source row lacks fields: {missing}")
    query = SourceQuery(
        query_id=str(row["id"]),
        split=split,
        text=str(row["question"]),
        category=str(row["type"]),
        level=str(row["level"]),
    )
    if not query.query_id or not query.text:
        raise InspectionError("HotpotQA source row has an empty ID or question")
    return query


def parse_hotpot_judgment_row(row: Mapping[str, Any]) -> Judgment:
    """Parse gold fields after corpus freeze; source construction never calls this."""
    supporting = row.get("supporting_facts")
    if not isinstance(supporting, Mapping):
        raise InspectionError("HotpotQA judgment lacks supporting_facts")
    titles = supporting.get("title")
    sentence_ids = supporting.get("sent_id")
    if not isinstance(titles, list) or not isinstance(sentence_ids, list):
        raise InspectionError("HotpotQA supporting_facts has the wrong shape")
    if len(titles) != len(sentence_ids):
        raise InspectionError("HotpotQA supporting-fact arrays have different lengths")
    facts = tuple((str(title), int(sentence_id)) for title, sentence_id in zip(titles, sentence_ids))
    if len(set(facts)) != len(facts):
        raise InspectionError("HotpotQA judgment repeats a supporting sentence")
    return Judgment(
        query_id=str(row["id"]),
        answer=str(row.get("answer", "")),
        supporting_facts=facts,
    )


def sampled_source_queries(
    rows: Iterable[Mapping[str, Any]], split: str, limit: int
) -> tuple[SourceQuery, ...]:
    parsed = [parse_hotpot_source_row(row, split) for row in rows]
    if len({row.query_id for row in parsed}) != len(parsed):
        raise InspectionError(f"HotpotQA {split} contains duplicate query IDs")

    def sample_key(query: SourceQuery) -> tuple[bytes, str]:
        preimage = f"{HOTPOT_SELECTION_SALT}\0{split}\0{query.query_id}".encode()
        return hashlib.sha256(preimage).digest(), query.query_id

    return tuple(sorted(parsed, key=sample_key)[:limit])


def iter_hotpot_abstracts(abstracts_dir: Path) -> Iterator[dict[str, Any]]:
    members = sorted(abstracts_dir.rglob("*.bz2"), key=lambda path: path.as_posix().encode())
    if len(members) != 15_517:
        raise InspectionError(
            f"HotpotQA abstract shard count mismatch: expected 15517, found {len(members)}"
        )
    for member in members:
        with bz2.open(member, "rt", encoding="utf-8") as source:
            for line_number, line in enumerate(source, 1):
                try:
                    row = json.loads(line)
                except json.JSONDecodeError as error:
                    raise InspectionError(f"invalid JSON at {member}:{line_number}") from error
                required = {
                    "charoffset",
                    "charoffset_with_links",
                    "id",
                    "text",
                    "text_with_links",
                    "title",
                    "url",
                }
                if set(row) != required:
                    raise InspectionError(
                        f"HotpotQA abstract schema mismatch at {member}:{line_number}"
                    )
                arrays = (
                    row["charoffset"],
                    row["charoffset_with_links"],
                    row["text"],
                    row["text_with_links"],
                )
                if not all(isinstance(value, list) for value in arrays) or len(
                    row["text"]
                ) != len(row["text_with_links"]):
                    raise InspectionError(
                        f"HotpotQA abstract array mismatch at {member}:{line_number}"
                    )
                yield row


def outgoing_aliases(row: Mapping[str, Any]) -> tuple[str, ...]:
    aliases: set[str] = set()
    for sentence in row["text_with_links"]:
        for encoded in HREF_RE.findall(str(sentence)):
            decoded = html.unescape(urllib.parse.unquote(encoded)).replace("_", " ")
            decoded = decoded.split("#", 1)[0]
            normalized = normalize(decoded)
            if normalized:
                aliases.add(normalized)
    return tuple(sorted(aliases, key=lambda value: value.encode("utf-8")))


def record_id(source_id: str) -> str:
    if not source_id.isdecimal():
        raise InspectionError(f"HotpotQA Wikipedia ID is not decimal: {source_id!r}")
    return f"hotpotqa:wiki:{source_id}"


def resolve_seed_candidates(
    source_queries: Sequence[SourceQuery],
    candidates: Mapping[str, Sequence[SeedCandidate]],
) -> tuple[SeedResolution, ...]:
    resolutions: list[SeedResolution] = []
    for query in sorted(source_queries, key=lambda row: row.query_id.encode("utf-8")):
        rows = list(candidates.get(query.query_id, ()))
        if not rows:
            resolutions.append(SeedResolution(query.query_id, "no_match", (), None, None))
            continue
        longest = max(len(row.normalized_title) for row in rows)
        longest_rows = [row for row in rows if len(row.normalized_title) == longest]
        by_record = {row.record_id: row for row in longest_rows}
        candidate_ids = tuple(sorted(by_record, key=lambda value: value.encode("utf-8")))
        if len(candidate_ids) != 1:
            resolutions.append(
                SeedResolution(query.query_id, "ambiguous", candidate_ids, None, None)
            )
            continue
        selected = by_record[candidate_ids[0]]
        resolutions.append(
            SeedResolution(
                query.query_id,
                "resolved",
                candidate_ids,
                selected.record_id,
                selected.title,
            )
        )
    return tuple(resolutions)


def freeze_hotpot_corpus(
    source_queries: Sequence[SourceQuery], abstracts_dir: Path
) -> FrozenCorpus:
    """Freeze a corpus using source queries and corpus rows only.

    The function has no judgment argument and never opens a query/judgment file.
    """
    query_substrings: dict[str, list[str]] = defaultdict(list)
    for query in source_queries:
        for alias in alias_substrings(query.text):
            query_substrings[alias].append(query.query_id)
    for query_ids in query_substrings.values():
        query_ids.sort(key=lambda value: value.encode("utf-8"))

    candidates: dict[str, list[SeedCandidate]] = defaultdict(list)
    records_seen = 0
    source_ids: set[str] = set()
    raw_titles: dict[str, bytes] = {}
    conflicting_titles: set[str] = set()
    for row in iter_hotpot_abstracts(abstracts_dir):
        records_seen += 1
        source_id = str(row["id"])
        if source_id in source_ids:
            raise InspectionError(f"HotpotQA abstracts repeat source ID {source_id}")
        source_ids.add(source_id)
        title = str(row["title"])
        normalized_title = normalize(title)
        text_digest = hashlib.sha256("".join(row["text"]).encode()).digest()
        previous = raw_titles.setdefault(normalized_title, text_digest)
        if previous != text_digest:
            conflicting_titles.add(normalized_title)
        matching_queries = query_substrings.get(normalized_title)
        if not matching_queries:
            continue
        candidate = SeedCandidate(
            record_id=record_id(source_id),
            source_id=source_id,
            title=title,
            normalized_title=normalized_title,
            outgoing_aliases=outgoing_aliases(row)[:HOTPOT_NEIGHBOR_LIMIT],
        )
        for query_id in matching_queries:
            candidates[query_id].append(candidate)

    resolutions = resolve_seed_candidates(source_queries, candidates)
    by_record = {
        candidate.record_id: candidate
        for rows in candidates.values()
        for candidate in rows
    }
    selected_aliases: set[str] = set()
    for resolution in resolutions:
        if resolution.selected_record_id is None:
            continue
        selected = by_record[resolution.selected_record_id]
        selected_aliases.add(selected.normalized_title)
        selected_aliases.update(selected.outgoing_aliases)
    if len(selected_aliases) > MAX_CORPUS_RECORDS:
        raise InspectionError(
            f"label-blind corpus bound exceeded: {len(selected_aliases)} > {MAX_CORPUS_RECORDS}"
        )

    selected_rows: dict[str, dict[str, Any]] = {}
    selected_conflicts: set[str] = set()
    for row in iter_hotpot_abstracts(abstracts_dir):
        normalized_title = normalize(str(row["title"]))
        if normalized_title not in selected_aliases:
            continue
        previous = selected_rows.get(normalized_title)
        if previous is None:
            selected_rows[normalized_title] = row
            continue
        previous_text = "".join(previous["text"])
        current_text = "".join(row["text"])
        if previous_text != current_text:
            selected_conflicts.add(normalized_title)
        if int(str(row["id"])) < int(str(previous["id"])):
            selected_rows[normalized_title] = row

    missing = sorted(selected_aliases - selected_rows.keys(), key=lambda value: value.encode())
    alias_to_record = {
        alias: record_id(str(row["id"])) for alias, row in selected_rows.items()
    }
    records: list[CorpusRecord] = []
    for alias, row in selected_rows.items():
        outgoing = {
            alias_to_record[target]
            for target in outgoing_aliases(row)
            if target in alias_to_record and target != alias
        }
        records.append(
            CorpusRecord(
                record_id=alias_to_record[alias],
                source_id=str(row["id"]),
                title=str(row["title"]),
                text="".join(row["text"]),
                outgoing_record_ids=tuple(sorted(outgoing, key=lambda value: value.encode())),
            )
        )
    records.sort(key=lambda row: row.record_id.encode("utf-8"))
    if len(records) >= 50_000:
        raise InspectionError(f"HotpotQA corpus is not under 50K records: {len(records)}")
    preimage = {
        "conflict_policy": "lowest numeric Wikipedia page ID",
        "neighbor_limit": HOTPOT_NEIGHBOR_LIMIT,
        "records": [asdict(row) for row in records],
        "records_seen": records_seen,
        "sample_salt": HOTPOT_SELECTION_SALT,
        "selected_conflicting_titles": sorted(selected_conflicts),
        "selected_missing_titles": missing,
        "source_conflicting_titles": len(conflicting_titles),
    }
    return FrozenCorpus(
        records=tuple(records),
        resolutions=resolutions,
        preimage_sha256=sha256_bytes(canonical_bytes(preimage)),
        source_conflicting_titles=len(conflicting_titles),
        source_records=records_seen,
        source_unique_titles=len(raw_titles),
        selected_conflicting_titles=len(selected_conflicts),
        selected_missing_titles=len(missing),
    )


def post_freeze_eligibility(
    corpus: FrozenCorpus,
    judgments: Sequence[Judgment],
) -> tuple[tuple[str, ...], dict[str, int]]:
    """Apply gold evidence only after ``FrozenCorpus`` already exists."""
    available = corpus.normalized_title_to_record_id
    eligible: list[str] = []
    reasons: Counter[str] = Counter()
    for judgment in sorted(judgments, key=lambda row: row.query_id.encode("utf-8")):
        titles = judgment.supporting_titles
        if not titles:
            reasons["missing_complete_evidence"] += 1
        elif all(normalize(title) in available for title in titles):
            eligible.append(judgment.query_id)
        else:
            reasons["not_in_frozen_corpus"] += 1
    return tuple(eligible), dict(sorted(reasons.items()))


def population_hash(query_ids: Iterable[str]) -> str:
    ordered = sorted(query_ids, key=lambda value: value.encode("utf-8"))
    return sha256_bytes("".join(f"{query_id}\n" for query_id in ordered).encode())


def read_hotpot_parquet_rows(paths: Sequence[Path]) -> list[dict[str, Any]]:
    try:
        import pyarrow.parquet as parquet
    except ImportError as error:
        raise InspectionError(
            "HotpotQA inspection requires pyarrow in an evaluation-only environment"
        ) from error
    rows: list[dict[str, Any]] = []
    for path in paths:
        parquet_file = parquet.ParquetFile(path)
        for batch in parquet_file.iter_batches(batch_size=512):
            rows.extend(batch.to_pylist())
    return rows


def inspect_hotpot_rows(rows: Sequence[Mapping[str, Any]], split: str) -> dict[str, object]:
    counts: Counter[str] = Counter()
    support_documents: Counter[int] = Counter()
    titles: dict[str, bytes] = {}
    conflicting_titles: set[str] = set()
    for row in rows:
        if set(row) != EXPECTED_HOTPOT_ROW_FIELDS:
            raise InspectionError(f"HotpotQA {split} row schema mismatch")
        counts["questions"] += 1
        context = row["context"]
        context_titles = context.get("title")
        context_sentences = context.get("sentences")
        if not isinstance(context_titles, list) or not isinstance(context_sentences, list):
            raise InspectionError(f"HotpotQA {split} context shape mismatch")
        if len(context_titles) != len(context_sentences):
            raise InspectionError(f"HotpotQA {split} context arrays differ")
        counts["contexts"] += len(context_titles)
        for title, sentences in zip(context_titles, context_sentences):
            if not isinstance(sentences, list):
                raise InspectionError(f"HotpotQA {split} sentence shape mismatch")
            digest = hashlib.sha256("".join(sentences).encode()).digest()
            source_title = str(title)
            previous = titles.setdefault(source_title, digest)
            if previous != digest:
                conflicting_titles.add(source_title)
            counts["sentences"] += len(sentences)
        judgment = parse_hotpot_judgment_row(row)
        counts["supporting_sentences"] += len(judgment.supporting_facts)
        support_documents[len(judgment.supporting_titles)] += 1
    result: dict[str, object] = {
        **dict(sorted(counts.items())),
        "conflicting_titles": len(conflicting_titles),
        "support_document_count_distribution": dict(sorted(support_documents.items())),
        "unique_titles": len(titles),
    }
    if result != EXPECTED_HOTPOT_STRUCTURE[split]:
        raise InspectionError(f"HotpotQA {split} structure mismatch: {result}")
    return result


def iter_json_array(stream: BinaryIO, chunk_size: int = 1024 * 1024) -> Iterator[Any]:
    decoder = json.JSONDecoder()
    buffer = ""
    position = 0
    started = False
    while True:
        block = stream.read(chunk_size)
        if block:
            buffer += block.decode("utf-8")
        while True:
            while position < len(buffer) and buffer[position].isspace():
                position += 1
            if not started:
                if position >= len(buffer):
                    break
                if buffer[position] != "[":
                    raise InspectionError("JSON source is not a top-level array")
                position += 1
                started = True
                continue
            while position < len(buffer) and (
                buffer[position].isspace() or buffer[position] == ","
            ):
                position += 1
            if position >= len(buffer):
                break
            if buffer[position] == "]":
                return
            try:
                value, end = decoder.raw_decode(buffer, position)
            except json.JSONDecodeError:
                break
            yield value
            position = end
        if position:
            buffer = buffer[position:]
            position = 0
        if not block:
            raise InspectionError("unexpected end of JSON array")


def inspect_2wiki_dataset(path: Path) -> dict[str, object]:
    verify_artifact(path, TWOWIKI_ARTIFACTS[0])
    report: dict[str, object] = {}
    all_titles: dict[str, bytes] = {}
    all_query_ids: set[str] = set()
    with zipfile.ZipFile(path) as archive:
        members = {
            row.filename: row.file_size for row in archive.infolist() if not row.is_dir()
        }
        if members != EXPECTED_TWOWIKI_MEMBERS:
            raise InspectionError(f"2Wiki dataset member mismatch: {members}")
        for split in ("train", "dev", "test"):
            counts: Counter[str] = Counter()
            types: Counter[str] = Counter()
            support_documents: Counter[int] = Counter()
            evidence_lengths: Counter[int] = Counter()
            split_titles: dict[str, bytes] = {}
            with archive.open(f"{split}.json") as source:
                for row in iter_json_array(source):
                    if set(row) != EXPECTED_TWOWIKI_ROW_FIELDS:
                        raise InspectionError(f"2Wiki {split} row schema mismatch")
                    counts["questions"] += 1
                    query_id = str(row["_id"])
                    if query_id in all_query_ids:
                        raise InspectionError(f"2Wiki repeats query ID {query_id}")
                    all_query_ids.add(query_id)
                    types[str(row["type"])] += 1
                    context = row["context"]
                    counts["contexts"] += len(context)
                    for title, sentences in context:
                        digest = hashlib.sha256("".join(sentences).encode()).digest()
                        previous = split_titles.setdefault(str(title), digest)
                        if previous != digest:
                            counts["conflicting_titles"] += 1
                        all_previous = all_titles.setdefault(str(title), digest)
                        if all_previous != digest:
                            counts["cross_split_conflicting_titles"] += 1
                        counts["sentences"] += len(sentences)
                    facts = row.get("supporting_facts", [])
                    evidences = row.get("evidences", [])
                    counts["supporting_sentences"] += len(facts)
                    counts["evidence_triples"] += len(evidences)
                    support_documents[len({fact[0] for fact in facts})] += 1
                    evidence_lengths[len(evidences)] += 1
                    counts["public_answers"] += int(bool(row.get("answer", "")))
            if counts["questions"] != EXPECTED_TWOWIKI_SPLITS[split]:
                raise InspectionError(
                    f"2Wiki {split} count mismatch: {counts['questions']}"
                )
            report[split] = {
                **dict(sorted(counts.items())),
                "evidence_length_distribution": dict(sorted(evidence_lengths.items())),
                "support_document_count_distribution": dict(
                    sorted(support_documents.items())
                ),
                "types": dict(sorted(types.items())),
                "unique_titles": len(split_titles),
            }
    counts_report = {
        "all_unique_titles": len(all_titles),
        "splits": report,
    }
    actual_counts_sha256 = sha256_bytes(canonical_bytes(counts_report))
    if actual_counts_sha256 != EXPECTED_TWOWIKI_DATASET_COUNTS_SHA256:
        raise InspectionError(
            "2Wiki dataset-count digest mismatch: "
            f"expected {EXPECTED_TWOWIKI_DATASET_COUNTS_SHA256}, "
            f"found {actual_counts_sha256}"
        )
    return {
        "all_unique_titles": len(all_titles),
        "archive": asdict(TWOWIKI_ARTIFACTS[0]),
        "splits": report,
    }


def inspect_2wiki_corpus(path: Path) -> dict[str, int]:
    verify_artifact(path, TWOWIKI_ARTIFACTS[1])
    ids: set[str] = set()
    titles: dict[str, bytes] = {}
    conflicting_titles: set[str] = set()
    counts: Counter[str] = Counter()
    with zipfile.ZipFile(path) as archive:
        members = [row for row in archive.infolist() if not row.is_dir()]
        if len(members) != 1 or members[0].filename != "para_with_hyperlink.jsonl":
            raise InspectionError("2Wiki hyperlink archive has unexpected members")
        if members[0].file_size != 7_023_046_781:
            raise InspectionError("2Wiki hyperlink JSONL uncompressed size mismatch")
        with archive.open(members[0]) as source:
            for line in source:
                row = json.loads(line)
                if set(row) != EXPECTED_TWOWIKI_CORPUS_FIELDS:
                    raise InspectionError("2Wiki hyperlink-corpus row schema mismatch")
                if any(set(mention) != EXPECTED_TWOWIKI_MENTION_FIELDS for mention in row["mentions"]):
                    raise InspectionError("2Wiki hyperlink mention schema mismatch")
                counts["records"] += 1
                source_id = str(row["id"])
                if source_id in ids:
                    counts["duplicate_ids"] += 1
                ids.add(source_id)
                title = str(row["title"])
                digest = hashlib.sha256("".join(row["sentences"]).encode()).digest()
                previous = titles.setdefault(title, digest)
                if previous != digest:
                    conflicting_titles.add(title)
                counts["sentences"] += len(row["sentences"])
                counts["mentions"] += len(row["mentions"])
                counts["mentions_without_ref_ids"] += sum(
                    not mention.get("ref_ids") for mention in row["mentions"]
                )
    counts["unique_ids"] = len(ids)
    counts["unique_titles"] = len(titles)
    counts["conflicting_titles"] = len(conflicting_titles)
    counts["duplicate_ids"] += 0
    result = dict(sorted(counts.items()))
    if result != EXPECTED_TWOWIKI_GLOBAL_CORPUS:
        raise InspectionError(f"2Wiki hyperlink-corpus count mismatch: {result}")
    return result


def hotpot_report(downloads: Path, abstracts_dir: Path) -> dict[str, object]:
    for artifact in HOTPOT_ARTIFACTS:
        verify_artifact(downloads / artifact.filename, artifact)
    train_paths = [downloads / artifact.filename for artifact in HOTPOT_ARTIFACTS[1:3]]
    dev_paths = [downloads / HOTPOT_ARTIFACTS[3].filename]
    train_rows = read_hotpot_parquet_rows(train_paths)
    dev_rows = read_hotpot_parquet_rows(dev_paths)
    if len(train_rows) != EXPECTED_HOTPOT_SPLITS["train"]:
        raise InspectionError(f"HotpotQA train count mismatch: {len(train_rows)}")
    if len(dev_rows) != EXPECTED_HOTPOT_SPLITS["dev_distractor"]:
        raise InspectionError(f"HotpotQA dev count mismatch: {len(dev_rows)}")
    upstream_structure = {
        "dev_distractor": inspect_hotpot_rows(dev_rows, "dev_distractor"),
        "train": inspect_hotpot_rows(train_rows, "train"),
    }

    train_sources = sampled_source_queries(train_rows, "train", HOTPOT_TRAIN_LIMIT)
    dev_sources = sampled_source_queries(
        dev_rows, "dev_distractor", HOTPOT_REPORTING_LIMIT
    )
    sources = train_sources + dev_sources
    corpus = freeze_hotpot_corpus(sources, abstracts_dir)
    by_id = {str(row["id"]): row for row in train_rows + dev_rows}
    train_judgments = [
        parse_hotpot_judgment_row(by_id[row.query_id]) for row in train_sources
    ]
    dev_judgments = [
        parse_hotpot_judgment_row(by_id[row.query_id]) for row in dev_sources
    ]
    eligible_train, train_exclusions = post_freeze_eligibility(corpus, train_judgments)
    eligible_dev, dev_exclusions = post_freeze_eligibility(corpus, dev_judgments)
    resolution_by_id = {row.query_id: row for row in corpus.resolutions}
    resolved_ids = {
        query_id
        for query_id, resolution in resolution_by_id.items()
        if resolution.status == "resolved"
    }
    derived_train = tuple(query_id for query_id in eligible_train if query_id in resolved_ids)
    derived_dev = tuple(query_id for query_id in eligible_dev if query_id in resolved_ids)
    derived_train_exclusions = Counter(
        resolution_by_id[query_id].status
        for query_id in eligible_train
        if query_id not in resolved_ids
    )
    derived_dev_exclusions = Counter(
        resolution_by_id[query_id].status
        for query_id in eligible_dev
        if query_id not in resolved_ids
    )
    resolution_counts = Counter(row.status for row in corpus.resolutions)
    edge_count = sum(len(row.outgoing_record_ids) for row in corpus.records)
    report = {
        "artifacts": [asdict(artifact) for artifact in HOTPOT_ARTIFACTS],
        "corpus": {
            "chunks": len(corpus.records),
            "directed_edges": edge_count,
            "maximum_records": MAX_CORPUS_RECORDS,
            "preimage_sha256": corpus.preimage_sha256,
            "records": len(corpus.records),
            "selected_conflicting_titles": corpus.selected_conflicting_titles,
            "selected_missing_titles": corpus.selected_missing_titles,
        },
        "development": {
            "derived_seed_eligible": len(derived_train),
            "derived_seed_excluded": len(eligible_train) - len(derived_train),
            "derived_seed_exclusion_reasons": dict(
                sorted(derived_train_exclusions.items())
            ),
            "derived_seed_population_sha256": population_hash(derived_train),
            "eligible": len(eligible_train),
            "exclusions": train_exclusions,
            "population_sha256": population_hash(eligible_train),
            "sampled": len(train_sources),
        },
        "reporting": {
            "derived_seed_eligible": len(derived_dev),
            "derived_seed_excluded": len(eligible_dev) - len(derived_dev),
            "derived_seed_exclusion_reasons": dict(
                sorted(derived_dev_exclusions.items())
            ),
            "derived_seed_population_sha256": population_hash(derived_dev),
            "eligible": len(eligible_dev),
            "exclusions": dev_exclusions,
            "population_sha256": population_hash(eligible_dev),
            "sampled": len(dev_sources),
        },
        "seed_resolution": dict(sorted(resolution_counts.items())),
        "source_counts": {
            "abstract_conflicting_titles": corpus.source_conflicting_titles,
            "abstract_records": corpus.source_records,
            "abstract_unique_titles": corpus.source_unique_titles,
            "dev_distractor": len(dev_rows),
            "train": len(train_rows),
        },
        "upstream_structure": upstream_structure,
    }
    if report["corpus"] != EXPECTED_HOTPOT_CORPUS_COUNTS:
        raise InspectionError(f"HotpotQA frozen-corpus mismatch: {report['corpus']}")
    if report["seed_resolution"] != EXPECTED_HOTPOT_SEED_RESOLUTION:
        raise InspectionError(
            f"HotpotQA seed-resolution mismatch: {report['seed_resolution']}"
        )
    if report["source_counts"] != EXPECTED_HOTPOT_SOURCE_COUNTS:
        raise InspectionError(f"HotpotQA source-count mismatch: {report['source_counts']}")
    expected_populations = {
        "development": (
            603,
            {"not_in_frozen_corpus": 1_397},
            "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f",
        ),
        "reporting": (
            297,
            {"not_in_frozen_corpus": 703},
            "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010",
        ),
    }
    for split, (eligible, exclusions, digest) in expected_populations.items():
        actual = report[split]
        if (
            actual["eligible"] != eligible
            or actual["exclusions"] != exclusions
            or actual["population_sha256"] != digest
        ):
            raise InspectionError(f"HotpotQA {split} population mismatch: {actual}")
    return report


def write_report(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_bytes(value) + b"\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache-dir", type=Path, default=DEFAULT_CACHE)
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify = subparsers.add_parser("verify-sources")
    verify.add_argument("--download", action="store_true")
    hotpot = subparsers.add_parser("inspect-hotpotqa")
    hotpot.add_argument("--abstracts-dir", type=Path, required=True)
    hotpot.add_argument("--output", type=Path, required=True)
    wiki = subparsers.add_parser("inspect-2wiki")
    wiki.add_argument("--include-global-corpus", action="store_true")
    wiki.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    downloads = args.cache_dir / "downloads"
    if args.command == "verify-sources":
        artifacts = HOTPOT_ARTIFACTS + TWOWIKI_ARTIFACTS
        for artifact in artifacts:
            if args.download:
                download_artifact(downloads, artifact)
            else:
                verify_artifact(downloads / artifact.filename, artifact)
        print(f"verified {len(artifacts)} pinned source artifacts")
        return
    if args.command == "inspect-hotpotqa":
        report = hotpot_report(downloads, args.abstracts_dir)
        write_report(args.output, report)
        print(f"wrote {args.output}")
        return
    if args.command == "inspect-2wiki":
        report = inspect_2wiki_dataset(downloads / TWOWIKI_ARTIFACTS[0].filename)
        if args.include_global_corpus:
            report["global_corpus"] = inspect_2wiki_corpus(
                downloads / TWOWIKI_ARTIFACTS[1].filename
            )
        write_report(args.output, report)
        print(f"wrote {args.output}")
        return
    raise InspectionError(f"unknown command: {args.command}")


if __name__ == "__main__":
    try:
        main()
    except InspectionError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
