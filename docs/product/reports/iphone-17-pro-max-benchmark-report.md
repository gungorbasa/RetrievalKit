# iPhone 17 Pro Max Benchmark Report

Source file:
`/Users/gungorbasa/Downloads/text-E0D445D7597E-1.txt`

Run status: `ok: true`

## Benchmark Setup

| Setting | Value |
|---|---:|
| Device | iPhone 17 Pro Max |
| Chunks | 24,000 |
| Dimensions | 384, 768 |
| Queries | 200 |
| Top K | 10 |
| Metric | Cosine |
| Encodings | F32, F16, I8ScalarQuantized |
| Filtered run | `filter_every=10`, roughly 10% candidate selectivity |
| Total wall time | 25.66 s |

Detected CPU/vector capabilities:

| Capability | Value |
|---|---|
| AArch64 dot product | `true` |
| SimSIMD | `neon,neon_f16,neon_i8,dynamic` |

The important point: this device exposes both AArch64 dot product support and
SimSIMD `neon_i8`, so it can use the fast integer dot-product path that matters
most for I8ScalarQuantized.

## Executive Summary

RetrievalKit is comfortably inside the V1 retrieval-latency target on iPhone 17 Pro
Max for 24K exact-search indexes.

For unfiltered exact vector search:

| Dimension | F32 Avg | F16 Avg | I8 Avg | I8 Speedup vs F32 | I8 Recall vs F32 |
|---:|---:|---:|---:|---:|---:|
| 384 | 1.469 ms | 1.596 ms | 0.508 ms | 2.89x | 98.95% |
| 768 | 3.462 ms | 3.565 ms | 0.652 ms | 5.31x | 99.20% |

For filtered exact vector search:

| Dimension | F32 Avg | F16 Avg | I8 Avg | I8 Speedup vs F32 Unfiltered | I8 Recall vs F32 |
|---:|---:|---:|---:|---:|---:|
| 384 | 0.415 ms | 0.450 ms | 0.238 ms | 6.18x | 98.85% |
| 768 | 0.692 ms | 0.726 ms | 0.323 ms | 10.71x | 99.20% |

This confirms the current direction: I8ScalarQuantized is the best V1 mobile
encoding when the app can tolerate about 99% recall against F32. It is much
smaller than F32/F16 and materially faster on this iPhone.

## Full Results

### 384 Dimensions

| Encoding | Filter | Avg | P50 | P95 | Max | Total Payload | Vector Payload | Recall vs F32 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| F32 | No | 1.469 ms | 1.449 ms | 1.559 ms | 3.038 ms | 39.85 MiB | 35.16 MiB | 100.00% |
| F32 | Yes | 0.415 ms | 0.408 ms | 0.489 ms | 1.354 ms | 41.06 MiB | 35.16 MiB | 100.00% |
| F16 | No | 1.596 ms | 1.579 ms | 1.668 ms | 1.953 ms | 22.27 MiB | 17.58 MiB | 99.95% |
| F16 | Yes | 0.450 ms | 0.449 ms | 0.496 ms | 0.655 ms | 23.48 MiB | 17.58 MiB | 99.95% |
| I8 | No | 0.508 ms | 0.503 ms | 0.565 ms | 0.685 ms | 13.57 MiB | 8.88 MiB | 98.95% |
| I8 | Yes | 0.238 ms | 0.238 ms | 0.262 ms | 0.284 ms | 14.79 MiB | 8.88 MiB | 98.85% |

Filter benefit at 384d:

| Encoding | Filter Speedup | Latency Reduction |
|---|---:|---:|
| F32 | 3.54x | 71.8% |
| F16 | 3.55x | 71.8% |
| I8 | 2.14x | 53.2% |

### 768 Dimensions

| Encoding | Filter | Avg | P50 | P95 | Max | Total Payload | Vector Payload | Recall vs F32 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| F32 | No | 3.462 ms | 3.363 ms | 3.914 ms | 4.999 ms | 75.00 MiB | 70.31 MiB | 100.00% |
| F32 | Yes | 0.692 ms | 0.690 ms | 0.731 ms | 0.826 ms | 76.22 MiB | 70.31 MiB | 100.00% |
| F16 | No | 3.565 ms | 3.541 ms | 3.722 ms | 4.602 ms | 39.85 MiB | 35.16 MiB | 100.00% |
| F16 | Yes | 0.726 ms | 0.722 ms | 0.769 ms | 1.288 ms | 41.06 MiB | 35.16 MiB | 100.00% |
| I8 | No | 0.652 ms | 0.636 ms | 0.716 ms | 1.110 ms | 22.36 MiB | 17.67 MiB | 99.20% |
| I8 | Yes | 0.323 ms | 0.322 ms | 0.353 ms | 0.405 ms | 23.58 MiB | 17.67 MiB | 99.20% |

