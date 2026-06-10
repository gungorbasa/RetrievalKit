# VectorKit Python Wrapper

This is a thin Python wrapper around the VectorKit Rust retrieval core. Python
provides an ergonomic API, while Rust handles indexing, filtering, ranking,
persistence, and result tracing.

The wrapper does not include an embedding model. Callers provide embeddings from
the same local or remote provider for indexing and querying.

## Local Development

From this directory:

```bash
maturin develop
```

## Build A Local Wheel

From the repository root:

```bash
scripts/build-python-wheel.sh
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

## Example

```python
from vectorkit import Index, hybrid_search_text, search_text


def embed(texts):
    # Replace with a real embedding provider. The returned vectors must match
    # the index dimension.
    return [[1.0, 0.0, 0.0, 0.0] for _ in texts]


index = Index(dimension=4, metric="cosine", encoding="i8")

texts = ["Python wrapper stays thin", "Rust core performs retrieval"]
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

## Filter Syntax

Common filters use `where={...}`:

```python
index.search(
    query_embedding,
    limit=10,
    where={
        "project": "vectorkit",
        "archived": False,
        "created_at": {"$gte": 1710000000000},
        "source": {"$in": ["notes", "docs"]},
    },
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
    vector_candidates=10,
    keyword_candidates=25,
    fusion="weighted",
    vector_weight=0.6,
    keyword_weight=0.4,
)
```

`limit` is the final number of fused hits. `vector_candidates` and
`keyword_candidates` control how many candidates each retrieval mode contributes
before fusion. Use `fusion="rrf"` with `rrf_k=60.0` to use reciprocal rank
fusion instead of weighted normalized score fusion.

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
