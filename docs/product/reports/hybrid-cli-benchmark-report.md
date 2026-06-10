# Hybrid CLI Benchmark Report

Run date: 2026-06-10

Environment:

| Setting | Value |
|---|---:|
| Host | Apple M1 Max |
| OS | Darwin 25.5.0 arm64 |
| Build | `cargo run --release` |
| Chunks | 10,000 |
| Dimensions | 384, 768 |
| Queries | 100 |
| Top K | 10 |
| Metric | Cosine |
| Encodings | F32, I8ScalarQuantized |
| Filter | `filter_every=10`, roughly 10% candidate selectivity |
| Search modes | Vector, keyword, hybrid weighted, hybrid RRF |

Command:

```bash
cargo run --release -p vectorkit-cli -- bench matrix \
  --chunks 10000 \
  --dimensions 384,768 \
  --queries 100 \
  --top-k 10 \
  --search-modes vector,keyword,hybrid-weighted,hybrid-rrf \
  --encodings f32,i8 \
  --filter-every 10
```

## Results

| Dim | Mode | Enc | Avg | P50 | P95 | Max | Vector Cand Avg | Keyword Cand Avg | Recall vs F32 | Payload |
|---:|:---|:---|---:|---:|---:|---:|---:|---:|---:|---:|
| 384 | vector | f32 | 0.385 ms | 0.373 ms | 0.454 ms | 1.286 ms | n/a | n/a | 1.0000 | 17.546 MiB |
| 384 | vector | i8 | 0.234 ms | 0.228 ms | 0.276 ms | 0.634 ms | n/a | n/a | 0.9920 | 6.598 MiB |
| 384 | keyword | f32 | 0.613 ms | 0.587 ms | 0.715 ms | 0.963 ms | n/a | n/a | 1.0000 | 17.546 MiB |
| 384 | keyword | i8 | 0.650 ms | 0.630 ms | 0.816 ms | 0.980 ms | n/a | n/a | 1.0000 | 6.598 MiB |
| 384 | hybrid weighted | f32 | 1.107 ms | 1.079 ms | 1.351 ms | 1.651 ms | 0.423 ms | 0.655 ms | 1.0000 | 17.546 MiB |
| 384 | hybrid weighted | i8 | 1.006 ms | 0.979 ms | 1.202 ms | 1.481 ms | 0.245 ms | 0.677 ms | 0.9920 | 6.598 MiB |
| 384 | hybrid RRF | f32 | 1.101 ms | 1.085 ms | 1.341 ms | 1.592 ms | 0.427 ms | 0.642 ms | 1.0000 | 17.546 MiB |
| 384 | hybrid RRF | i8 | 0.983 ms | 0.946 ms | 1.159 ms | 1.210 ms | 0.241 ms | 0.648 ms | 0.9920 | 6.598 MiB |
| 768 | vector | f32 | 0.549 ms | 0.535 ms | 0.639 ms | 1.382 ms | n/a | n/a | 1.0000 | 32.195 MiB |
| 768 | vector | i8 | 0.388 ms | 0.382 ms | 0.430 ms | 0.833 ms | n/a | n/a | 0.9940 | 10.260 MiB |
| 768 | keyword | f32 | 0.615 ms | 0.604 ms | 0.692 ms | 0.779 ms | n/a | n/a | 1.0000 | 32.195 MiB |
| 768 | keyword | i8 | 0.610 ms | 0.591 ms | 0.693 ms | 0.722 ms | n/a | n/a | 1.0000 | 10.260 MiB |
| 768 | hybrid weighted | f32 | 1.195 ms | 1.168 ms | 1.392 ms | 1.429 ms | 0.568 ms | 0.631 ms | 1.0000 | 32.195 MiB |
| 768 | hybrid weighted | i8 | 1.010 ms | 0.965 ms | 1.194 ms | 1.329 ms | 0.394 ms | 0.651 ms | 0.9940 | 10.260 MiB |
| 768 | hybrid RRF | f32 | 1.174 ms | 1.123 ms | 1.332 ms | 1.492 ms | 0.572 ms | 0.629 ms | 1.0000 | 32.195 MiB |
| 768 | hybrid RRF | i8 | 1.001 ms | 0.958 ms | 1.208 ms | 1.268 ms | 0.385 ms | 0.647 ms | 0.9960 | 10.260 MiB |

