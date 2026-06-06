# Documentation

This directory separates active product decisions from deferred research notes.

## Product

Current implementation source of truth:

- [`product/vectorkit-product-spec.md`](product/vectorkit-product-spec.md)
- [`product/working-memory.md`](product/working-memory.md) for active handoff
  context and recent decisions that should survive chat changes

## Research

Technical explorations that are not part of the current V1 scope:

- [`research/rust-hnsw-vector-search-plan.md`](research/rust-hnsw-vector-search-plan.md)

## Current Direction

VectorKit V1 is a small-index local retrieval SDK:

```text
target size: fewer than 50K chunks
primary engine: exact vector search
retrieval: exact vector + BM25 + hybrid ranking
priority: speed, correctness, filtering, persistence, Swift/iOS integration
```

HNSW/ANN work is deferred until exact/hybrid retrieval is polished and benchmarked on real iOS datasets.
