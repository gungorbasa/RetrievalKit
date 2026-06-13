# Social Network End-to-End Benchmark Report

Run date: 2026-06-13

Environment:

| Setting | Value |
|---|---:|
| Host | MacBookPro18,4 |
| CPU | Apple M1 Max |
| Memory | 32 GiB |
| OS | macOS 26.5.1 arm64 |
| Python | 3.11.14 |
| Corpus | Social Network fixture |
| Chunks / vectors | 28,650 |
| Dimension | 384 |
| Encoding | I8ScalarQuantized |
| Embedding model | `BAAI/bge-small-en-v1.5` via FastEmbed |
| Search mode | Exact vector search |
| Metric | Cosine |
| Top K | 5 |
| Warmup | 50 query executions, excluded |
| Measured | 750 query executions |

Command:

```bash
target/social-network-example-venv/bin/python \
  examples/python/social_network_search/social_network_search.py \
  --end-to-end-benchmark \
  --search-mode vector \
  --limit 5 \
  --warmup-queries 50 \
  --measured-queries 750
```

## Results

End-to-end query latency includes query embedding plus exact vector search.
Model initialization and index loading are reported separately and are excluded
from per-query latency.

| System | Corpus | P50 | P95 | P99 | Mean |
|---|---:|---:|---:|---:|---:|
| VectorKit exact vector search | 28,650 chunks | 8.295 ms | 10.033 ms | 12.128 ms | 8.588 ms |

Component breakdown:

| Component | Mean | P50 | P95 | P99 | Min | Max |
|---|---:|---:|---:|---:|---:|---:|
| Embedding | 8.015 ms | 7.736 ms | 9.444 ms | 11.534 ms | 6.031 ms | 41.361 ms |
| Exact vector search | 0.571 ms | 0.559 ms | 0.630 ms | 0.789 ms | 0.529 ms | 1.296 ms |
| Embedding + search | 8.588 ms | 8.295 ms | 10.033 ms | 12.128 ms | 6.587 ms | 41.928 ms |

Setup timings:

| Phase | Time |
|---|---:|
| Model init | 111.369 ms |
| Index load | 1,101.167 ms |
| Measured benchmark wall time | 6,447.148 ms |

## Notes

This uses the Moss-style benchmark shape: warmups first, then 750 measured
query executions with `top_k=5`, and the timed query path includes embedding.
The corpus size is intentionally different: VectorKit currently uses the real
Social Network fixture with 28,650 chunks instead of a 100,000-document FAQ
corpus.

The exact vector search component remains well under 1 ms at p99 on this Mac.
The end-to-end number is dominated by FastEmbed query embedding latency.
