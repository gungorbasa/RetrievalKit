# Hybrid Performance Milestone V1 Report

Status: Rust optimization implemented; Mac public-path confirmation complete;
physical-iPhone confirmation remains required before replacing the frozen
iPhone benchmark evidence.

Date: 2026-08-14

## Purpose

The completed Apple end-to-end benchmark measured 50K weighted-hybrid
retrieval P95 at `19.929 ms` on iPhone 17 Pro Max, above the product's
retrieval-only `15 ms` qualification-boundary target. This milestone isolates
the production hybrid stages at 25K and 49,999 active chunks, optimizes only
the measured bottleneck, and confirms the result through the public Swift API.

This work does not change ranking semantics, candidate defaults, the
fewer-than-50K support envelope, or the deferred ANN/HNSW decision.

## Stage benchmark

`crates/retrievalkit-core/benches/hybrid_stage_profile.rs` builds deterministic
I8/cosine indexes using the Apple end-to-end benchmark's realistic corpus text
shape. It measures the real production hybrid path with top K 10, 50 vector
candidates, 50 BM25 candidates, and alpha `0.6`. Embedding is excluded.

The benchmark reports filter planning, vector candidate generation, BM25
candidate generation, fusion, final hit hydration, and total retrieval. It
runs both unfiltered queries and an indexed `team = Atlas` filter. The
instrumentation is behind the off-by-default `benchmark-instrumentation` Cargo
feature and is not exposed through language wrappers. Schema version 2 hashes
the complete ordered hit payload, including exact floating-point bits, trace
fields, matched terms, and query boundaries, for future result comparisons.

Reference environment:

- `MacBookPro18,4`, Apple M1 Max, 32 GB;
- macOS 26.5.2 (`25F84`), arm64;
- Rust release mode, I8 scalar-quantized database vectors; and
- five deterministic semantic, identifier, mixed, and long common-term query
  shapes.

Command:

```bash
RETRIEVALKIT_PROFILE_WARMUP=50 \
RETRIEVALKIT_PROFILE_SAMPLES=300 \
cargo bench -q -p retrievalkit-core \
  --features benchmark-instrumentation \
  --bench hybrid_stage_profile
```

## Bottleneck and change

The baseline showed that fusion and hydration were not material. At 49,999
chunks, BM25 consumed `41.348 ms` of the `42.189 ms` unfiltered P95. The prior
BM25 query path cloned every matched query term into every matching chunk's
trace buffer before bounded top-K selection. Long queries containing common
terms caused hundreds of thousands of short-lived string allocations even
though only 50 keyword candidates survived.

The optimized path now:

1. accumulates numeric BM25 scores without constructing per-chunk trace
   strings;
2. performs deterministic bounded top-K selection;
3. reconstructs matched-term traces only for surviving hits through sorted
   posting-list lookup; and
4. keeps postings sorted across monotonic inserts, out-of-order inserts, and
   replacements.

Fresh chunk insertion also skips the replacement scan when the chunk ID has no
prior BM25 state. Hybrid queries build an indexed metadata-filter plan once and
share it across vector and BM25 candidate generation.

## Rust before and after

The comparable baseline and optimized runs each used 20 warmups and 100
samples. Their original ranked chunk-ID digests remained identical for every
same-size/scenario comparison; the stronger complete-hit digest described
above was added after review and is therefore not retroactively compared with
those baseline runs.

| Chunks | Scenario | Stage | Baseline P95 | Optimized P95 | Change |
| ---: | --- | --- | ---: | ---: | ---: |
| 25,000 | Unfiltered | BM25 | 16.602 ms | 5.523 ms | -66.7% |
| 25,000 | Unfiltered | Total hybrid | 17.061 ms | 6.015 ms | -64.7% |
| 25,000 | `team = Atlas` | Total hybrid | 6.094 ms | 4.716 ms | -22.6% |
| 49,999 | Unfiltered | BM25 | 41.348 ms | 12.101 ms | -70.7% |
| 49,999 | Unfiltered | Total hybrid | 42.189 ms | 13.009 ms | -69.2% |
| 49,999 | `team = Atlas` | Total hybrid | 12.865 ms | 10.279 ms | -20.1% |

Three subsequent 50-warmup/300-sample sessions confirmed 49,999 unfiltered
total P95 at `12.729`, `12.821`, and `12.650 ms`. BM25 P95 was `11.851`,
`11.900`, and `11.764 ms`. The result digest was identical across all three
sessions.

## Public Swift/Core ML confirmation

The base XCFramework was rebuilt from the optimized Rust core. Three fresh
Mac processes then ran the frozen 50K FP32-production weighted-hybrid scenario
through:

```text
query text -> Core ML embedding -> public Swift hybridSearch
           -> Rust vector/BM25/fusion -> decoded Swift top-10 hits
```

The existing 50K corpus, query suite, schedule, tokenizer, model, and persisted
I8 index were unchanged. The independent Apple benchmark validator accepted
all three reports, 2,250 retained samples, three process IDs, raw timing
nesting, summaries, and result-identity stability.

| Metric | Frozen pre-change median-session P95 | Optimized median-session P95 | Change |
| --- | ---: | ---: | ---: |
| Retrieval only | 50.975 ms | 14.667 ms | -71.2% |
| Direct text-to-results total | 59.833 ms | 22.760 ms | -62.0% |

The optimized retrieval result is below the 50K `15 ms` retrieval-only target
on this Mac qualification run. It does not replace the frozen physical-iPhone
result. A new, explicitly authorized iPhone run must rebuild the device
artifact from this revision, execute three fresh weighted-hybrid sessions, and
pass the independent validator before claiming that the iPhone miss is closed.

## Correctness and remaining risk

- Rust BM25, hybrid, filter, deletion, replacement, persistence, and
  cross-wrapper fixture tests pass.
- Ranked chunk-ID digests were unchanged in the original before/after runs;
  production/profiled-path parity tests cover the complete hit payload, and
  new profiler runs use the stronger schema-v2 digest.
- Fusion weights, candidate limits, final ordering, scores, and matched-term
  traces remain public-contract compatible.
- The new benchmark corpus is realistic deterministic application text, not a
  human-judged relevance collection.
- Physical-iPhone performance, energy, and thermal behavior are not inferred
  from the Mac result.
