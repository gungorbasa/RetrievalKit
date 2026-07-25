# Documentation

This directory separates active product decisions from deferred research notes.

## Guides

Start here for product choices and runnable Project Apollo examples:

- [`guides/swift.md`](guides/swift.md)
- [`guides/python.md`](guides/python.md)
- [`guides/typescript.md`](guides/typescript.md)
- [`guides/kotlin.md`](guides/kotlin.md)

## Product

Current implementation source of truth:

- [`product/retrievalkit-product-spec.md`](product/retrievalkit-product-spec.md)
- [`product/working-memory.md`](product/working-memory.md) for active handoff
  context and recent decisions that should survive chat changes
- [`product/memory-benchmark.md`](product/memory-benchmark.md) for isolated RSS,
  persistence, search, and compaction validation
- [`product/reports/retrieval-quality-v2-report.md`](product/reports/retrieval-quality-v2-report.md)
  for the active harder vector-only and hybrid quality evidence
- [`product/retrieval-quality-evaluation-standard.md`](product/retrieval-quality-evaluation-standard.md)
  for the Moss comparison, industry gold standards, and V3 evaluation plan
- [`product/reports/retrieval-quality-v1-report.md`](product/reports/retrieval-quality-v1-report.md)
  for the original 12-query baseline
- [`product/size-speed-report.md`](product/size-speed-report.md) for the current
  compact-index footprint and retrieval-speed analysis
- [`product/release-process.md`](product/release-process.md) for the guarded
  current Swift/Python release-candidate workflow; the provisional Node and
  Kotlin artifacts are source-only and are not yet publication inputs
- [`product/reports/v0.1.0-release-candidate-report.md`](product/reports/v0.1.0-release-candidate-report.md)
  for the qualified artifact identities, verification, and remaining blockers

## Research

Technical explorations that are not part of the current V1 scope:

- [`research/rust-hnsw-vector-search-plan.md`](research/rust-hnsw-vector-search-plan.md)
- [`research/turbovec-notes.md`](research/turbovec-notes.md)

## Current Direction

RetrievalKit V1 is a small-index local retrieval SDK:

```text
target size: fewer than 50K chunks
primary engine: exact vector search
retrieval: exact vector + BM25 + hybrid ranking
priority: speed, correctness, filtering, persistence, native SDK integration
```

HNSW/ANN work is deferred until exact/hybrid retrieval is polished and benchmarked on real iOS datasets.
