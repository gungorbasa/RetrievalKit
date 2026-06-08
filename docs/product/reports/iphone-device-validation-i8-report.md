# iPhone Device Validation I8 Report

Source files:

- `/Users/gungorbasa/Downloads/text-DB3AB247094D-1.txt`
- `/Users/gungorbasa/Downloads/text-F9C6703B7B5C-1.txt`

Run status: `ok: true`

## Benchmark Setup

| Setting | Value |
|---|---:|
| Device label | iPhone |
| OS | iOS 26.5 |
| Chunks | 24,000 |
| Dimensions | 384, 768 |
| Queries | 200 |
| Top K | 10 |
| Metric | Cosine |
| Encoding | I8ScalarQuantized |
| Filtered run | `filter_every=10`, roughly 10% candidate selectivity |
| Persistence | BM25 included |
| Recall ground truth | Disabled for memory validation |
| Total wall time | 5.07 s |

Detected CPU/vector capabilities:

| Capability | Value |
|---|---|
| AArch64 dot product | `true` |
| SimSIMD | `neon,neon_f16,neon_i8,dynamic` |

This device exposes both AArch64 dot-product support and SimSIMD `neon_i8`, so
the I8ScalarQuantized path can use the fast integer dot-product backend.

## Results

| Dimension | Filter | Avg | P50 | P95 | Max | Post-load Avg | Post-load P95 | Save | Load | Persisted | RSS After Load |
|---:|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 384 | No | 0.471 ms | 0.467 ms | 0.506 ms | 0.831 ms | 0.461 ms | 0.496 ms | 18.52 ms | 22.54 ms | 12.772 MiB | 180.33 MiB |
| 384 | 1/10 | 0.111 ms | 0.102 ms | 0.173 ms | 0.271 ms | 0.105 ms | 0.162 ms | 18.85 ms | 28.68 ms | 12.910 MiB | 214.89 MiB |
| 768 | No | 0.640 ms | 0.635 ms | 0.658 ms | 1.030 ms | 0.634 ms | 0.733 ms | 23.96 ms | 25.62 ms | 21.561 MiB | 255.34 MiB |
| 768 | 1/10 | 0.167 ms | 0.149 ms | 0.256 ms | 0.304 ms | 0.174 ms | 0.259 ms | 19.57 ms | 27.83 ms | 21.699 MiB | 249.27 MiB |

Persisted file breakdown:

| Dimension | Filter | Total | Vectors | Chunks | BM25 | Tombstones | Manifest |
|---:|:---|---:|---:|---:|---:|---:|---:|
| 384 | No | 12.772 MiB | 8.881 MiB | 1.751 MiB | 2.118 MiB | 0.023 MiB | 0.000 MiB |
| 384 | 1/10 | 12.910 MiB | 8.881 MiB | 1.888 MiB | 2.118 MiB | 0.023 MiB | 0.000 MiB |
| 768 | No | 21.561 MiB | 17.670 MiB | 1.751 MiB | 2.118 MiB | 0.023 MiB | 0.000 MiB |
| 768 | 1/10 | 21.699 MiB | 17.670 MiB | 1.888 MiB | 2.118 MiB | 0.023 MiB | 0.000 MiB |

## Interpretation

### 1. Retrieval latency is comfortably inside the V1 target

The V1 retrieval-only target is roughly 5-10 ms on modern iPhone hardware. This
I8-only physical-device run is far below that:

| Scenario | P95 |
|---|---:|
| 384d unfiltered | 0.506 ms |
| 384d filtered | 0.173 ms |
| 768d unfiltered | 0.658 ms |
| 768d filtered | 0.256 ms |

Post-load latency is stable and close to warm in-memory latency. That suggests
the current load path reconstructs the expected fast search structures.

### 2. 384d I8 is validated as the first compact V1 target

`24K x 384d I8ScalarQuantized` persists at `12.772 MiB` unfiltered and
`12.910 MiB` with the synthetic filter metadata. This leaves meaningful
headroom under the `20 MiB` package target for realistic app metadata,
additional manifest fields, and format overhead.

### 3. 768d I8 is fast, but still misses the size target

`24K x 768d I8ScalarQuantized` persists at `21.561 MiB` unfiltered and
`21.699 MiB` with filter metadata. The latency is excellent, but the package is
about `1.56-1.70 MiB` over a `20 MiB` target and about `2.49-2.63 MiB` over a
strict decimal `20 MB` target.

The vector payload is the dominant cost:

| Dimension | Vector Payload | Total Persisted |
|---:|---:|---:|
| 384 | 8.881 MiB | 12.772 MiB |
| 768 | 17.670 MiB | 21.561 MiB |

For 768d under the current target, the next useful experiments are lower-bit
candidate encodings, optional BM25 omission/rebuild, or reducing the default
chunk target.

### 4. Filtering materially improves latency

The indexed equality filter reduces candidate work substantially:

| Dimension | Unfiltered Avg | Filtered Avg | Speedup |
|---:|---:|---:|---:|
| 384 | 0.471 ms | 0.111 ms | 4.23x |
| 768 | 0.640 ms | 0.167 ms | 3.83x |

This supports keeping metadata filter indexing as a first-class V1 feature.

### 5. RSS needs isolated measurement before drawing per-index conclusions

The RSS values in this report are process-level observations from one
sequential app run. They include the Swift app, framework code, allocator
retention, temporary build/save/load allocations, and previous benchmark rows.

They are useful as a sanity check that the app does not crash or balloon
unboundedly, but they should not be treated as isolated per-index memory
footprints. For accurate memory decisions, run one scenario per fresh process or
add a mode that executes exactly one dimension/filter pair after app launch.

## Product Recommendation

Use `I8ScalarQuantized` with 384-dimensional embeddings as the first practical
V1 mobile default.

Keep `768d I8` supported and documented as a fast higher-quality option, but do
not claim it satisfies the current `20 MiB` package target at 24K chunks with
BM25 persisted.

## Next Technical Work

1. Add isolated device benchmark presets for one run per launch:
   `384d I8 unfiltered`, `384d I8 filtered`, `768d I8 unfiltered`, and
   `768d I8 filtered`.
2. Add compact `768d` experiments:
   `persist_bm25=false`, binary or 2/4-bit candidate encoding, and optional
   small F16 rerank store.
3. Add a realistic fixture benchmark with real chunk text, metadata, and BM25
   distributions.
4. Add a generated report command that turns raw JSON into this markdown table
   automatically.
