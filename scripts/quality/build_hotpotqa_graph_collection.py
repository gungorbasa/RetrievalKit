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
from collections import defaultdict
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
    corpus, source_queries, _ = build_real_source_corpus(args.cache_dir, args.abstracts_dir)
    print(
        json.dumps(
            {
                "corpus_preimage_sha256": corpus.preimage_sha256,
                "edges": sum(len(record.outgoing_record_ids) for record in corpus.records),
                "records": len(corpus.records),
                "sampled_queries": len(source_queries),
                "status": "source_corpus_valid",
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
