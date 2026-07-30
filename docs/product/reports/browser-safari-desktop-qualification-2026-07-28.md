# Safari Desktop Browser Qualification — 2026-07-28

Status: Safari 26.5.2 passed the production correctness, CacheStorage,
lifecycle, provider-selection, and actual 50K signed-I8 retrieval matrix after
the owner enabled Safari WebDriver. Safari selected WebGPU embedding and
SIMD128 retrieval. Retrieval-only p95 passed at `1.940 ms`, but end-to-end p95
was `18.380 ms`. The owner accepted a Safari-specific `20 ms` reference budget
on 2026-07-28, so Safari passes. This exception does not change the general
Chrome WebGPU `15 ms` budget. Further Safari optimization is deferred. No
package, website, model artifact, tag, or RetrievalKit release was published.

## Environment and contract

- Hardware: Apple M1 Max, 10 cores, 32 GB RAM.
- OS: macOS 26.5.2 (25F84), arm64.
- Safari/safaridriver: 26.5.2 (21624.2.5.11.8).
- Node: 25.9.0.
- Rust: 1.92.0.
- Corpus: 50,000 chunks, 384 dimensions, cosine,
  `I8ScalarQuantized`, Top 10.
- Query: tokenizer-verified 32 BERT tokens.
- Samples: 50 warm-ups and 750 measured batch-one queries.
- Architecture: FP32 ONNX Runtime Web embedding and RetrievalKit SIMD128
  retrieval in separate dedicated module Workers.

## Correctness and cache result

Safari passed:

- one shared concurrent cold acquisition with exactly six artifact requests;
- interrupted-acquisition cleanup with no partial cache generation;
- missing-cache `localOnly` rejection and cached local-only load;
- corrupt model rejection, generation cleanup, and verified recovery;
- Unicode, exact 256-token truncation, and empty-input behavior;
- post-close lifecycle rejection;
- exactly 384 finite, L2-normalized F32 output values;
- deterministic actual signed-I8 Top-10 retrieval;
- dedicated Worker ownership for embedding and retrieval.

## Performance

| Boundary | Safari WebGPU + SIMD128 |
|---|---:|
| Cached initialization | 2,026.660 ms |
| First inference | 184.520 ms |
| 50K ingestion | 52,133.680 ms |
| Warm embedding p50 / p95 | 13.700 / 16.520 ms |
| Retrieval p50 / p95 | 1.520 / 1.940 ms |
| End-to-end p50 / p95 | 15.240 / 18.380 ms |

Retrieval passes the `8 ms` budget. Safari's `18.380 ms` end-to-end result
passes the owner-approved Safari-specific `20 ms` reference budget. It remains
a WebGPU result and must not be relabeled as the `25 ms` WASM compatibility
tier.

## Command and evidence

```sh
cd "/Users/gungorbasa/.codex/worktrees/34d6/Vector Search"

/usr/bin/safaridriver --enable

node scripts/embedding/qualify-browser-desktop-matrix.mjs \
  --artifacts target/python-node-embedding-cold-cache/sentence-transformers_all-MiniLM-L6-v2/c9745ed1d9f207416be6d2e6f8de32d1f16199bf \
  --output target/browser-desktop-matrix-safari-50k-accepted-policy.json \
  --browsers safari \
  --execution auto \
  --chunks 50000 \
  --timeout-ms 1800000 \
  --require-all
```

The evidence JSON SHA-256 is
`80adf52555758ff168e2a39411cedff16c0b4bba15339417cc8279c72f68bec3`.

## Remaining work

- Treat Safari WebGPU optimization as deferred improvement work; preserve the
  accepted `20 ms` reference budget until new owner-approved measurements
  supersede it.
- Run mobile Safari and Chrome qualification on real devices.
- Characterize private-browsing and natural CacheStorage pressure behavior.
- Address or explicitly accept material model download, cache, peak-memory,
  and 50K ingestion costs before browser release finalization.
