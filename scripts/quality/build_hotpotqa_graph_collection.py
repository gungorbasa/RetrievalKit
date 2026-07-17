#!/usr/bin/env python3
"""Build the frozen HotpotQA linked-abstract V3 graph collections.

The source-only corpus functions in this module deliberately precede and are
separate from all judgment parsing.  Raw upstream data and generated outputs
belong below ignored ``target/`` paths and are never redistribution artifacts.
"""

from __future__ import annotations

import argparse
import bz2
import hashlib
import html
import json
import re
import sys
import tarfile
import unicodedata
import urllib.parse
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Callable, Iterable, Iterator, Mapping, Sequence


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CACHE = ROOT / "target/benchmarks/public-collections"
SAMPLE_SALT = "vectorkit-hotpotqa-linked-abstracts-v1"
TRAIN_SPLIT = "train"
DEV_SPLIT = "dev_distractor"
TRAIN_LIMIT = 2_000
DEV_LIMIT = 1_000
NEIGHBOR_LIMIT = 15
EXPECTED_ABSTRACT_ROWS = 5_233_329
EXPECTED_ABSTRACT_UNIQUE_TITLES = 5_230_693
EXPECTED_ABSTRACT_CONFLICTS = 2_619
EXPECTED_RECORDS = 12_670
EXPECTED_EDGES = 43_737
EXPECTED_MISSING_ALIASES = 1_776
EXPECTED_SELECTED_CONFLICTS = 59
EXPECTED_CORPUS_PREIMAGE_SHA256 = (
    "a59dd4edc535abde55d27aa8262d64b99d7a25c05754cd0724fef5216c5204c6"
)
EXPECTED_POPULATIONS = {
    "development": {
        "eligible": 603,
        "global_exclusions": 1_397,
        "population_sha256": "1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f",
        "derived_eligible": 599,
        "derived_exclusions": 4,
        "derived_population_sha256": "da343545fa764b44c5382f4a16c933dded7bd613ae6e12768b5c2772c6739582",
    },
    "test": {
        "eligible": 297,
        "global_exclusions": 703,
        "population_sha256": "9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010",
        "derived_eligible": 296,
        "derived_exclusions": 1,
        "derived_population_sha256": "93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8",
    },
}
EXPECTED_RESOLUTIONS = {"ambiguous": 235, "no_match": 2, "resolved": 2_763}
EMPTY_POPULATION_SHA256 = hashlib.sha256(b"").hexdigest()
DERIVED_POLICY_ID = "hotpotqa-exact-title-v1"
CORPUS_ID = "hotpotqa-linked-abstracts-corpus-v1"
COLLECTION_BASE_ID = "hotpotqa-linked-abstracts-graph-v1"
TRAVERSAL = {
    "limits": {
        "max_hops": 2,
        "max_results": 10_000,
        "max_visited": 100_000,
        "max_working_bytes": 67_108_864,
    },
    "steps": [
        {
            "direction": "outgoing",
            "max_hops": 2,
            "min_hops": 0,
            "relationship_type": "LinksTo",
        }
    ],
}
EXPECTED_SHARDS = 15_517
EXPECTED_ARCHIVE_MEMBERS = 15_674
EXPECTED_ARCHIVE_INVENTORY_SHA256 = (
    "e2c7b289c1ed0c7e11faabd9ef1b37bceeea1a997e3673657bdfee053c6450cf"
)
HREF_RE = re.compile(r'<a\s+[^>]*href="([^"]+)"', re.IGNORECASE)


class AdapterError(RuntimeError):
    """A frozen adapter invariant failed."""


@dataclass(frozen=True)
class Artifact:
    source_id: str
    filename: str
    url: str
    bytes: int
    sha256: str
    md5: str | None = None


ARTIFACTS = (
    Artifact(
        "upstream/corpus/hotpotqa-linked-abstracts-2019-01-14",
        "enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2",
        "https://nlp.stanford.edu/projects/hotpotqa/enwiki-20171001-pages-meta-current-withlinks-abstracts.tar.bz2",
        1_553_565_403,
        "1acca1c5cc93c4890ea51091d2bad7c3ef6987aead127ab88728dc9e26555729",
        "01edf64cd120ecc03a2745352779514c",
    ),
    Artifact(
        "upstream/query/hotpotqa-distractor-train-00000-1908d6af",
        "hotpotqa-distractor-train-00000-1908d6af.parquet",
        "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/train-00000-of-00002.parquet?download=true",
        165_624_177,
        "76d3bb3048a7cc73c1958107c0c5872a00d7e7d00c105b81e92f6769e7822e68",
    ),
    Artifact(
        "upstream/query/hotpotqa-distractor-train-00001-1908d6af",
        "hotpotqa-distractor-train-00001-1908d6af.parquet",
        "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/train-00001-of-00002.parquet?download=true",
        166_162_479,
        "713661628434fbb19fff7392e2e321e4ed107e3c7c7784d0690946e5f722763f",
    ),
    Artifact(
        "upstream/query/hotpotqa-distractor-validation-1908d6af",
        "hotpotqa-distractor-validation-1908d6af.parquet",
        "https://huggingface.co/datasets/hotpotqa/hotpot_qa/resolve/1908d6afbbead072334abe2965f91bd2709910ab/distractor/validation-00000-of-00001.parquet?download=true",
        27_452_575,
        "c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6",
    ),
)

ABSTRACT_FIELDS = {
    "charoffset",
    "charoffset_with_links",
    "id",
    "text",
    "text_with_links",
    "title",
    "url",
}
QUESTION_FIELDS = {
    "answer",
    "context",
    "id",
    "level",
    "question",
    "supporting_facts",
    "type",
}


@dataclass(frozen=True)
class SourceQuery:
    upstream_id: str
    split: str
    question_text: str
    query_type: str
    level: str


@dataclass(frozen=True)
class SeedCandidate:
    record_id: str
    source_id: str
    title: str
    normalized_title: str
    outgoing_aliases: tuple[str, ...]


@dataclass(frozen=True)
class SeedResolution:
    upstream_id: str
    status: str
    candidate_record_ids: tuple[str, ...]
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
    selected_conflicting_titles: tuple[str, ...]
    selected_missing_titles: tuple[str, ...]

    @property
    def normalized_title_to_record_id(self) -> dict[str, str]:
        return {normalize(record.title): record.record_id for record in self.records}


@dataclass(frozen=True)
class Judgment:
    upstream_id: str
    answer: str
    supporting_facts: tuple[tuple[str, int], ...]

    @property
    def supporting_titles(self) -> tuple[str, ...]:
        return tuple(sorted({title for title, _ in self.supporting_facts}, key=lexical))


