# VectorKit Python Wrapper

This is a thin Python wrapper around the VectorKit Rust retrieval core. Python
provides an ergonomic API, while Rust handles indexing, filtering, ranking,
persistence, and result tracing.

The wrapper does not include an embedding model. Callers provide embeddings from
the same local or remote provider for indexing and querying.

Requires Python 3.10 or newer.

## Capability API

Use `RetrievalDatabase` for new code. It matches the Rust and Swift system
architecture while keeping Python naming and data structures idiomatic:

```python
from vectorkit import (
    RetrievalConfiguration,
    RetrievalDatabaseBuilder,
    VectorIndexConfiguration,
)

builder = RetrievalDatabaseBuilder(
    corpus_id="notes",
    retrieval=RetrievalConfiguration(
        semantic=VectorIndexConfiguration(dimension=384),
    ),
)
builder.upsert(
    {
        "record": {
            "id": "note-42",
            "record_type": "Note",
            "metadata": {"project": "vectorkit"},
        },
        "chunks": [{"key": "summary", "text": "Local retrieval architecture"}],
    },
    embeddings={"summary": embedding},
)
database = builder.build()

hits = database.retrieval.semantic_search(
    query_embedding,
    where={"project": "vectorkit"},
)
```

Records and chunks remain capability-neutral; embeddings are supplied
separately by stable chunk key. Every retrieval database exposes both
`semantic_search(...)` and `hybrid_search(...)`.

The lower-level `Index` API remains available for compatibility, pipeline
integration, mutation, compaction, and direct BM25 benchmarking. It is not the
canonical capability-oriented database API.

## Local Development

From this directory:

```bash
maturin develop
```

From the repository root, run the full local wrapper check:

```bash
scripts/check-python-wrapper.sh
```

Run tests:

```bash
python -m pytest
```

Run optional type checks when `mypy` is installed in your development
environment:

```bash
python -m mypy
```

Run optional lint checks when `ruff` is installed:

```bash
python -m ruff check .
```

`mypy` and `ruff` are developer tools only. They are not runtime dependencies of
the `vectorkit` package.

## Build A Local Wheel

From the repository root:

```bash
scripts/build-python-wheel.sh
```

The script builds the wheel, installs that exact wheel into a clean smoke-test
virtual environment, and runs:

```bash
python wrappers/python/tests/smoke_installed.py
```

The wheel is written to:

```text
target/wheels/
```

Install it into another local environment:

```bash
python -m pip install "target/wheels/vectorkit-*.whl"
```

The wheel contains a compiled Rust extension, so it is specific to the platform
and Python version used to build it. For example, a macOS arm64 CPython 3.14
wheel is only for compatible macOS arm64 CPython 3.14 environments.

Use `--skip-smoke-test` only when you need to produce a wheel without validating
the installed package.

## Persistence Safety

`Index.save(path)` writes and syncs a complete immutable generation before
atomically publishing it through `manifest.json`. If a save is interrupted, the
previously published generation remains loadable. The next successful save
removes abandoned and superseded generations. Existing V1 root-file indexes
remain readable and migrate automatically on their next save.

Only one writer may save a given directory at a time. VectorKit uses an
OS-released lock, so a process crash does not leave the directory permanently
locked; a competing save fails with an actionable `PersistenceError`.

Treat the index directory as VectorKit-owned; do not modify `.snapshots` or
`manifest.json` directly.

```python
from pathlib import Path

from vectorkit import Index

path = Path("./search-index")
index = Index(dimension=384)
index.save(path)

# On the next app launch:
loaded_index = Index.load(path)
```

New saves use a checksummed V3 manifest. Validate a stored index without keeping
it loaded for search:

```python
from vectorkit import CorruptIndexError, Index

try:
    Index.validate(path)
except CorruptIndexError as error:
    print(error)  # restore a known-good copy or rebuild the index
```

