# Browser WebGPU Hot-Path Investigation — 2026-07-28

Status: the browser embedding boundary was profiled through the real
two-Worker Chrome path. Two redundant post-inference `Float32Array` copies were
removed without changing validation or public behavior. The final production
50K run passed correctness with end-to-end p95 `12.460 ms`. On 2026-07-28 the
owner accepted provider-tiered reference budgets of `15 ms` for WebGPU,
`25 ms` for WASM compatibility, and `8 ms` for retrieval-only, so this Chrome
result passes the accelerated tier. No package, tag, website, model artifact,
or RetrievalKit release was published.

## Environment and contract

- Hardware: Apple M1 Max, 10 cores, 32 GB RAM.
- OS: macOS 26.5.2 (25F84), arm64.
- Chrome: 150.0.7871.126, headless production package execution.
- Node: 25.9.0.
- Rust: 1.92.0.
- Builds: TypeScript production builds and existing Cargo release WASM.
- Corpus: 50,000 chunks, 384 dimensions, cosine, signed-I8 scalar
  quantization, Top 10.
- Query: tokenizer-verified 32 BERT tokens.
- Samples: 50 warm-ups and 750 measured batch-one queries.
- Architecture: FP32 ONNX Runtime Web embedding and SIMD128 RetrievalKit
  retrieval in separate dedicated module Workers.

## Phase profile

A temporary qualification-only wrapper measured time inside
`OnnxEmbeddingRuntime.embed` separately from the public Worker/client boundary.
The wrapper posted one extra diagnostic message per inference and was removed
after profiling, so these runs diagnose the boundary but are not the final
performance evidence.

| Experiment | Runtime p95 | Public embedding p95 | Retrieval p95 | End-to-end p95 |
|---|---:|---:|---:|---:|
| 256 chunks | 7.005 ms | 7.090 ms | 0.175 ms | 7.255 ms |
| 50K baseline | 10.515 ms | 10.685 ms | 2.180 ms | 12.590 ms |
| 50K, 10-second post-build settle | 10.350 ms | 10.480 ms | 1.985 ms | 12.390 ms |
| 50K, 1,000-document transfer batches | 10.280 ms | 10.430 ms | 1.930 ms | 12.300 ms |
| 50K, cached embedder recreation after build | 9.975 ms | 10.100 ms | 1.975 ms | 11.955 ms |
| 50K, embedder first loaded after build | 10.155 ms | 10.285 ms | 1.980 ms | 12.145 ms |

The 256-chunk result shows only `0.085 ms` between the runtime and public
embedding p95. At 50K, the corresponding delta remained small while the
runtime phase itself increased by about `3.5 ms`. The missing budget therefore
is not in Worker RPC, client validation, or retrieval wrapper overhead.

Additional experiments ruled out a single large transferable buffer, retained
caller-side F32 source vectors, a short stabilization interval, WebGPU/WASM
provider-list partitioning, session age, embedder/database load order, and
background-target foregrounding. Results varied under repeated sustained
50K construction, but none passed the combined gate. The evidence localizes
the regression to WebGPU execution under the sustained 50K browser workload;
it does not establish one safe production fix.

## Production change and final result

The runtime must copy ONNX output before disposing the result tensor. It now
validates that owned buffer in place. The Worker service and single-embedding
client also validate in place instead of allocating two more 384-value arrays.
Batch results still copy rows so callers receive independent vectors. Exactly
384 finite, L2-normalized F32 values remain mandatory at every boundary.

The final run used the original uninstrumented production harness and changed
no corpus, precision, provider, token, sample, ranking, or Worker semantics:

| Boundary | Chrome WebGPU + SIMD128 |
|---|---:|
| Cached initialization | 909.880 ms |
| First inference | 22.980 ms |
| 50K ingestion | 63,630.295 ms |
| Warm embedding p50 / p95 | 8.670 / 10.610 ms |
| Retrieval p50 / p95 | 1.835 / 1.995 ms |
| End-to-end p50 / p95 | 10.505 / 12.460 ms |

All CacheStorage, verified acquisition, Unicode, truncation, lifecycle,
finite/normalized output, dedicated-Worker, and deterministic actual-I8 Top-10
checks passed. The JSON evidence is
`target/browser-desktop-matrix-50k-copy-cleanup.json`, SHA-256
`29ffa34a970e629170b2008f654a9d89c4ac5c94c9de4c78e372b6b6817aa1be`.

## Commands

```sh
cd "/Users/gungorbasa/.codex/worktrees/34d6/Vector Search/wrappers/browser-embedding"
npm run check
npm run check:package

cd "../browser"
npm run check
npm run build

cd "../.."
node --test \
  scripts/embedding/test_qualify_browser_embedding.mjs \
  scripts/embedding/test_qualify_browser_embedding_webgpu.mjs \
  scripts/embedding/test_qualify_browser_desktop_matrix.mjs

node scripts/embedding/qualify-browser-desktop-matrix.mjs \
  --artifacts target/python-node-embedding-cold-cache/sentence-transformers_all-MiniLM-L6-v2/c9745ed1d9f207416be6d2e6f8de32d1f16199bf \
  --output target/browser-desktop-matrix-50k-copy-cleanup.json \
  --browsers chrome \
  --execution auto \
  --chunks 50000 \
  --timeout-ms 1800000 \
  --require-all
```

## Accepted performance policy and remaining work

- On the named Apple M1 Max 50K/32-token/50/750 reference contract, WebGPU
  embedding plus SIMD128 retrieval must have p95 `<=15 ms`, WASM compatibility
  embedding plus SIMD128 retrieval must have p95 `<=25 ms`, and retrieval-only
  p95 must remain `<=8 ms`.
- Do not claim a universal sub-10-ms browser result or present the tiered
  reference budgets as device-wide SLAs.
- Chrome/Metal GPU scheduling and memory-pressure tracing is optional future
  optimization work.
- Re-evaluate ONNX Runtime Web releases only against the frozen conformance
  corpus and the same production 50K gate.
- Safari measured `18.380 ms` end-to-end p95 and passes the later
  owner-approved Safari-specific `20 ms` reference budget. Safari optimization
  is deferred. Mobile browsers, private browsing, natural cache pressure,
  material ingestion cost, and browser package publication remain separate
  open gates.
