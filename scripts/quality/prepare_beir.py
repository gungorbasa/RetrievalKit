#!/usr/bin/env python3
"""Prepare pinned BEIR collections for VectorKit's Rust quality runner."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import time
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
BEIR_BASE = "https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets"


@dataclass(frozen=True)
class Split:
    query_count: int
    qrels_count: int


@dataclass(frozen=True)
class Dataset:
    name: str
    checksum: str
    corpus_count: int
    splits: dict[str, Split]

    @property
    def url(self) -> str:
        return f"{BEIR_BASE}/{self.name}.zip"


DATASETS = {
    "scifact": Dataset(
        name="scifact",
        checksum="5f7d1de60b170fc8027bb7898e2efca1",
        corpus_count=5183,
        splits={
            "train": Split(query_count=809, qrels_count=919),
            "test": Split(query_count=300, qrels_count=339),
        },
    ),
    "nfcorpus": Dataset(
        name="nfcorpus",
        checksum="a89dba18a62ef92f7d323ec890a0d38d",
        corpus_count=3633,
        splits={
            "train": Split(query_count=2590, qrels_count=110575),
            "dev": Split(query_count=324, qrels_count=11385),
            "test": Split(query_count=323, qrels_count=12334),
        },
    ),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", choices=sorted(DATASETS), required=True)
    parser.add_argument("--split", choices=("train", "dev", "test"), default="test")
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=ROOT / "target/benchmarks/beir",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Defaults to <cache-dir>/<dataset>/vectorkit.",
    )
    parser.add_argument(
        "--model-dir",
        type=Path,
        default=ROOT / "target/embedding-models/all-MiniLM-L6-v2",
    )
    parser.add_argument("--evaluation-depth", type=int, default=1000)
    parser.add_argument("--download-only", action="store_true")
    parser.add_argument("--force-download", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.evaluation_depth < 10:
        raise SystemExit("--evaluation-depth must be at least 10")
    dataset = DATASETS[args.dataset]
    if args.split not in dataset.splits:
        available = ", ".join(dataset.splits)
        raise SystemExit(
            f"{dataset.name} does not provide split {args.split!r}; available: {available}"
        )
    archive = args.cache_dir / "downloads" / f"{dataset.name}.zip"
    source_dir = args.cache_dir / dataset.name / "source"
    download(dataset, archive, force=args.force_download)
    extract(dataset, archive, source_dir)
    corpus, queries, qrels = load_and_validate(dataset, source_dir, args.split)
    print(
        f"Validated {dataset.name}/{args.split}: {len(corpus)} documents, "
        f"{len(queries)} queries, {len(qrels)} qrels"
    )
    if args.download_only:
        return

    default_output = "vectorkit" if args.split == "test" else f"vectorkit-{args.split}"
    output = args.output or args.cache_dir / dataset.name / default_output
    embedder = CoreMlMiniLmEmbedder(args.model_dir)
    prepare_collection(
        dataset,
        corpus,
        queries,
        qrels,
        output,
        embedder,
        args.evaluation_depth,
        args.split,
    )
    print(f"Wrote VectorKit collection to {output}")


def download(dataset: Dataset, archive: Path, *, force: bool) -> None:
    archive.parent.mkdir(parents=True, exist_ok=True)
    if archive.exists() and not force:
        verify_checksum(dataset, archive)
        return
    temporary = archive.with_suffix(".download")
    temporary.unlink(missing_ok=True)
    print(f"Downloading {dataset.url}")
    try:
        with (
            urllib.request.urlopen(dataset.url, timeout=60) as response,
            temporary.open("wb") as target,
        ):
            shutil.copyfileobj(response, target)
        verify_checksum(dataset, temporary)
        temporary.replace(archive)
    finally:
        temporary.unlink(missing_ok=True)


def verify_checksum(dataset: Dataset, path: Path) -> None:
    digest = hashlib.md5(usedforsecurity=False)
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    actual = digest.hexdigest()
    if actual != dataset.checksum:
        raise SystemExit(
            f"checksum mismatch for {path}: expected {dataset.checksum}, found {actual}"
        )


def extract(dataset: Dataset, archive: Path, source_dir: Path) -> None:
    marker = source_dir / ".checksum"
    if (
        marker.exists()
        and marker.read_text(encoding="utf-8").strip() == dataset.checksum
    ):
        return
    if source_dir.exists():
        shutil.rmtree(source_dir)
    source_dir.mkdir(parents=True)
    with zipfile.ZipFile(archive) as bundle:
        prefix = f"{dataset.name}/"
        for member in bundle.infolist():
            if not member.filename.startswith(prefix):
                raise SystemExit(f"unexpected archive member: {member.filename}")
            relative = Path(member.filename).relative_to(dataset.name)
            if ".." in relative.parts:
                raise SystemExit(f"unsafe archive member: {member.filename}")
            destination = source_dir / relative
            if member.is_dir():
                destination.mkdir(parents=True, exist_ok=True)
                continue
            destination.parent.mkdir(parents=True, exist_ok=True)
            with bundle.open(member) as source, destination.open("wb") as target:
                shutil.copyfileobj(source, target)
    marker.write_text(dataset.checksum + "\n", encoding="utf-8")


def load_and_validate(
    dataset: Dataset, source_dir: Path, split: str
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[tuple[str, str, int]]]:
    corpus = read_jsonl(source_dir / "corpus.jsonl")
    all_queries = read_jsonl(source_dir / "queries.jsonl")
    expected = dataset.splits[split]
    qrels = read_beir_qrels(source_dir / f"qrels/{split}.tsv")
    split_query_ids = {query_id for query_id, _, _ in qrels}
    queries = [query for query in all_queries if str(query["_id"]) in split_query_ids]
    if len(corpus) != dataset.corpus_count:
        raise SystemExit(
            f"{dataset.name} corpus count mismatch: expected {dataset.corpus_count}, "
            f"found {len(corpus)}"
        )
    if len(queries) != expected.query_count:
        raise SystemExit(
            f"{dataset.name}/{split} query count mismatch: "
            f"expected {expected.query_count}, found {len(queries)}"
        )
    if len(qrels) != expected.qrels_count:
        raise SystemExit(
            f"{dataset.name}/{split} qrels count mismatch: "
            f"expected {expected.qrels_count}, found {len(qrels)}"
        )
    document_ids = {str(document["_id"]) for document in corpus}
    query_ids = {str(query["_id"]) for query in queries}
    if len(document_ids) != len(corpus):
        raise SystemExit(f"{dataset.name} corpus contains duplicate document IDs")
    if len(query_ids) != len(queries):
        raise SystemExit(f"{dataset.name}/{split} contains duplicate query IDs")
    if any("text" not in document for document in corpus):
        raise SystemExit(f"{dataset.name} corpus contains a document without text")
    if any("text" not in query for query in queries):
        raise SystemExit(f"{dataset.name}/{split} contains a query without text")
    if len({(query_id, document_id) for query_id, document_id, _ in qrels}) != len(
        qrels
    ):
        raise SystemExit(
            f"{dataset.name}/{split} qrels contain duplicate query/document pairs"
        )
    for identifier in document_ids | query_ids:
        validate_trec_token(dataset.name, identifier)
    if any(grade < 0 or grade > 127 for _, _, grade in qrels):
        raise SystemExit(f"{dataset.name} qrels contain a grade outside 0 through 127")
    positive_query_ids = {query_id for query_id, _, grade in qrels if grade > 0}
    if positive_query_ids != query_ids:
        missing = sorted(query_ids - positive_query_ids)
        raise SystemExit(
            f"{dataset.name} queries without positive qrels: {', '.join(missing)}"
        )
    for query_id, document_id, _ in qrels:
        if query_id not in query_ids or document_id not in document_ids:
            raise SystemExit(
                f"qrels reference missing pair query={query_id!r}, document={document_id!r}"
            )
    return corpus, queries, qrels


def validate_trec_token(dataset: str, value: str) -> None:
    if not value or any(character.isspace() for character in value) or "\0" in value:
        raise SystemExit(f"{dataset} identifier is not a valid TREC token: {value!r}")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line
    ]


def read_beir_qrels(path: Path) -> list[tuple[str, str, int]]:
    rows: list[tuple[str, str, int]] = []
    for offset, line in enumerate(path.read_text(encoding="utf-8").splitlines()):
        if offset == 0 and line.startswith("query-id"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            raise SystemExit(f"invalid BEIR qrels at {path}:{offset + 1}")
        rows.append((fields[0], fields[1], int(fields[2])))
    rows.sort()
    return rows


class CoreMlMiniLmEmbedder:
    def __init__(self, model_dir: Path) -> None:
        try:
            import coremltools as ct
            import numpy as np
            from transformers import AutoTokenizer
        except ImportError as error:
            raise SystemExit(
                "embedding generation requires coremltools, numpy, and transformers; "
                "use target/embedding-conversion-venv/bin/python"
            ) from error
        packages = sorted(model_dir.glob("*.mlpackage"))
        if len(packages) != 1:
            raise SystemExit(f"expected one .mlpackage in {model_dir}")
        self._np = np
        self._tokenizer = AutoTokenizer.from_pretrained(
            model_dir / "tokenizer", local_files_only=True
        )
        self._model = ct.models.MLModel(
            str(packages[0]), compute_units=ct.ComputeUnit.ALL
        )
        self.sequence_length = 256
        self.dimension = 384

    def embed(self, text: str) -> list[float]:
        encoded = self._tokenizer(
            text,
            max_length=self.sequence_length,
            padding="max_length",
            truncation=True,
            return_tensors="np",
        )
        inputs = {
            "input_ids": encoded["input_ids"].astype(self._np.int32),
            "attention_mask": encoded["attention_mask"].astype(self._np.int32),
        }
        if "token_type_ids" in encoded:
            inputs["token_type_ids"] = encoded["token_type_ids"].astype(self._np.int32)
        prediction = self._model.predict(inputs)
        vector = self._np.asarray(
            prediction["embedding"], dtype=self._np.float32
        ).reshape(-1)
        if vector.size != self.dimension:
            raise SystemExit(
                f"embedding dimension mismatch: expected {self.dimension}, found {vector.size}"
            )
        norm = float(self._np.linalg.norm(vector))
        if norm > 0:
            vector /= norm
        return [round(float(value), 8) for value in vector]


def prepare_collection(
    dataset: Dataset,
    corpus: list[dict[str, Any]],
    queries: list[dict[str, Any]],
    qrels: list[tuple[str, str, int]],
    output: Path,
    embedder: CoreMlMiniLmEmbedder,
    evaluation_depth: int,
    split: str,
) -> None:
    output.mkdir(parents=True, exist_ok=True)
    documents_started = time.perf_counter()
    documents = []
    for offset, document in enumerate(
        sorted(corpus, key=lambda item: str(item["_id"]))
    ):
        text = combine_title_and_text(document)
        documents.append(
            {
                "id": str(document["_id"]),
                "text": text,
                "metadata": {"dataset": dataset.name, "split": split},
                "embedding": embedder.embed(text),
            }
        )
        report_progress("documents", offset + 1, len(corpus))
    documents_seconds = time.perf_counter() - documents_started
    queries_started = time.perf_counter()
    prepared_queries = []
    for offset, query in enumerate(sorted(queries, key=lambda item: str(item["_id"]))):
        text = str(query["text"])
        prepared_queries.append(
            {
                "id": str(query["_id"]),
                "category": dataset.name,
                "text": text,
                "embedding": embedder.embed(text),
            }
        )
        report_progress("queries", offset + 1, len(queries))
    queries_seconds = time.perf_counter() - queries_started

    write_jsonl(output / "documents.jsonl", documents)
    write_jsonl(output / "queries.jsonl", prepared_queries)
    write_qrels(output / "qrels.tsv", qrels)
    manifest = {
        "schema_version": 2,
        "collection_id": f"beir-{dataset.name}-{split}-minilm-l6-v2",
        "model": {
            "id": "sentence-transformers/all-MiniLM-L6-v2",
            "slug": "all-MiniLM-L6-v2",
            "sequence_length": embedder.sequence_length,
            "dimension": embedder.dimension,
        },
        "top_k": 10,
        "evaluation_depth": evaluation_depth,
        "candidate_pairs": [[evaluation_depth, evaluation_depth]],
        "default_pair": [evaluation_depth, evaluation_depth],
        "quality_gates": {},
        "documents_path": "documents.jsonl",
        "queries_path": "queries.jsonl",
        "qrels_path": "qrels.tsv",
        "embedding_provenance": {
            "generator": "scripts/quality/prepare_beir.py",
            "model": "sentence-transformers/all-MiniLM-L6-v2",
            "sequence_length": embedder.sequence_length,
            "normalized": True,
        },
        "dataset_provenance": {
            "name": dataset.name,
            "split": split,
            "source_url": dataset.url,
            "checksum": f"md5:{dataset.checksum}",
            "preprocessing": "BEIR title + two newlines + text; one VectorKit chunk per document",
            "corpus_documents": dataset.corpus_count,
            "queries": dataset.splits[split].query_count,
            "qrels": dataset.splits[split].qrels_count,
        },
    }
    write_json(output / "collection.json", manifest)
    print(
        f"Embedding time: documents={documents_seconds:.3f}s, "
        f"queries={queries_seconds:.3f}s (excluded from retrieval latency)"
    )


def combine_title_and_text(document: dict[str, Any]) -> str:
    title = str(document.get("title", "")).strip()
    text = str(document.get("text", "")).strip()
    return f"{title}\n\n{text}" if title else text


def report_progress(label: str, current: int, total: int) -> None:
    if current == total or current % 100 == 0:
        print(f"Embedded {current}/{total} {label}")


def write_jsonl(path: Path, rows: Iterable[dict[str, Any]]) -> None:
    text = "".join(
        json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n" for row in rows
    )
    path.write_text(text, encoding="utf-8")


def write_qrels(path: Path, qrels: Iterable[tuple[str, str, int]]) -> None:
    path.write_text(
        "".join(
            f"{query_id} 0 {document_id} {grade}\n"
            for query_id, document_id, grade in qrels
        ),
        encoding="utf-8",
    )


def write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