V1 and V2 indexes remain readable without checksums. Their next save publishes
a V3 generation. Corrupt payloads fail with `CorruptIndexError`; invalid or
unsupported manifests fail with `UnsupportedFormatError`.

`save` returns actual persisted file sizes. It raises `PersistenceError` with
the failed operation, path, operating-system cause, and a recovery hint when the
directory cannot be written.

## Concurrency

Exact, keyword, and hybrid search release the Python GIL while Rust performs
retrieval. Multiple Python threads may search the same `Index` concurrently:

```python
from concurrent.futures import ThreadPoolExecutor

with ThreadPoolExecutor(max_workers=4) as pool:
    futures = [pool.submit(index.search, embedding, limit=10) for _ in range(4)]
    result_sets = [future.result() for future in futures]
```

Load, validation, add, delete, save, and compaction also release the GIL during
Rust-only work. Search methods hold a shared PyO3 borrow; add, delete, save, and
compaction require an exclusive mutable borrow. If mutation is attempted while
a search is active—or a search while mutation is active—PyO3 rejects the call
with `RuntimeError: Already borrowed`. Coordinate those operations in the
application instead of retrying blindly. The `Index` remains alive until every
active native call returns.

## Compaction

Updates and deletes create tombstones so search results change immediately
without rewriting the full index. Reclaim their memory before saving a smaller
snapshot:

```python
if index.tombstoned_chunk_count > 0:
    report = index.compact()
    print(
        f"removed {report['chunks_removed']} chunks; "
        f"reclaimed about {report['estimated_bytes_reclaimed']} bytes"
    )
    index.save(path)
```

`compact()` is an inexpensive no-op when there are no tombstones. It preserves
all active chunk IDs and never reuses removed IDs. The byte report estimates
in-memory payload savings; call `save()` afterward to publish a compacted disk
snapshot and receive actual persisted file sizes.

Compaction is synchronous and temporarily holds both the current and replacement
structures. Run it away from latency-sensitive work and leave memory headroom,
especially near the 50K-chunk V1 ceiling. The estimate reports retained payload
before and after compaction; it is not a peak-RSS measurement.

## Platform Wheels

The local `scripts/build-python-wheel.sh` helper builds for the platform and
Python interpreter running the command. To produce macOS, Linux, and Windows
wheels, run the wheel build on each target operating system and smoke-test the
installed package. The smoke test runs by default.

```bash
scripts/build-python-wheel.sh
```

This keeps the repository ready for platform-specific wheels without requiring a
GitHub CI workflow yet.

## Example

Run the complete local example after `maturin develop`:

```bash
python examples/pipeline_quickstart.py
```

`Pipeline` defaults to the shared Rust sentence chunker. If your embedding
provider exposes its tokenizer, pass both its exact token counter and model
limit so oversized chunks are recursively subdivided before embedding:

```python
pipeline = Pipeline(
    index,
    embed=embed,
    count_tokens=tokenizer.count_tokens,
    max_tokens=model.max_input_tokens,
)
```

Character limits are only a fallback because token counts depend on the model's
tokenizer.

```python
from vectorkit import Index, hybrid_search_text, search_text
from vectorkit.ingest import chunk_text


def embed(texts):
    # Replace with a real embedding provider. The returned vectors must match
    # the index dimension.
    return [[1.0, 0.0, 0.0, 0.0] for _ in texts]


index = Index(dimension=4, metric="cosine")

chunks = chunk_text(
    "Python wrapper stays thin. Rust performs shared ingestion and retrieval.",
    max_characters=48,
    overlap_characters=8,
    strategy="sentence",
)
texts = [chunk["text"] for chunk in chunks]
embeddings = embed(texts)

index.add(
    documents=[
        {
            "id": "doc-1",
            "metadata": {"project": "vectorkit"},
            "chunks": [
                {
                    "text": text,
                    "embedding": embedding,
                    "metadata": {"archived": False},
                }
                for text, embedding in zip(texts, embeddings)
            ],
        }
    ]
)

hits = search_text(
    index,
    "How does the wrapper work?",
    embed=embed,
    limit=5,
    where={"project": "vectorkit", "archived": False},
)

hybrid_hits = hybrid_search_text(
    index,
    "How does the wrapper work?",
    embed=embed,
    limit=5,
    where={"project": "vectorkit", "archived": False},
    vector_candidates=10,
    keyword_candidates=25,
)
```

