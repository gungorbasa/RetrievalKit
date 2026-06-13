# Top-K Maintenance Benchmark Report

Run date: 2026-06-13

Environment:

| Setting | Value |
|---|---:|
| Host | MacBookPro18,4 |
| CPU | Apple M1 Max |
| OS | macOS 26.5.1 arm64 |
| Build | `cargo run --release` |
| Candidates per query | 50,000 |
| Queries | 1,000 |
| Top K | 5, 10, 50, 100 |

Command:

```bash
cargo run --release -p vectorkit-cli -- bench topk \
  --candidates 50000 \
  --queries 1000 \
  --top-k 5,10,50,100
```

## Candidate-Maintenance Results

Both algorithms produced identical checksums for every `k`, so the benchmark
compares equivalent ranked outputs.

| K | Algorithm | Avg | P50 | P95 | Max |
|---:|:---|---:|---:|---:|---:|
| 5 | bounded vec | 0.420 ms | 0.398 ms | 0.474 ms | 4.471 ms |
| 5 | binary heap | 0.059 ms | 0.054 ms | 0.071 ms | 1.259 ms |
| 10 | bounded vec | 1.184 ms | 1.126 ms | 1.377 ms | 8.433 ms |
| 10 | binary heap | 0.059 ms | 0.056 ms | 0.069 ms | 0.248 ms |
| 50 | bounded vec | 10.601 ms | 10.206 ms | 11.725 ms | 100.408 ms |
| 50 | binary heap | 0.076 ms | 0.073 ms | 0.087 ms | 0.408 ms |
| 100 | bounded vec | 22.314 ms | 21.724 ms | 25.181 ms | 77.019 ms |
| 100 | binary heap | 0.094 ms | 0.091 ms | 0.108 ms | 1.008 ms |

## Full Search Spot Check

Command:

```bash
cargo run --release -p vectorkit-cli -- bench matrix \
  --chunks 50000 \
  --dimensions 384 \
  --queries 200 \
  --top-k 5,10,50,100 \
  --search-modes vector \
  --encodings i8
```

| Chunks | Dim | K | Encoding | Avg | P50 | P95 | Max |
|---:|---:|---:|:---|---:|---:|---:|---:|
| 50,000 | 384 | 5 | i8 | 0.904 ms | 0.814 ms | 1.132 ms | 6.787 ms |
| 50,000 | 384 | 10 | i8 | 0.844 ms | 0.800 ms | 1.017 ms | 2.681 ms |
| 50,000 | 384 | 50 | i8 | 0.853 ms | 0.823 ms | 0.909 ms | 2.341 ms |
| 50,000 | 384 | 100 | i8 | 0.910 ms | 0.867 ms | 1.075 ms | 1.942 ms |

## Decision

Use a binary heap for bounded top-k maintenance in exact vector search and BM25.
The old bounded vector scan was simple, but its `O(N * k)` maintenance cost was
measurable even at `k=5` and became dominant for larger result counts. The heap
keeps final ordering deterministic by sorting the retained top-k hits before
materialization.
