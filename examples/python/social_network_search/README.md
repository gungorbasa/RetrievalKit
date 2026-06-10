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
  `Index.search(query_embedding, limit=..., where=...)`.

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

Run a query against the saved index:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "Mark and Erica arguing in a dim bar" \
  --limit 5
```

Filter to shots only:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --query "shots on the Harvard campus at night" \
  --where-kind shot
```

The saved index is written to:

```text
target/examples/social-network-index/
```