Chunk limits and overlap are measured in Unicode characters. `start_byte` and
`end_byte` are UTF-8 byte offsets into the original string. Sentence mode
prefers sentence endings, then whitespace, and falls back to the hard character
limit. The implementation lives in Rust and is shared with the separate Swift
`VectorKitIngest` product.

Graph capabilities are intentionally absent from this base distribution. Use
the separate `vectorkit-graph` distribution in `wrappers/python-graph` when an
application needs graph-only or combined graph-and-retrieval databases. The two
native distributions must not be imported in the same Python process.

Use `TimestampMillis` when metadata must remain distinct from an ordinary
integer across ingestion, filtering, persistence, and result hydration:

```python
from vectorkit import TimestampMillis

metadata = {"captured_at": TimestampMillis(120_000)}
where = {"captured_at": {"$gte": TimestampMillis(60_000)}}
```

## Document Pipeline

The optional pipeline module composes chunking, a caller-provided embedding
provider, indexing, and hybrid text search:

```python
from vectorkit import Index
from vectorkit.pipeline import Pipeline

index = Index(dimension=384)
pipeline = Pipeline(
    index,
    embed=embed,
)

pipeline.add("note-42", note_text, metadata={"source": "notes"})
hits = pipeline.search("pricing decisions", limit=5)
```

All chunk embeddings are created and validated before the existing document is
replaced. Empty input and embedding count or dimension mismatches fail without
mutating the index.

Applications can instead pass any object implementing `chunks(text)`. This
allows Markdown-, transcript-, email-, or source-code-aware chunking while the
pipeline continues to own embedding validation and atomic document upsert.
The customization protocol lives in `vectorkit.pipeline`; the concrete
`RustTextChunker` remains available from `vectorkit.ingest` when callers want
non-default limits.

## Filter Syntax

Common filters use `where={...}`:

```python
from vectorkit import Filter

filters: Filter = {
    "project": "vectorkit",
    "archived": False,
    "created_at": {"$gte": 1710000000000},
    "source": {"$in": ["notes", "docs"]},
}

index.search(
    query_embedding,
    limit=10,
    where=filters,
)
```

## Hybrid Search

Hybrid search combines vector and BM25 keyword candidates in the Rust core:

```python
hits = index.hybrid_search(
    "python wrapper",
    query_embedding,
    limit=10,
    where={"project": "vectorkit"},
    alpha=0.6,
)
```

`limit` is the final number of fused hits. `vector_candidates` and
`keyword_candidates` control how many candidates each retrieval mode contributes
before fusion. `alpha` directly controls weighted normalized fusion: `1` is
vector-only, `0` is BM25-only, and the default `0.6` gives vector search 60% of
the blend. The candidate defaults are 50 vector and 50 keyword results. Pass
`encoding="f32"` for correctness-reference indexes.

Inputs and search results are plain dictionaries, with public `TypedDict` shapes
available for annotations:

```python
from vectorkit import DocumentInput, HybridHit, SearchHit

documents: list[DocumentInput] = [
    {
        "id": "doc-1",
        "chunks": [{"text": "python wrapper", "embedding": query_embedding}],
    }
]

index.add(documents)
vector_hits: list[SearchHit] = index.search(query_embedding)
hybrid_hits: list[HybridHit] = index.hybrid_search("python wrapper", query_embedding)
```

Complex filters can use helper constructors:

```python
from vectorkit import where

index.search(
    query_embedding,
    limit=10,
    where=where.all(
        where.eq("project", "vectorkit"),
        where.range("created_at", gte=1710000000000),
    ),
)
```
