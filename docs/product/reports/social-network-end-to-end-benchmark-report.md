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
| Embedding model | `BAAI/bge-small-en-v1.5` via FastEmbed and `all-MiniLM-L6-v2` via Core ML |
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

| System | Corpus | Embedding runtime | Search runtime | P50 | P95 | P99 | Mean |
|---|---:|---|---|---:|---:|---:|---:|
| MiniLM Core ML + Swift exact search | 28,650 chunks | Core ML `all-MiniLM-L6-v2` seq=256 | Swift RetrievalKit | 3.439 ms | 4.042 ms | 6.028 ms | 3.527 ms |
| BGE FastEmbed + Python exact search | 28,650 chunks | FastEmbed `BAAI/bge-small-en-v1.5` | Python RetrievalKit | 8.295 ms | 10.033 ms | 12.128 ms | 8.588 ms |

MiniLM component breakdown:

| Component | Mean | P50 | P95 | P99 | Min | Max |
|---|---:|---:|---:|---:|---:|---:|
| Core ML MiniLM embedding | 3.057 ms | 2.973 ms | 3.545 ms | 5.493 ms | 2.770 ms | 16.799 ms |
| Swift exact vector search | 0.470 ms | 0.466 ms | 0.497 ms | 0.535 ms | 0.445 ms | 0.633 ms |
| Approx embedding + search | 3.527 ms | 3.439 ms | 4.042 ms | 6.028 ms | 3.215 ms | 17.432 ms |

BGE/FastEmbed component breakdown:

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

MiniLM setup timings:

| Phase | Time |
|---|---:|
| Record prep | 21,314.501 ms |
| Core ML document embedding | 118,834.370 ms |
| RetrievalKit index add | 278,908.603 ms |
| Save index | 4,409.619 ms |
| Full build | 402,297.322 ms |
| Query fixture embedding | 2,391.703 ms |
| Persisted index size | 31.346 MiB |

## Notes

This uses the Moss-style benchmark shape: warmups first, then 750 measured
query executions with `top_k=5`.
The corpus size is intentionally different: RetrievalKit currently uses the real
Social Network fixture with 28,650 chunks instead of a 100,000-document FAQ
corpus.

The MiniLM row combines separately measured Core ML query embedding and Swift
exact-search distributions. It should be replaced with a single Swift
end-to-end measurement once Swift-side tokenization/model execution is wired
into the benchmark harness.

The exact vector search component remains well under 1 ms at p99 on this Mac.
The end-to-end number is dominated by embedding latency.

See also:
`docs/product/reports/social-network-minilm-swift-search-report.md`.