@dataclass(frozen=True)
class SplitArtifacts:
    split: str
    queries: tuple[dict[str, Any], ...]
    qrels: tuple[tuple[str, str, int], ...]
    evidence: tuple[dict[str, Any], ...]
    exclusions: tuple[dict[str, Any], ...]
    population_sha256: str
    derived_population_sha256: str


def lexical(value: str) -> bytes:
    return value.encode("utf-8")


def canonical_bytes(value: object, *, final_lf: bool = False) -> bytes:
    data = json.dumps(
        value, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")
    return data + (b"\n" if final_lf else b"")


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
        raise AdapterError(f"missing pinned artifact: {path}")
    actual_bytes = path.stat().st_size
    if actual_bytes != artifact.bytes:
        raise AdapterError(
            f"source byte-count mismatch for {path}: expected {artifact.bytes}, found {actual_bytes}"
        )
    actual_sha256 = file_digest(path)
    if actual_sha256 != artifact.sha256:
        raise AdapterError(
            f"source SHA-256 mismatch for {path}: expected {artifact.sha256}, found {actual_sha256}"
        )
    if artifact.md5 is not None:
        actual_md5 = file_digest(path, "md5")
        if actual_md5 != artifact.md5:
            raise AdapterError(
                f"source MD5 mismatch for {path}: expected {artifact.md5}, found {actual_md5}"
            )


def verify_source_artifacts(cache_dir: Path) -> tuple[Path, ...]:
    downloads = cache_dir / "downloads"
    paths = tuple(downloads / artifact.filename for artifact in ARTIFACTS)
    for path, artifact in zip(paths, ARTIFACTS, strict=True):
        verify_artifact(path, artifact)
    partials = sorted(downloads.glob("*.download"), key=lambda path: lexical(path.name))
    if partials:
        raise AdapterError(f"partial source downloads are present: {partials}")
    return paths


def verify_archive_inventory(archive: Path) -> None:
    inventory: list[dict[str, object]] = []
    with tarfile.open(archive, "r:bz2") as bundle:
        members = bundle.getmembers()
        if len(members) != EXPECTED_ARCHIVE_MEMBERS:
            raise AdapterError(
                f"archive member count mismatch: expected {EXPECTED_ARCHIVE_MEMBERS}, found {len(members)}"
            )
        for member in members:
            if member.isdir():
                kind = "dir"
            elif member.isfile() and member.name.endswith(".bz2"):
                kind = "file"
            else:
                raise AdapterError(f"unknown linked-abstract archive member: {member.name}")
            if member.name.startswith("/") or ".." in Path(member.name).parts:
                raise AdapterError(f"unsafe linked-abstract archive member: {member.name}")
            inventory.append({"name": member.name, "size": member.size, "type": kind})
    actual_files = sum(row["type"] == "file" for row in inventory)
    if actual_files != EXPECTED_SHARDS:
        raise AdapterError(
            f"archive shard count mismatch: expected {EXPECTED_SHARDS}, found {actual_files}"
        )
    actual_hash = sha256_bytes(canonical_bytes(inventory))
    if actual_hash != EXPECTED_ARCHIVE_INVENTORY_SHA256:
        raise AdapterError(
            "archive inventory SHA-256 mismatch: "
            f"expected {EXPECTED_ARCHIVE_INVENTORY_SHA256}, found {actual_hash}"
        )


def normalize(value: str) -> str:
    folded = unicodedata.normalize("NFC", value).casefold()
    collapsed = " ".join(folded.split())
    return unicodedata.normalize("NFC", collapsed)


def is_alphanumeric(value: str) -> bool:
    return bool(value) and (value[0].isalpha() or value[0].isnumeric())


def boundary_offsets(value: str) -> tuple[int, ...]:
    offsets = [0]
    for offset in range(1, len(value)):
        if is_alphanumeric(value[offset - 1]) != is_alphanumeric(value[offset]):
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


def parse_source_query(row: Mapping[str, Any], split: str) -> SourceQuery:
    """Parse only the five source fields; judgment keys are never read."""
    required = ("id", "question", "type", "level")
    missing = [field for field in required if field not in row]
    if missing:
        raise AdapterError(f"HotpotQA source query lacks fields: {missing}")
    query = SourceQuery(
        upstream_id=str(row["id"]),
        split=split,
        question_text=str(row["question"]),
        query_type=str(row["type"]),
        level=str(row["level"]),
    )
    if not all((query.upstream_id, query.question_text, query.query_type, query.level)):
        raise AdapterError("HotpotQA source query contains an empty source field")
    return query


def sampling_key(query: SourceQuery) -> tuple[bytes, bytes]:
    preimage = (
        SAMPLE_SALT.encode("utf-8")
        + b"\0"
        + query.split.encode("utf-8")
        + b"\0"
        + query.upstream_id.encode("utf-8")
    )
    return hashlib.sha256(preimage).digest(), lexical(query.upstream_id)


def sample_source_queries(
    rows: Iterable[Mapping[str, Any]], split: str, limit: int
) -> tuple[SourceQuery, ...]:
    parsed = [parse_source_query(row, split) for row in rows]
    identifiers = [query.upstream_id for query in parsed]
    if len(set(identifiers)) != len(identifiers):
        raise AdapterError(f"HotpotQA {split} contains duplicate query IDs")
    if len(parsed) < limit:
        raise AdapterError(
            f"HotpotQA {split} has fewer than the required {limit} source queries"
        )
    return tuple(sorted(parsed, key=sampling_key)[:limit])


def validate_abstract_row(row: Mapping[str, Any], location: str) -> None:
    if set(row) != ABSTRACT_FIELDS:
        raise AdapterError(f"abstract schema mismatch at {location}")
    if not isinstance(row["id"], str) or not row["id"].isdecimal():
        raise AdapterError(f"abstract page ID is not a decimal string at {location}")
    if not isinstance(row["title"], str) or not isinstance(row["url"], str):
        raise AdapterError(f"abstract title/URL type mismatch at {location}")
    arrays = (
        row["text"],
        row["text_with_links"],
        row["charoffset"],
        row["charoffset_with_links"],
    )
    if not all(isinstance(value, list) for value in arrays):
        raise AdapterError(f"abstract array type mismatch at {location}")
    if len(row["text"]) != len(row["text_with_links"]):
        raise AdapterError(f"abstract text/link array mismatch at {location}")
    if not all(isinstance(value, str) for value in row["text"]):
        raise AdapterError(f"abstract text sentence type mismatch at {location}")
    if not all(isinstance(value, str) for value in row["text_with_links"]):
        raise AdapterError(f"abstract linked sentence type mismatch at {location}")


def abstract_shards(abstracts_dir: Path) -> tuple[Path, ...]:
    files = tuple(
        sorted(abstracts_dir.rglob("*.bz2"), key=lambda path: lexical(path.as_posix()))
    )
    if len(files) != EXPECTED_SHARDS:
        raise AdapterError(
            f"extracted abstract shard count mismatch: expected {EXPECTED_SHARDS}, found {len(files)}"
        )
    unexpected = [
        path
        for path in abstracts_dir.rglob("*")
        if path.is_file() and path.suffix != ".bz2"
    ]
    if unexpected:
        raise AdapterError(f"unknown extracted abstract files: {unexpected[:5]}")
    return files


def iter_abstracts(abstracts_dir: Path) -> Iterator[dict[str, Any]]:
    for shard in abstract_shards(abstracts_dir):
        with bz2.open(shard, "rt", encoding="utf-8") as source:
            for line_number, line in enumerate(source, 1):
                try:
                    row = json.loads(line)
                except json.JSONDecodeError as error:
                    raise AdapterError(
                        f"invalid abstract JSON at {shard}:{line_number}"
                    ) from error
                if not isinstance(row, dict):
                    raise AdapterError(f"abstract row is not an object at {shard}:{line_number}")
                validate_abstract_row(row, f"{shard}:{line_number}")
                yield row


def outgoing_aliases(row: Mapping[str, Any]) -> tuple[str, ...]:
    aliases: set[str] = set()
    for sentence in row["text_with_links"]:
        for encoded in HREF_RE.findall(sentence):
            try:
                decoded = html.unescape(urllib.parse.unquote(encoded)).replace("_", " ")
            except UnicodeError:
                continue
            normalized = normalize(decoded.split("#", 1)[0])
            if normalized:
                aliases.add(normalized)
    return tuple(sorted(aliases, key=lexical))


def stable_record_id(source_id: str) -> str:
    if not source_id.isdecimal():
        raise AdapterError(f"Wikipedia page ID is not decimal: {source_id!r}")
    return f"hotpotqa:wiki:{source_id}"


def resolve_seed_candidates(
    source_queries: Sequence[SourceQuery],
    candidates: Mapping[str, Sequence[SeedCandidate]],
) -> tuple[SeedResolution, ...]:
    resolutions: list[SeedResolution] = []
    for query in sorted(source_queries, key=lambda item: lexical(item.upstream_id)):
        rows = list(candidates.get(query.upstream_id, ()))
        if not rows:
            resolutions.append(
                SeedResolution(query.upstream_id, "no_match", (), None, None)
            )
            continue
        longest = max(len(row.normalized_title) for row in rows)
        by_record = {
            row.record_id: row
            for row in rows
            if len(row.normalized_title) == longest
        }
        candidate_ids = tuple(sorted(by_record, key=lexical))
        if len(candidate_ids) != 1:
            resolutions.append(
                SeedResolution(
                    query.upstream_id, "ambiguous", candidate_ids, None, None
                )
            )
            continue
        selected = by_record[candidate_ids[0]]
        resolutions.append(
            SeedResolution(
                query.upstream_id,
                "resolved",
                candidate_ids,
                selected.record_id,
                selected.title,
            )
        )
    return tuple(resolutions)


def _default_expected() -> dict[str, int | str]:
    return {
        "source_records": EXPECTED_ABSTRACT_ROWS,
        "source_unique_titles": EXPECTED_ABSTRACT_UNIQUE_TITLES,
        "source_conflicting_titles": EXPECTED_ABSTRACT_CONFLICTS,
        "records": EXPECTED_RECORDS,
        "edges": EXPECTED_EDGES,
        "missing": EXPECTED_MISSING_ALIASES,
        "selected_conflicts": EXPECTED_SELECTED_CONFLICTS,
        "preimage_sha256": EXPECTED_CORPUS_PREIMAGE_SHA256,
    }


def freeze_source_corpus(
    source_queries: Sequence[SourceQuery],
    row_factory: Callable[[], Iterable[Mapping[str, Any]]],
    *,
    expected: Mapping[str, int | str] | None = None,
) -> FrozenCorpus:
    """Build the label-blind corpus from source queries and abstract fields only."""
    query_substrings: dict[str, list[str]] = defaultdict(list)
    for query in source_queries:
        for alias in alias_substrings(query.question_text):
            query_substrings[alias].append(query.upstream_id)
    for query_ids in query_substrings.values():
        query_ids.sort(key=lexical)

    candidates: dict[str, list[SeedCandidate]] = defaultdict(list)
    source_ids: set[str] = set()
    title_digests: dict[str, bytes] = {}
    source_conflicts: set[str] = set()
    records_seen = 0
    for offset, row in enumerate(row_factory(), 1):
        validate_abstract_row(row, f"abstract stream row {offset}")
        records_seen += 1
        source_id = row["id"]
        if source_id in source_ids:
            raise AdapterError(f"abstract source ID is duplicated: {source_id}")
        source_ids.add(source_id)
        title = row["title"]
        normalized_title = normalize(title)
        text_digest = hashlib.sha256("".join(row["text"]).encode("utf-8")).digest()
        previous = title_digests.setdefault(normalized_title, text_digest)
        if previous != text_digest:
            source_conflicts.add(normalized_title)
        matching_queries = query_substrings.get(normalized_title)
        if not matching_queries:
            continue
        candidate = SeedCandidate(
            record_id=stable_record_id(source_id),
            source_id=source_id,
            title=title,
            normalized_title=normalized_title,
            outgoing_aliases=outgoing_aliases(row)[:NEIGHBOR_LIMIT],
        )
        for upstream_id in matching_queries:
            candidates[upstream_id].append(candidate)

    resolutions = resolve_seed_candidates(source_queries, candidates)
    candidates_by_record = {
        candidate.record_id: candidate
        for query_candidates in candidates.values()
        for candidate in query_candidates
    }
    requested_aliases: set[str] = set()
    for resolution in resolutions:
        if resolution.selected_record_id is None:
            continue
        selected = candidates_by_record[resolution.selected_record_id]
        requested_aliases.add(selected.normalized_title)
        requested_aliases.update(selected.outgoing_aliases)
    if len(requested_aliases) > (TRAIN_LIMIT + DEV_LIMIT) * (NEIGHBOR_LIMIT + 1):
        raise AdapterError("label-blind corpus request exceeds the frozen 48,000 bound")

    selected_rows: dict[str, Mapping[str, Any]] = {}
    selected_conflicts: set[str] = set()
    for offset, row in enumerate(row_factory(), 1):
        validate_abstract_row(row, f"abstract stream row {offset}")
        normalized_title = normalize(row["title"])
        if normalized_title not in requested_aliases:
            continue
        previous = selected_rows.get(normalized_title)
        if previous is None:
            selected_rows[normalized_title] = row
            continue
        if "".join(previous["text"]) != "".join(row["text"]):
            selected_conflicts.add(normalized_title)
        if int(row["id"]) < int(previous["id"]):
            selected_rows[normalized_title] = row

    missing = tuple(sorted(requested_aliases - selected_rows.keys(), key=lexical))
    alias_to_record = {
        alias: stable_record_id(row["id"]) for alias, row in selected_rows.items()
    }
    records: list[CorpusRecord] = []
    for alias, row in selected_rows.items():
        outgoing_ids = {
            alias_to_record[target]
            for target in outgoing_aliases(row)
            if target in alias_to_record and target != alias
        }
        records.append(
            CorpusRecord(
                record_id=alias_to_record[alias],
                source_id=row["id"],
                title=row["title"],
                text="".join(row["text"]),
                outgoing_record_ids=tuple(sorted(outgoing_ids, key=lexical)),
            )
        )
    records.sort(key=lambda record: lexical(record.record_id))
    selected_conflicts_tuple = tuple(sorted(selected_conflicts, key=lexical))
    preimage = {
        "conflict_policy": "lowest numeric Wikipedia page ID",
        "neighbor_limit": NEIGHBOR_LIMIT,
        "records": [asdict(record) for record in records],
        "records_seen": records_seen,
        "sample_salt": SAMPLE_SALT,
        "selected_conflicting_titles": list(selected_conflicts_tuple),
        "selected_missing_titles": list(missing),
        "source_conflicting_titles": len(source_conflicts),
    }
    frozen = FrozenCorpus(
        records=tuple(records),
        resolutions=resolutions,
        preimage_sha256=sha256_bytes(canonical_bytes(preimage)),
        source_conflicting_titles=len(source_conflicts),
        source_records=records_seen,
        source_unique_titles=len(title_digests),
        selected_conflicting_titles=selected_conflicts_tuple,
        selected_missing_titles=missing,
    )
    if expected is not None:
        validate_frozen_corpus(frozen, expected)
    return frozen


def validate_frozen_corpus(
    corpus: FrozenCorpus, expected: Mapping[str, int | str] | None = None
) -> None:
    locked = _default_expected() if expected is None else expected
    actual: dict[str, int | str] = {
        "source_records": corpus.source_records,
        "source_unique_titles": corpus.source_unique_titles,
        "source_conflicting_titles": corpus.source_conflicting_titles,
        "records": len(corpus.records),
        "edges": sum(len(record.outgoing_record_ids) for record in corpus.records),
        "missing": len(corpus.selected_missing_titles),
        "selected_conflicts": len(corpus.selected_conflicting_titles),
        "preimage_sha256": corpus.preimage_sha256,
    }
    if actual != dict(locked):
        raise AdapterError(f"frozen corpus count/hash mismatch: expected {dict(locked)}, found {actual}")
    record_ids = [record.record_id for record in corpus.records]
    if record_ids != sorted(record_ids, key=lexical) or len(set(record_ids)) != len(record_ids):
        raise AdapterError("frozen corpus record identities are not unique lexical order")
    for record in corpus.records:
        if record.outgoing_record_ids != tuple(sorted(set(record.outgoing_record_ids), key=lexical)):
            raise AdapterError(f"record {record.record_id} outgoing links are not deduplicated lexical order")
        if record.record_id in record.outgoing_record_ids:
            raise AdapterError(f"record {record.record_id} contains a self link")


def read_question_rows(paths: Sequence[Path]) -> list[dict[str, Any]]:
    try:
        import pyarrow as pa
        import pyarrow.parquet as parquet
    except ImportError as error:
        raise AdapterError("HotpotQA build requires pinned pyarrow==25.0.0") from error
    if pa.__version__ != "25.0.0":
        raise AdapterError(f"pyarrow version mismatch: expected 25.0.0, found {pa.__version__}")
    rows: list[dict[str, Any]] = []
    for path in paths:
        parquet_file = parquet.ParquetFile(path)
        names = set(parquet_file.schema_arrow.names)
        if names != QUESTION_FIELDS:
            raise AdapterError(f"question shard schema mismatch for {path}: {sorted(names)}")
        for batch in parquet_file.iter_batches(batch_size=512):
            rows.extend(batch.to_pylist())
    return rows


def build_real_source_corpus(
    cache_dir: Path, abstracts_dir: Path
) -> tuple[FrozenCorpus, tuple[SourceQuery, ...], list[dict[str, Any]]]:
    source_paths = verify_source_artifacts(cache_dir)
    verify_archive_inventory(source_paths[0])
    train_rows = read_question_rows(source_paths[1:3])
    dev_rows = read_question_rows(source_paths[3:4])
    if len(train_rows) != 90_447 or len(dev_rows) != 7_405:
        raise AdapterError(
            f"question row counts mismatch: train={len(train_rows)}, dev={len(dev_rows)}"
        )
    all_ids = [str(row["id"]) for row in train_rows + dev_rows]
    if len(set(all_ids)) != len(all_ids):
        raise AdapterError("question IDs are duplicated across frozen shards")
    train_sources = sample_source_queries(train_rows, TRAIN_SPLIT, TRAIN_LIMIT)
    dev_sources = sample_source_queries(dev_rows, DEV_SPLIT, DEV_LIMIT)
    source_queries = train_sources + dev_sources
    corpus = freeze_source_corpus(
        source_queries,
        lambda: iter_abstracts(abstracts_dir),
        expected=_default_expected(),
    )
    return corpus, source_queries, train_rows + dev_rows


def population_hash(query_ids: Iterable[str]) -> str:
    return sha256_bytes(
        b"".join(f"{query_id}\n".encode("utf-8") for query_id in sorted(query_ids, key=lexical))
    )


def parse_judgment(row: Mapping[str, Any]) -> Judgment:
    """Parse gold fields only after a ``FrozenCorpus`` has been returned."""
    if set(row) != QUESTION_FIELDS:
        raise AdapterError("HotpotQA judgment row has an unknown or missing field")
    supporting = row["supporting_facts"]
    context = row["context"]
    if not isinstance(supporting, Mapping) or set(supporting) != {"title", "sent_id"}:
        raise AdapterError("HotpotQA supporting_facts schema mismatch")
    if not isinstance(context, Mapping) or set(context) != {"title", "sentences"}:
        raise AdapterError("HotpotQA context schema mismatch")
    titles = supporting["title"]
    sentence_ids = supporting["sent_id"]
    context_titles = context["title"]
    context_sentences = context["sentences"]
    if not all(isinstance(value, list) for value in (titles, sentence_ids, context_titles, context_sentences)):
        raise AdapterError("HotpotQA judgment arrays have the wrong type")
    if len(titles) != len(sentence_ids) or len(context_titles) != len(context_sentences):
        raise AdapterError("HotpotQA judgment parallel arrays have different lengths")
    context_by_title: dict[str, list[str]] = {}
    for title, sentences in zip(context_titles, context_sentences, strict=True):
        if not isinstance(title, str) or not isinstance(sentences, list) or not all(
            isinstance(sentence, str) for sentence in sentences
        ):
            raise AdapterError("HotpotQA context title/sentence type mismatch")
        normalized = normalize(title)
        if normalized in context_by_title:
            raise AdapterError("HotpotQA context repeats a normalized title")
        context_by_title[normalized] = sentences
    facts: list[tuple[str, int]] = []
    for title, sentence_id in zip(titles, sentence_ids, strict=True):
        if not isinstance(title, str) or not isinstance(sentence_id, int) or isinstance(sentence_id, bool):
            raise AdapterError("HotpotQA supporting fact has the wrong type")
        sentences = context_by_title.get(normalize(title))
        if sentences is None or sentence_id < 0 or sentence_id >= len(sentences):
            raise AdapterError(
                f"HotpotQA supporting fact is absent from context: {title!r}/{sentence_id}"
            )
        facts.append((title, sentence_id))
    if len(set(facts)) != len(facts):
        raise AdapterError("HotpotQA judgment repeats a supporting sentence")
    upstream_id = row["id"]
    if not isinstance(upstream_id, str) or not upstream_id:
        raise AdapterError("HotpotQA judgment ID is invalid")
    answer = row["answer"]
    if not isinstance(answer, str):
        raise AdapterError("HotpotQA answer is not a string")
    return Judgment(upstream_id, answer, tuple(facts))


def canonical_record(record: CorpusRecord) -> dict[str, Any]:
    return {
        "chunks": [
            {
                "chunk_key": "abstract",
                "metadata": {},
                "text": f"{record.title}\n\n{record.text}",
            }
        ],
        "content": record.text,
        "fields": {
            "outgoing_record_ids": {
                "type": "list",
                "value": [
                    {"type": "string", "value": record_id}
                    for record_id in record.outgoing_record_ids
                ],
            },
            "title": {"type": "string", "value": record.title},
            "upstream_page_id": {"type": "string", "value": record.source_id},
        },
        "metadata": {},
        "record_id": record.record_id,
        "record_type": "WikipediaArticle",
    }


def graph_schema() -> dict[str, Any]:
    return {
        "chunk_nodes": {
            "inverse_relationship": "PartOf",
            "node_type": "Chunk",
            "owns_relationship": "ContainsChunk",
        },
        "record_nodes": [
            {
                "node_type": "Article",
                "queryable_fields": [["title"]],
                "record_type": "WikipediaArticle",
            }
        ],
        "relationships": [
            {
                "allow_self_edge": False,
                "cardinality": "many",
                "duplicate_references": "deduplicate",
                "inverse_relationship": None,
                "missing_target": "omit_edge",
                "relationship_type": "LinksTo",
                "source_field": ["outgoing_record_ids"],
                "source_node_type": "Article",
                "target_node_type": "Article",
            }
        ],
        "version": 1,
    }


def node_seed(record_id: str) -> dict[str, Any]:
    return {
        "kind": "node_ids",
        "nodes": [
            {
                "node_type": "Article",
                "source": {"kind": "record", "record_id": record_id},
            }
        ],
    }


def seed_aliases(corpus: FrozenCorpus) -> list[dict[str, Any]]:
    rows = [
        {
            "alias": record.title,
            "normalized_alias": normalize(record.title),
            "seed": node_seed(record.record_id),
            "source": {"field": ["title"], "record_id": record.record_id},
        }
        for record in corpus.records
    ]
    rows.sort(
        key=lambda row: (
            lexical(row["normalized_alias"]),
            canonical_bytes(row["seed"]),
            lexical(row["source"]["record_id"]),
            canonical_bytes(row["source"]["field"]),
            lexical(row["alias"]),
        )
    )
    if len({row["normalized_alias"] for row in rows}) != len(rows):
        raise AdapterError("retained title alias table is not collision-free")
    return rows


def build_split_artifacts(
    corpus: FrozenCorpus,
    sampled_sources: Sequence[SourceQuery],
    sampled_rows_by_id: Mapping[str, Mapping[str, Any]],
    output_split: str,
) -> SplitArtifacts:
    if output_split not in {"development", "test"}:
        raise AdapterError(f"unknown output split: {output_split}")
    upstream_split = TRAIN_SPLIT if output_split == "development" else DEV_SPLIT
    sources = [source for source in sampled_sources if source.split == upstream_split]
    source_by_id = {source.upstream_id: source for source in sources}
    if len(source_by_id) != len(sources):
        raise AdapterError(f"duplicate sampled source IDs in {output_split}")
    available = corpus.normalized_title_to_record_id
    resolution_by_id = {row.upstream_id: row for row in corpus.resolutions}
    queries: list[dict[str, Any]] = []
    qrels: list[tuple[str, str, int]] = []
    evidence: list[dict[str, Any]] = []
    exclusions: list[dict[str, Any]] = []
    derived_success: list[str] = []
    for upstream_id in sorted(source_by_id, key=lexical):
        source = source_by_id[upstream_id]
        raw = sampled_rows_by_id.get(upstream_id)
        if raw is None:
            raise AdapterError(f"missing sampled judgment row: {upstream_id}")
        judgment = parse_judgment(raw)
        normalized_titles = sorted(
            {normalize(title) for title in judgment.supporting_titles}, key=lexical
        )
        if not normalized_titles:
            exclusions.append(
                exclusion_row(
                    upstream_id,
                    "global",
                    "missing_complete_evidence",
                    "upstream query has no supporting facts",
                )
            )
            continue
        missing_titles = [title for title in normalized_titles if title not in available]
        if missing_titles:
            exclusions.append(
                exclusion_row(
                    upstream_id,
                    "global",
                    "not_in_frozen_corpus",
                    "complete supporting-document set is not in the label-blind frozen corpus",
                )
            )
            continue
        evidence_ids = sorted({available[title] for title in normalized_titles}, key=lexical)
        if len(evidence_ids) != 2:
            raise AdapterError(
                f"eligible HotpotQA query {upstream_id} expected two supporting documents, found {len(evidence_ids)}"
            )
        queries.append(
            {
                "category": f"{source.query_type}:{source.level}",
                "derived_seed_policy_id": DERIVED_POLICY_ID,
                "explicit_seed": None,
                "metadata_filter": None,
                "query_id": upstream_id,
                "split": output_split,
                "tasks": ["evidence", "retrieval"],
                "text": source.question_text,
                "traversal": TRAVERSAL,
            }
        )
        qrels.extend((upstream_id, record_id, 1) for record_id in evidence_ids)
        evidence.append({"evidence_sets": [evidence_ids], "query_id": upstream_id})
        resolution = resolution_by_id[upstream_id]
        if resolution.status == "resolved":
            if resolution.selected_record_id not in {record.record_id for record in corpus.records}:
                raise AdapterError(f"resolved seed is absent from frozen corpus: {upstream_id}")
            derived_success.append(upstream_id)
        elif resolution.status in {"ambiguous", "no_match"}:
            reason = f"derived_seed_{resolution.status}"
            exclusions.append(
                exclusion_row(
                    upstream_id,
                    DERIVED_POLICY_ID,
                    reason,
                    "frozen exact-title resolver did not produce one source-only seed",
                )
            )
        else:
            raise AdapterError(f"unknown seed resolution status: {resolution.status}")
    queries.sort(key=lambda row: lexical(row["query_id"]))
    qrels.sort(key=lambda row: (lexical(row[0]), lexical(row[1])))
    evidence.sort(key=lambda row: lexical(row["query_id"]))
    exclusions.sort(
        key=lambda row: tuple(
            lexical(str(row[field]))
            for field in ("query_id", "lane", "phase", "reason", "source")
        )
    )
    population = population_hash(row["query_id"] for row in queries)
    derived_population = population_hash(derived_success)
    expected = EXPECTED_POPULATIONS[output_split]
    global_count = sum(row["lane"] == "global" for row in exclusions)
    derived_count = len(exclusions) - global_count
    actual = {
        "eligible": len(queries),
        "global_exclusions": global_count,
        "population_sha256": population,
        "derived_eligible": len(derived_success),
        "derived_exclusions": derived_count,
        "derived_population_sha256": derived_population,
    }
    if actual != expected:
        raise AdapterError(f"{output_split} population mismatch: expected {expected}, found {actual}")
    if len(qrels) != 2 * len(queries) or len(evidence) != len(queries):
        raise AdapterError(f"{output_split} qrel/evidence completeness mismatch")
    return SplitArtifacts(
        output_split,
        tuple(queries),
        tuple(qrels),
        tuple(evidence),
        tuple(exclusions),
        population,
        derived_population,
    )


def exclusion_row(
    query_id: str, lane: str, reason: str, details: str
) -> dict[str, str]:
    return {
        "details": details,
        "lane": lane,
        "phase": "pre_freeze",
        "query_id": query_id,
        "reason": reason,
        "source": "adapter",
    }


def jsonl_bytes(rows: Iterable[object]) -> bytes:
    return b"".join(canonical_bytes(row, final_lf=True) for row in rows)


def qrels_bytes(rows: Iterable[tuple[str, str, int]]) -> bytes:
    return b"".join(
        f"{query_id} 0 {record_id} {grade}\n".encode("utf-8")
        for query_id, record_id, grade in rows
    )


def canonical_collection_core_files(
    corpus: FrozenCorpus, split: SplitArtifacts
) -> dict[str, bytes]:
    files = {
        "evidence-judgments.jsonl": jsonl_bytes(split.evidence),
        "exclusions.jsonl": jsonl_bytes(split.exclusions),
        "expected-paths.jsonl": b"",
        "graph-schema.json": canonical_bytes(graph_schema(), final_lf=True),
        "qrels.tsv": qrels_bytes(split.qrels),
        "queries.jsonl": jsonl_bytes(split.queries),
        "records.jsonl": jsonl_bytes(canonical_record(record) for record in corpus.records),
    }
    if files["expected-paths.jsonl"] != b"":
        raise AdapterError("expected-paths.jsonl must be exactly zero bytes")
    return files


def build_real_collection_inputs(
    cache_dir: Path, abstracts_dir: Path
) -> tuple[FrozenCorpus, tuple[SourceQuery, ...], dict[str, Mapping[str, Any]], dict[str, SplitArtifacts]]:
    corpus, sources, rows = build_real_source_corpus(cache_dir, abstracts_dir)
    row_by_id = {str(row["id"]): row for row in rows}
    splits = {
        name: build_split_artifacts(corpus, sources, row_by_id, name)
        for name in ("development", "test")
    }
    resolution_counts = Counter(row.status for row in corpus.resolutions)
    if dict(sorted(resolution_counts.items())) != EXPECTED_RESOLUTIONS:
        raise AdapterError(
            f"seed resolution mismatch: expected {EXPECTED_RESOLUTIONS}, found {dict(resolution_counts)}"
        )
    development_files = canonical_collection_core_files(corpus, splits["development"])
    test_files = canonical_collection_core_files(corpus, splits["test"])
    for shared in ("records.jsonl", "graph-schema.json"):
        if development_files[shared] != test_files[shared]:
            raise AdapterError(f"shared corpus file differs across splits: {shared}")
    return corpus, sources, row_by_id, splits


COLLECTION_PATHS = {
    "chunking_manifest": "manifests/chunking.json",
    "corpus_embeddings_f32": "corpus-embeddings.f32.jsonl",
    "embedding_manifest": "manifests/embedding.json",
    "evidence_judgments": "evidence-judgments.jsonl",
    "exclusions": "exclusions.jsonl",
    "expected_paths": "expected-paths.jsonl",
    "graph_construction_manifest": "manifests/graph-construction.json",
    "graph_schema": "graph-schema.json",
    "preprocessing_manifest": "manifests/preprocessing.json",
    "qrels": "qrels.tsv",
    "queries": "queries.jsonl",
    "query_embeddings_f32": "query-embeddings.f32.jsonl",
    "records": "records.jsonl",
    "seed_policy_manifest": "manifests/seed-policy.json",
    "split_manifest": "manifests/split.json",
}
QUANTIZATION_POLICY = {
    "arithmetic": "ieee754_f32_each_operation",
    "clamp_max": 127,
    "clamp_min": -128,
    "dot_accumulator": "signed_i32_exact",
    "encoding_expression": "value_times_reciprocal_scale",
    "kind": "symmetric_per_vector_i8",
    "rounding": "half_away_from_zero",
    "scale_divisor": 127,
    "score_expression": "f32_i32_dot_times_query_scale_times_chunk_scale",
    "zero_vector_scale": 0,
}


def manifest_input(source_id: str, digest: str) -> dict[str, str]:
    return {"sha256": digest, "source_id": source_id}


def manifest_output(path: str, files: Mapping[str, bytes]) -> dict[str, str]:
    return {"path": path, "sha256": sha256_bytes(files[path])}


def upstream_inputs(
    inventory: Mapping[str, str], prefixes: Sequence[str]
) -> list[dict[str, str]]:
    rows = [
        manifest_input(source_id, digest)
        for source_id, digest in inventory.items()
        if any(source_id.startswith(prefix) for prefix in prefixes)
    ]
    return sorted(rows, key=lambda row: lexical(row["source_id"]))


def collection_inputs(
    files: Mapping[str, bytes], paths: Sequence[str]
) -> list[dict[str, str]]:
    return [
        manifest_input(f"collection/{path}", sha256_bytes(files[path]))
        for path in sorted(paths, key=lexical)
    ]


def exclusion_counts(split: SplitArtifacts) -> list[dict[str, Any]]:
    by_reason = Counter((row["lane"], row["reason"]) for row in split.exclusions)
    global_before = EXPECTED_POPULATIONS[split.split]["eligible"] + EXPECTED_POPULATIONS[split.split]["global_exclusions"]
    rows: list[dict[str, Any]] = []
    for reason in (
        "duplicate_identity",
        "filter_label_conflict",
        "invalid_upstream_record",
        "missing_complete_evidence",
        "no_relevant_documents",
        "not_in_frozen_corpus",
    ):
        excluded = by_reason[("global", reason)]
        rows.append(
            {
                "after": global_before - excluded,
                "before": global_before,
                "excluded": excluded,
                "lane": "global",
                "reason": reason,
            }
        )
        global_before -= excluded
    derived_before = len(split.queries)
    for reason in ("derived_seed_ambiguous", "derived_seed_no_match"):
        excluded = by_reason[(DERIVED_POLICY_ID, reason)]
        rows.append(
            {
                "after": derived_before - excluded,
                "before": derived_before,
                "excluded": excluded,
                "lane": DERIVED_POLICY_ID,
                "reason": reason,
            }
        )
        derived_before -= excluded
    rows.sort(key=lambda row: (lexical(row["lane"]), lexical(row["reason"])))
    return rows


def source_inventory_preimage(inventory: Mapping[str, str]) -> list[dict[str, str]]:
    return [
        manifest_input(source_id, digest)
        for source_id, digest in sorted(inventory.items(), key=lambda row: lexical(row[0]))
    ]


def build_transformation_manifests(
    files: dict[str, bytes],
    corpus: FrozenCorpus,
    split: SplitArtifacts,
    inventory: Mapping[str, str],
    embedding_parameters: Mapping[str, Any],
    unicode_tables_sha256: str,
) -> dict[str, dict[str, Any]]:
    tool = {"name": "build_hotpotqa_graph_collection.py", "version": "1.0.0"}

    def outputs(*paths: str) -> list[dict[str, str]]:
        return [
            manifest_output(path, files) for path in sorted(paths, key=lexical)
        ]

    inventory_sha = sha256_bytes(canonical_bytes(source_inventory_preimage(inventory)))
    exclusions = exclusion_counts(split)
    development_hash = EXPECTED_POPULATIONS["development"]["population_sha256"]
    test_hash = EXPECTED_POPULATIONS["test"]["population_sha256"]
    lock_preimage = {
        "collection_rule": "frozen source-only salted sample, exact-title seed plus first 15 lexical outgoing aliases, then complete-evidence eligibility",
        "development_population_sha256": development_hash,
        "exclusion_counts": exclusions,
        "release_id": "hotpotqa-distractor-v1.1-v1-linked-abstracts-2019-01-14",
        "source_inventory_sha256": inventory_sha,
        "split_id": split.split,
        "test_population_sha256": test_hash,
    }
    aliases = seed_aliases(corpus)
    failed_ids = [
        row["query_id"]
        for row in split.exclusions
        if row["lane"] == DERIVED_POLICY_ID
    ]
    manifests = {
        "manifests/preprocessing.json": {
            "inputs": upstream_inputs(inventory, ["upstream/corpus/"]),
            "outputs": [],
            "parameters": {
                "field_selection": [["id"], ["text"], ["text_with_links"], ["title"]],
                "source_record_id_path": ["id"],
                "source_record_type_path": None,
                "source_to_record_mapping": "one retained Wikipedia abstract row to one WikipediaArticle record",
                "text_join_separator": "",
                "title_path": ["title"],
                "unicode_handling": "preserve source strings; normalize only aliases with frozen seed policy",
                "whitespace_rules": "preserve source title/text bytes without trimming",
            },
            "policy_id": "hotpotqa-abstract-preprocessing-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
        "manifests/chunking.json": {
            "inputs": upstream_inputs(inventory, ["upstream/corpus/"]),
            "outputs": outputs("records.jsonl"),
            "parameters": {
                "boundary_policy": "one complete upstream abstract per chunk",
                "chunker_name": "hotpotqa-abstract-v1",
                "chunker_version": "1",
                "maximum_size": 1_000_000_000,
                "overlap": 0,
                "source_offset_policy": "whole concatenated upstream abstract",
                "stable_key_derivation": "constant abstract within stable Wikipedia page record",
                "units": "unicode scalar values",
            },
            "policy_id": "hotpotqa-abstract-chunking-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
        "manifests/graph-construction.json": {
            "inputs": collection_inputs(files, ["records.jsonl"]),
            "outputs": outputs("graph-schema.json"),
            "parameters": {
                "duplicate_references": "deduplicate normalized hyperlink targets before lexical record-ID order",
                "inverse_edges": False,
                "judgment_inputs_sha256": None,
                "missing_target": "omit missing requested hyperlink targets",
                "node_derivation": "WikipediaArticle records map to Article nodes; canonical chunks map to Chunk nodes",
                "relationship_derivation": "directed LinksTo from source-only text_with_links; ContainsChunk/PartOf from canonical ownership",
                "schema_sha256": sha256_bytes(files["graph-schema.json"]),
                "self_edges": False,
                "source_fields": [["id"], ["text"], ["text_with_links"], ["title"]],
            },
            "policy_id": "hotpotqa-linked-abstract-graph-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
        "manifests/split.json": {
            "inputs": collection_inputs(files, ["graph-schema.json", "records.jsonl"])
            + upstream_inputs(
                inventory,
                [
                    "upstream/judgment/",
                    "upstream/license/",
                    "upstream/query/",
                    "upstream/scenario/",
                ],
            ),
            "outputs": outputs(
                "evidence-judgments.jsonl",
                "exclusions.jsonl",
                "expected-paths.jsonl",
                "qrels.tsv",
                "queries.jsonl",
            ),
            "parameters": {
                "archive_sha256": ARTIFACTS[0].sha256,
                "archive_url": ARTIFACTS[0].url,
                "collection_rule": lock_preimage["collection_rule"],
                "development_population_sha256": development_hash,
                "exclusion_counts": exclusions,
                "license_id": "CC-BY-SA-4.0",
                "license_notice_source_id": "upstream/license/hotpotqa-attribution-v1",
                "release_id": lock_preimage["release_id"],
                "source_inventory_sha256": inventory_sha,
                "split_id": split.split,
                "test_lock_sha256": sha256_bytes(canonical_bytes(lock_preimage)),
                "test_population_sha256": test_hash,
            },
            "policy_id": "hotpotqa-frozen-split-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
        "manifests/seed-policy.json": {
            "inputs": collection_inputs(
                files,
                ["exclusions.jsonl", "graph-schema.json", "queries.jsonl", "records.jsonl"],
            )
            + upstream_inputs(inventory, ["upstream/scenario/"]),
            "outputs": [],
            "parameters": {
                "derived_policies": [
                    {
                        "alias_table_sha256": sha256_bytes(canonical_bytes(aliases)),
                        "aliases": aliases,
                        "declared_population_sha256": split.population_sha256,
                        "failure_population_sha256": population_hash(failed_ids),
                        "policy_id": DERIVED_POLICY_ID,
                        "policy_version": "1",
                        "source_fields": [["title"]],
                        "successful_population_sha256": split.derived_population_sha256,
                    }
                ],
                "explicit_policy": {
                    "policy_id": "explicit",
                    "policy_version": "1",
                    "provenance": [],
                },
                "normalization": {
                    "case_folding": "unicode_default_full_case_folding",
                    "normalization_form": "NFC",
                    "normalization_version": "unicode-15.1-nfc-full-fold-whitespace-v1",
                    "punctuation": "preserve",
                    "unicode_tables_sha256": unicode_tables_sha256,
                    "unicode_version": "15.1",
                    "whitespace": "unicode_white_space_to_ascii_collapse_trim",
                },
            },
            "policy_id": "hotpotqa-seed-policy-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
        "manifests/embedding.json": {
            "inputs": collection_inputs(files, ["queries.jsonl", "records.jsonl"])
            + upstream_inputs(inventory, ["upstream/model/", "upstream/tokenizer/"]),
            "outputs": outputs(
                "corpus-embeddings.f32.jsonl", "query-embeddings.f32.jsonl"
            ),
            "parameters": dict(embedding_parameters),
            "policy_id": "hotpotqa-minilm-embedding-v1",
            "policy_version": "1",
            "schema_version": 1,
            "tool": tool,
        },
    }
    for path, value in manifests.items():
        files[path] = canonical_bytes(value, final_lf=True)
    return manifests


def collection_header(
    files: Mapping[str, bytes], split: SplitArtifacts, corpus_count: int
) -> dict[str, Any]:
    missing = set(COLLECTION_PATHS.values()) - files.keys()
    if missing:
        raise AdapterError(f"collection header cannot close missing files: {sorted(missing)}")
    return {
        "collection_id": f"{COLLECTION_BASE_ID}-{split.split}",
        "collection_version": "1",
        "corpus_id": CORPUS_ID,
        "counts": {
            "chunks": corpus_count,
            "evidence_rows": len(split.evidence),
            "exclusion_rows": len(split.exclusions),
            "expected_path_rows": 0,
            "qrel_rows": len(split.qrels),
            "queries": len(split.queries),
            "records": corpus_count,
        },
        "evaluation_depth": 100,
        "files": [
            {"bytes": len(files[path]), "path": path, "sha256": sha256_bytes(files[path])}
            for path in sorted(COLLECTION_PATHS.values(), key=lexical)
        ],
        "paths": COLLECTION_PATHS,
        "relevance_threshold": 1,
        "schema_version": 3,
        "split": split.split,
        "top_k": 10,
    }


def assemble_collection_files(
    corpus: FrozenCorpus,
    split: SplitArtifacts,
    corpus_embeddings: bytes,
    query_embeddings: bytes,
    inventory: Mapping[str, str],
    embedding_parameters: Mapping[str, Any],
    unicode_tables_sha256: str,
) -> dict[str, bytes]:
    files = canonical_collection_core_files(corpus, split)
    files["corpus-embeddings.f32.jsonl"] = corpus_embeddings
    files["query-embeddings.f32.jsonl"] = query_embeddings
    build_transformation_manifests(
        files,
        corpus,
        split,
        inventory,
        embedding_parameters,
        unicode_tables_sha256,
    )
    files["collection.json"] = canonical_bytes(
        collection_header(files, split, len(corpus.records)), final_lf=True
    )
    expected = {"collection.json", *COLLECTION_PATHS.values()}
    if set(files) != expected:
        raise AdapterError("assembled collection has a missing or unexpected file")
    return files


def write_collection_files(root: Path, files: Mapping[str, bytes]) -> None:
    expected = {"collection.json", *COLLECTION_PATHS.values()}
    if set(files) != expected:
        raise AdapterError(
            f"collection file inventory mismatch: expected {sorted(expected)}, found {sorted(files)}"
        )
    root.mkdir(parents=True, exist_ok=False)
    (root / "manifests").mkdir()
    for relative, data in files.items():
        path = root / relative
        path.write_bytes(data)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache-dir", type=Path, required=True)
    parser.add_argument("--abstracts-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeat-and-compare", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    # Later Phase 2b commits complete canonical emission, embedding, validation,
    # and atomic publication.  Keeping this call live makes Task 1 independently
    # executable and validates the complete real source corpus.
    corpus, source_queries, _, splits = build_real_collection_inputs(
        args.cache_dir, args.abstracts_dir
    )
    print(
        json.dumps(
            {
                "corpus_preimage_sha256": corpus.preimage_sha256,
                "edges": sum(len(record.outgoing_record_ids) for record in corpus.records),
                "development_queries": len(splits["development"].queries),
                "records": len(corpus.records),
                "sampled_queries": len(source_queries),
                "status": "source_corpus_valid",
                "test_queries": len(splits["test"].queries),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except AdapterError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
