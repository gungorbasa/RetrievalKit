# M1 Candidate Scope Benchmark

This benchmark isolates retrieval latency. Index construction and query
embedding are outside the timed region. Run it with:

```bash
cargo bench -p vectorkit-core --bench graph_free_regression
cargo bench -p vectorkit-core --bench candidate_scope
```

## Method

- Corpus: 10,000 deterministic synthetic one-chunk records.
- Vectors: 384-dimensional F32, dot product.
- Query: top 10; deterministic vector and two-term text query.
- Build: Cargo `bench`/optimized release profile.
- Graph-free run: 100 warmups and 1,000 single-threaded samples per exact,
  BM25, and hybrid operation.
- Scoped run: 100 warmups and 500 single-threaded samples per operation.
- Percentile: durations sorted ascending; nearest-rank `ceil(0.95 * n)`.
- Sparse scope: 100 candidates (1%). Dense scope: 5,000 candidates (50%).
- Device for the 2026-07-11 qualification: Apple M1 Max, arm64, macOS 26.5.2
  (25F84). The machine was not an isolated/pinned performance runner, so these
  results are development evidence; the release gate must be repeated on pinned
  hardware.

## 2026-07-11 results

Graph-free p95 compares the pre-M1 commit `c6d0c99` with this branch using the
same harness and machine. The current column is the median p95 from three final
verification runs after M2; this reduces sensitivity to a single noisy run:

| Operation | Pre-M1 p95 | M1 p95 | Change |
|---|---:|---:|---:|
| Exact | 910 µs | 922 µs | +1.32% |
| BM25 | 2,199 µs | 2,253 µs | +2.46% |
| Hybrid | 3,192 µs | 3,277 µs | +2.66% |

All observed changes are within the approved <=3% development gate. Because
this host was not pinned, the result does not replace release qualification.

Scoped p95:

| Operation | Sparse 1% | Dense 50% |
|---|---:|---:|
| Exact | 12 µs | 613 µs |
| BM25 | 189 µs | 1,159 µs |
| Hybrid | 232 µs | 1,955 µs |

The adaptive representation is intentionally private. These measurements can
move its threshold without changing the public `CandidateScope` contract.
