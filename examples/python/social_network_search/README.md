# Social Network Search Example

This example builds a local VectorKit index from:

```text
/Users/gungorbasa/Desktop/the_social_network_v.1.32.json
```

It uses FastEmbed for local text embeddings and VectorKit for local retrieval.

## Embedding Architecture

- Source data: scene and shot descriptions from the JSON file.
- Chunking: mirrors the retrieval-engine processors:
  - Scene data creates one searchable chunk per non-empty scene description
    field.
  - Shot data creates one searchable section for location, temporal, people,
    objects, emotion, audio, visual, and video data, then splits long sections
    with an approximate 500-token limit and 2-section overlap.
- Embedding provider: `fastembed.TextEmbedding`.
- Model: `BAAI/bge-small-en-v1.5`.
- Dimension: `384`.
- VectorKit storage/search encoding: `i8`.
- Query path: embed query text with the same FastEmbed model, then call
  `Index.search(...)`, `Index.keyword_search(...)`, or
  `Index.hybrid_search(...)` with optional metadata filters.

For `/Users/gungorbasa/Desktop/the_social_network_v.1.32.json`, the current
chunking pass prepares:

```text
scene chunks: 1,657
shot chunks: 26,993
total chunks/vectors: 28,650
```

The original Qdrant pipeline can overwrite some generated shot chunks because
its point ID does not include the chunk number. This example keeps every chunk
under a unique VectorKit document ID, so the final vector count matches the
number of generated chunks.

The user mentioned `386` dimensions, but FastEmbed's default
`BAAI/bge-small-en-v1.5` model returns `384` dimensions. VectorKit requires the
index dimension to match the embedding provider output exactly, so this example
uses `384`.

## Setup

From the repository root:

```bash
scripts/setup-social-network-example.sh
```

The setup script creates a local environment at:

```text
target/social-network-example-venv/
```

It installs:

- the local `vectorkit` wheel
- `fastembed`
- `PyYAML`

## Build And Search

Build the index and run the default query:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --rebuild
```

Rebuild the index after changing VectorKit persistence, BM25, hybrid search, or
metadata generation. Older saved indexes may still load, but keyword and hybrid
evaluation need the BM25 side of the index to be present for full behavior.

Run a query against the saved index:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "Mark and Erica arguing in a dim bar" \
  --limit 5
```

Run vector, keyword, and hybrid retrieval side by side:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "Mark and Erica arguing in a dim bar" \
  --search-mode all \
  --limit 5
```

Run a Mac-style measured end-to-end benchmark on the saved real index. This
keeps the current `28,650` chunk fixture, uses `top_k=5`, warms up first, and
then reports embedding + exact vector search latency over `750` measured query
executions:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --end-to-end-benchmark \
  --search-mode vector \
  --limit 5 \
  --warmup-queries 50 \
  --measured-queries 750
```

Latest measured results on `MacBookPro18,4` / Apple M1 Max:

| System | Corpus | Embedding | Search | P50 | P95 | P99 | Mean |
|---|---:|---|---|---:|---:|---:|---:|
| MiniLM Core ML + Swift exact search | 28,650 chunks | `all-MiniLM-L6-v2` seq=256 | Swift VectorKit | 3.439 ms | 4.042 ms | 6.028 ms | 3.527 ms |
| BGE FastEmbed + Python exact search | 28,650 chunks | `BAAI/bge-small-en-v1.5` | Python VectorKit | 8.295 ms | 10.033 ms | 12.128 ms | 8.588 ms |

See `docs/product/reports/social-network-end-to-end-benchmark-report.md` for
the environment, commands, and embedding/search component breakdown. See
`docs/product/reports/social-network-minilm-swift-search-report.md` for the
MiniLM-backed persisted index and Swift exact-search run.

## Build MiniLM Core ML Index

The MiniLM/Core ML index builder uses the Rust `embeddingkit-coreml` embedder by
default, then writes a VectorKit Python index with the generated vectors.

Build the Rust embedder once:

```bash
cargo build -p embeddingkit-coreml --release --bin embeddingkit-coreml-embed
```

Create the database and query embedding fixture:

```bash
PYTHONPATH=wrappers/python/python \
target/embedding-conversion-venv/bin/python \
  scripts/embedding/build-minilm-social-network-index.py \
  --embedding-runtime rust-coreml \
  --compute cpuAndNeuralEngine \
  --index-dir target/examples/social-network-index-minilm \
  --queries-path target/examples/social-network-minilm-queries.json
```

Use `--embedding-runtime python-coreml` to compare against the older Python
CoreMLTools path.

Filter to shots only:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "shots on the Harvard campus at night" \
  --where-kind shot
```

Filter to a time interval using `start_time` and `end_time` metadata. The
default `overlap` mode returns chunks whose interval intersects the requested
interval:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "Mark and Erica arguing" \
  --search-mode all \
  --where-kind shot \
  --start-time 0 \
  --end-time 600
```

Use `--time-filter-mode contained` to require chunks to be fully inside the
requested interval.

The saved index is written to:

```text
target/examples/social-network-index/
```
