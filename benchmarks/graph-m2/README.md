# Generic M2 Graph Benchmark

> [RetrievalKit](../../README.md) › Benchmarks › Bounded graph traversal

Measures graph build, bounded traversal, candidate projection, and scoped
exact retrieval on a deterministic synthetic graph. It validates the
implementation shape; it does not establish a universal capacity envelope.

## Run

```bash
cargo bench -p retrievalkit-graph --bench bounded_traversal
```

## Method

- Domain-neutral synthetic ring/fan-out graph.
- 2,000 canonical records and record nodes.
- Four deterministic outgoing references per record: 8,000 edges.
- Query starts from one stable `NodeId` and traverses `LINKS` for one to three
  hops, producing 12 unique result nodes.
- Projection converts those record nodes to generation-bound retrieval
  candidates through the build-time projection table.
- Composed exact search ranks only that projected scope using 8-dimensional F32
  vectors and top-k 10.
- Release build, single thread, 100 warmups and 500 measured samples.
- p95 uses sorted durations and nearest-rank `ceil(0.95 * n)`.
- Embedding, core ingestion, and graph build are outside query timings. Graph
  build duration is reported separately.
- Development device: Apple M1 Max, arm64, macOS 26.5.2 (25F84). This is not a
  pinned release-qualification host.

## 2026-07-11 result

| Measurement | Result |
| --- | ---: |
| Full graph build | 12 ms |
| Bounded traversal p95 | 18 µs |
| Record-node candidate projection p95 | 1 µs |
| Scoped exact retrieval p95 | 958 ns |

These results validate the implementation shape, not a universal capacity
claim. Larger node/edge counts, degree distributions, property seeds, mobile
devices, and concurrent load require their own matrix before publishing an
envelope.