Filter benefit at 768d:

| Encoding | Filter Speedup | Latency Reduction |
|---|---:|---:|
| F32 | 5.00x | 80.0% |
| F16 | 4.91x | 79.6% |
| I8 | 2.02x | 50.4% |

## Interpretation

### 1. I8 is the clear speed winner

I8ScalarQuantized is faster because it reads far less vector data and can use
integer dot-product instructions. At 768 dimensions, it drops average
unfiltered latency from 3.462 ms for F32 to 0.652 ms.

That is a 5.31x speedup while preserving 99.20% recall@10 against F32 on this
synthetic benchmark.

### 2. I8 is also the clear size winner

At 384 dimensions:

| Encoding | Total Payload |
|---|---:|
| F32 | 39.85 MiB |
| F16 | 22.27 MiB |
| I8 | 13.57 MiB |

At 768 dimensions:

| Encoding | Total Payload |
|---|---:|
| F32 | 75.00 MiB |
| F16 | 39.85 MiB |
| I8 | 22.36 MiB |

The 384d I8 index fits under the current 20 MB persisted-size target in this
synthetic benchmark. The 768d I8 index is close, but still above the 20 MB
target once non-vector payload is included.

### 3. F16 saves size, but does not improve speed here

F16 cuts vector payload roughly in half, but it is slightly slower than F32 in
these runs:

| Dimension | F32 Avg | F16 Avg |
|---:|---:|---:|
| 384 | 1.469 ms | 1.596 ms |
| 768 | 3.462 ms | 3.565 ms |

That means F16 is useful mainly as a quality-preserving size reduction, not as a
latency optimization on this device.

### 4. Filtering is already very effective

The filtered runs are much faster because the metadata filter narrows the
candidate offsets before vector scoring.

At 768d:

| Encoding | Unfiltered Avg | Filtered Avg |
|---|---:|---:|
| F32 | 3.462 ms | 0.692 ms |
| F16 | 3.565 ms | 0.726 ms |
| I8 | 0.652 ms | 0.323 ms |

This is important for real apps because most useful local retrieval flows have
natural filters: notebook, collection, account, file type, recency bucket,
document status, or user-selected scope.

### 5. The latency target is met

The V1 target is roughly 5-10 ms retrieval-only latency for fewer than 50K chunks
with 384d or 768d vectors.

On iPhone 17 Pro Max:

| Scenario | Worst Relevant P95 |
|---|---:|
| 384d F32 unfiltered | 1.559 ms |
| 768d F32 unfiltered | 3.914 ms |
| 384d I8 unfiltered | 0.565 ms |
| 768d I8 unfiltered | 0.716 ms |
| 768d I8 filtered | 0.353 ms |

Even the slowest exact-search case in this report is below 5 ms P95.

## Product Recommendation

Use I8ScalarQuantized as the primary mobile performance path for V1.

Recommended defaults:

| Use Case | Recommended Encoding | Reason |
|---|---|---|
| Best quality / debugging baseline | F32 | Ground truth and deterministic reference |
| Quality-preserving smaller storage | F16 | Similar recall, about half F32 vector size |
| Mobile production default | I8ScalarQuantized | Fastest and smallest with about 99% recall |

For the first production-facing mobile path, prioritize:

1. I8ScalarQuantized indexes for 384d embeddings.
2. Indexed metadata filters where app flows naturally provide scope.
3. F32 as the internal benchmark and correctness baseline.
4. F16 as an opt-in storage compromise when callers want near-F32 quality but do
   not want full F32 size.

## Next Technical Step

The next bottleneck is no longer raw vector scoring for 24K chunks on high-end
iPhone hardware. The next useful work is to measure and reduce total local
index footprint and app-level overhead:

1. Add benchmark output for persistence save size by file/component.
2. Add load-time measurement for persisted indexes.
3. Add peak memory/RSS or approximate resident payload reporting.
4. Run the same iPhone benchmark with realistic chunk text and metadata.

This will tell us whether the next optimization should target binary layout,
chunk metadata/text storage, BM25 payload size, load time, or memory residency.