Every row returned `1,000` total hits, which matches `100 queries * top_k 10`.
Every row also produced the same top-hit checksum, `856350`, for this synthetic
fixture. That means the benchmark modes are returning stable target-heavy
results on this generated corpus.

## Interpretation

### 1. Hybrid latency is still comfortably low at 10K chunks

Weighted hybrid average latency stayed close to `1.0-1.2 ms`:

| Dim | Enc | Weighted Avg | Weighted P95 |
|---:|:---|---:|---:|
| 384 | f32 | 1.107 ms | 1.351 ms |
| 384 | i8 | 1.006 ms | 1.202 ms |
| 768 | f32 | 1.195 ms | 1.392 ms |
| 768 | i8 | 1.010 ms | 1.194 ms |

This is well inside the V1 retrieval-only target of roughly `5-10 ms` for local
small indexes. The run is desktop hardware, so it should not replace physical
iPhone validation, but it is a useful local regression baseline.

### 2. Weighted fusion and RRF have nearly identical latency here

Weighted normalized fusion adds min-max normalization over the fused candidate
set, but at this candidate size the overhead is not meaningful:

| Dim | Enc | Weighted Avg | RRF Avg | Difference |
|---:|:---|---:|---:|---:|
| 384 | f32 | 1.107 ms | 1.101 ms | +0.006 ms |
| 384 | i8 | 1.006 ms | 0.983 ms | +0.023 ms |
| 768 | f32 | 1.195 ms | 1.174 ms | +0.021 ms |
| 768 | i8 | 1.010 ms | 1.001 ms | +0.009 ms |

This supports keeping weighted normalized score fusion as the default from a
latency perspective. Ranking quality still needs a realistic relevance fixture.

### 3. Hybrid is roughly vector candidate time plus keyword candidate time

The hybrid rows report component timings. For example:

| Dim | Enc | Vector Cand Avg | Keyword Cand Avg | Weighted Avg |
|---:|:---|---:|---:|---:|
| 384 | f32 | 0.423 ms | 0.655 ms | 1.107 ms |
| 384 | i8 | 0.245 ms | 0.677 ms | 1.006 ms |
| 768 | f32 | 0.568 ms | 0.631 ms | 1.195 ms |
| 768 | i8 | 0.394 ms | 0.651 ms | 1.010 ms |

The remaining time is candidate union, normalization/fusion, sorting,
materialization, and trace construction.

### 4. I8 still reduces vector cost and payload

For vector-only filtered search:

| Dim | F32 Avg | I8 Avg | Speedup |
|---:|---:|---:|---:|
| 384 | 0.385 ms | 0.234 ms | 1.65x |
| 768 | 0.549 ms | 0.388 ms | 1.41x |

Payload also drops substantially:

| Dim | F32 Payload | I8 Payload |
|---:|---:|---:|
| 384 | 17.546 MiB | 6.598 MiB |
| 768 | 32.195 MiB | 10.260 MiB |

The synthetic recall overlap against F32 stayed high: `0.9920` at 384d and
`0.9940-0.9960` at 768d for i8 rows.

### 5. Keyword cost dominates hybrid i8 in this filtered fixture

In the i8 hybrid rows, vector candidate retrieval is faster than BM25 candidate
retrieval:

| Dim | I8 Vector Cand Avg | I8 Keyword Cand Avg |
|---:|---:|---:|
| 384 | 0.245 ms | 0.677 ms |
| 768 | 0.394 ms | 0.651 ms |

If hybrid latency becomes a bottleneck, BM25 candidate generation and matched
term trace construction are the first places to inspect.

## Product Recommendation

Keep weighted normalized score fusion as the default hybrid mode for now.

This benchmark does not prove weighted fusion has better ranking quality than
RRF, but it shows the latency cost is negligible at the current default
candidate sizes on this local benchmark. RRF should remain available as an
explicit deterministic fallback.

## Next Work

1. Run the same search-mode matrix at `24K` chunks to match the existing device
   validation scale.
2. Add a relevance fixture with queries where vector-only, keyword-only, and
   hybrid rankings differ meaningfully.
3. Add device-side hybrid benchmark support so the iOS validation reports can
   compare vector, keyword, weighted hybrid, and RRF hybrid on physical hardware.
4. Track BM25 candidate time separately in device JSON reports, since keyword
   candidate retrieval dominates hybrid i8 latency in this local run.
