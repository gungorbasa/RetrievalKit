# Documentation

This directory separates active product decisions from deferred research notes.

## Product

Current implementation source of truth:

- [`product/vectorkit-product-spec.md`](product/vectorkit-product-spec.md)
- [`product/working-memory.md`](product/working-memory.md) for active handoff
  context and recent decisions that should survive chat changes
- [`product/memory-benchmark.md`](product/memory-benchmark.md) for isolated RSS,
  persistence, search, and compaction validation
- [`product/reports/retrieval-quality-v1-report.md`](product/reports/retrieval-quality-v1-report.md)
  for relevance, encoding-recall, and hybrid candidate-limit evidence
- [`product/size-speed-report.md`](product/size-speed-report.md) for the current
  compact-index footprint and retrieval-speed analysis

## Research

Technical explorations that are not part of the current V1 scope:

- [`research/rust-hnsw-vector-search-plan.md`](research/rust-hnsw-vector-search-plan.md)
- [`research/turbovec-notes.md`](research/turbovec-notes.md)

## Current Direction

VectorKit V1 is a small-index local retrieval SDK:

```text
target size: fewer than 50K chunks
primary engine: exact vector search
retrieval: exact vector + BM25 + hybrid ranking
priority: speed, correctness, filtering, persistence, Swift/iOS integration
```

HNSW/ANN work is deferred until exact/hybrid retrieval is polished and benchmarked on real iOS datasets.
