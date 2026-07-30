# Browser/WASM Portable Baseline — 2026-07-26

Status: engineering diagnostic, not browser qualification or a public
performance claim.

## Purpose

This report records the first reproducible release-mode baseline for the
portable Rust scoring path compiled to WebAssembly. It isolates generated
WASM binding and retrieval execution under Node. It does not measure a Web
Worker boundary, browser scheduling, embedding inference, browser memory, or
cross-browser behavior.

## Environment and method

- Hardware: Apple M1 Max, arm64.
- Operating system: macOS 26.5.2 (25F84).
- Rust: 1.92.0.
- Runtime: Node 25.9.0.
- Target/profile: `wasm32-unknown-unknown`, Cargo release.
- Binding generator: the `wasm-bindgen` CLI version locked by the workspace.
- Corpus: deterministic dense F32 vectors, 384 dimensions, one chunk per
  record.
- Queries: exact cosine vector, BM25 text, and hybrid with `alpha = 0.6`.
- Result count: top 10.
- Warm-up: five executions of each query type.
- Samples: 20 at 10K and 25K chunks; 10 at 50K chunks.
- Command:
  `scripts/benchmark-browser-wasm.sh <count> 384 <iterations> f32`.

Module load, ingestion, final build, and each query family are timed
separately. The benchmark source is
`crates/retrievalkit-wasm/tests/node-benchmark.cjs`.

## Results

| Chunks | Module load | Ingestion | Vector p50 / p95 | BM25 p50 / p95 | Hybrid p50 / p95 |
|---:|---:|---:|---:|---:|---:|
| 10K | 3.21 ms | 1.30 s | 3.53 / 3.76 ms | 0.06 / 0.08 ms | 3.69 / 3.90 ms |
| 25K | 2.80 ms | 10.17 s | 8.56 / 8.80 ms | 0.10 / 0.22 ms | 8.83 / 9.24 ms |
| 50K | 2.98 ms | 78.27 s | 17.10 / 17.65 ms | 0.19 / 0.54 ms | 17.82 / 18.27 ms |

The raw linked module is 1,716,728 bytes before browser-target binding output,
compression, or `wasm-opt`. Browser-target `wasm-bindgen` output reduces the
WASM payload to 1,202,036 bytes before compression or `wasm-opt`.

A 10K-chunk I8 diagnostic produced 3.72 / 3.90 ms vector p50 / p95 and
3.90 / 4.08 ms hybrid p50 / p95. The portable scalar tier therefore does not
show a query-speed advantage over F32 on this machine; compact encodings must
be evaluated primarily with browser memory and recall measurements before
changing defaults.

A 50K×192d F32 diagnostic satisfies the hard browser search gate without
changing the native libraries:

| Chunks | Dimensions | Ingestion | Vector p50 / p95 | Hybrid p50 / p95 |
|---:|---:|---:|---:|---:|
| 50K | 192 | 50.57 s | 7.43 / 7.50 ms | 7.77 / 8.11 ms |

This is scaling evidence, not a candidate replacement for the canonical 384d
native-parity profile. The browser must retain 384d I8 unless a different
embedding profile receives its own explicit quality qualification.

The matching 50K×384d I8 diagnostic produced:

| Chunks | Dimensions | Encoding | Ingestion | Vector p50 / p95 | Hybrid p50 / p95 |
|---:|---:|:---|---:|---:|---:|
| 50K | 384 | I8 per-vector symmetric | 64.12 s | 17.88 / 18.09 ms | 18.19 / 18.58 ms |

The representation already matches native behavior, but the portable WASM
lane uses a scalar signed-I8 dot product. An explicit WASM SIMD128 I8 scorer is
required to reach the ≤10 ms gate while preserving the same dimensions,
quantization, ranking, and quality contract.

## Explicit SIMD128 result

The separate SIMD128 artifact uses signed-I8 extended multiplication and I32
lane accumulation over 16 values per iteration, followed by the unchanged
per-vector scales. The Worker validates SIMD128 and selects exactly one
artifact before database construction; unsupported engines retain the portable
artifact.

| Chunks | Dimensions | Encoding | Vector p50 / p95 | BM25 p50 / p95 | Hybrid p50 / p95 |
|---:|---:|:---|---:|---:|---:|
| 25K | 384 | I8 SIMD128 | 0.88 / 0.94 ms | 0.09 / 0.13 ms | 1.10 / 1.25 ms |
| 50K | 384 | I8 SIMD128 | 1.68 / 1.80 ms | 0.17 / 0.24 ms | 2.03 / 2.20 ms |

The 50K SIMD128 result is about 10.1× faster for vector search and 8.4× faster
for hybrid search than the portable 50K I8 diagnostic. It passes the p95
≤10 ms engineering gate with substantial headroom.

The generated portable and SIMD128 modules run the same retrieval, BM25,
hybrid, graph, scoped-search, encoding, and lifecycle smoke. A direct
portable-versus-SIMD comparison additionally asserts identical complete
structured vector, BM25, and hybrid results for deterministic I8 corpora at
384 dimensions and at 396 dimensions, which exercises the SIMD scalar-tail
path.

These remain Node/WASM engineering diagnostics. Chrome, Firefox, and Safari
qualification is required before a browser compatibility or public speed
claim.

## Findings

The portable exact-scoring path meets the hard p95 ≤10 ms browser gate at 10K
and 25K but fails it at 50K on this reference machine. This result required
the explicit WASM SIMD128 tier documented above before 50K×384d could pass the
engineering gate. The separate 50K×192d profile already meets the latency gate
with the portable scorer.

A compiler-only `RUSTFLAGS='-C target-feature=+simd128'` experiment did not
accelerate the current scalar loop: at 25K×384d F32 it produced 8.67 / 9.13 ms
vector p50 / p95 and 8.91 / 9.19 ms hybrid p50 / p95, slightly slower than the
portable baseline. This motivated the explicit WASM SIMD128 scoring experiment
documented above; enabling the target feature alone was insufficient.

Initial ingestion is the blocking performance result. The current batch
binding calls the existing per-record upsert contract. Each new record scans
the accumulated chunk collection for an older generation of that record, so
the unique-record benchmark exhibits superlinear behavior. Optimizing this
must be additive: preserve the native per-record upsert API and behavior, then
introduce and benchmark a bulk construction path with identical validation,
identity, generation, filtering, BM25, and ranking semantics.

The next baseline must measure:

- the dedicated Worker transfer and browser binding cost;
- peak memory and WebAssembly memory growth;
- compact encodings and 768d vectors;
- filters, graph traversal, and graph-scoped retrieval;
- Chrome, Firefox, and Safari on named desktop and mobile devices.

No compatibility or public speed claim is authorized from this report.
