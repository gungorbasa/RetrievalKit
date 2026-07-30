# Browser Desktop Matrix Qualification — 2026-07-27

Status: Chrome and Firefox desktop production boundaries passed correctness,
CacheStorage, lifecycle, and 50K retrieval qualification. Chrome selected
WebGPU and Firefox selected the deterministic WASM fallback. Safari execution
was blocked by the host's disabled Apple WebDriver Remote Automation setting.
The real Chrome same-page embedding-plus-retrieval p95 was `12.405 ms`, so the
combined sub-10-ms engineering gate did not pass. No package, website, tag, or
release was published.

## Scope and architecture

The qualification used the independent production packages:

- `wrappers/browser-embedding` for verified FP32 MiniLM embedding in one
  dedicated module Worker;
- `wrappers/browser` plus the generated `retrievalkit-wasm` SIMD128 artifact
  for I8 retrieval in a second dedicated module Worker.

The page composed the two public asynchronous clients. It did not run
embedding or retrieval on the UI thread, couple the packages, or add model
execution to `retrievalkit-core`.

The matrix harness is
`scripts/embedding/qualify-browser-desktop-matrix.mjs`. It drives installed
Chrome with the DevTools protocol, Firefox with WebDriver/geckodriver, and
Safari with the matching system `safaridriver`. It serves only loopback package
builds, generated WASM, and the already verified frozen model tree.

## Environment

- Hardware: Apple M1 Max, 10 cores, 32 GB RAM.
- OS: macOS 26.5.2 (25F84), arm64.
- Chrome: 150.0.7871.126.
- Firefox: 150.0.1.
- Safari/safaridriver: 26.5.2.
- geckodriver: 0.37.1.
- Rust: 1.92.0.
- Node: 25.9.0.
- Build: Cargo release and TypeScript production builds.
- Retrieval corpus: 50,000 chunks, 384 dimensions, cosine,
  `I8ScalarQuantized`, top 10.
- Benchmark input: tokenizer-verified 32 BERT tokens.
- Method: 50 warm-ups and 750 measured batch-one queries.

## Provider selection correction

Firefox exposes `navigator.gpu` on this host even though
`navigator.gpu.requestAdapter()` returns `null` because WebGPU is blocklisted.
Checking only for the property allowed an `auto` session with a WebGPU/WASM
provider list to be labeled `webgpu` even when no GPU adapter existed.

Production provider selection now requires an actual adapter before attempting
the WebGPU tier. Chrome therefore reports `webgpu`; Firefox honestly reports
`wasm`. Explicit WebGPU remains strict and the normal automatic fallback
remains deterministic.

## Real CacheStorage and correctness

Chrome and Firefox both passed:

- missing-cache `localOnly` rejection with no network;
- two concurrent cold prefetch callers with exactly six artifact requests;
- interrupted acquisition rejection with zero partial cache residue;
- seven-entry atomic cache publication with the completion marker last;
- cached `localOnly` load;
- corrupt model detection, full generation cleanup, and verified recovery;
- Unicode input;
- exact 256-token truncation behavior;
- empty-input rejection;
- deterministic operations-after-close failure;
- exactly 384 finite L2-normalized F32 values;
- actual signed-I8 RetrievalKit Top-10 search;
- dedicated module Worker ownership for both package boundaries.

A separate real Chrome `globalThis.caches` fixture additionally passed
simulated eviction with zero invalid generation residue. Deterministic
injected `QuotaExceededError` coverage proved typed `RK_EMBEDDING_CACHE`
propagation, cleanup, and recovery because safely exhausting the browser's
actual disk quota is not deterministic.

## 50K performance

| Boundary | Chrome WebGPU + SIMD128 | Firefox WASM + SIMD128 |
|---|---:|---:|
| Cached initialization | 696.160 ms | 626.640 ms |
| First inference | 22.770 ms | 21.520 ms |
| 50K ingestion | 57,485.055 ms | 59,593.520 ms |
| Warm embedding p50 | 8.715 ms | 19.040 ms |
| Warm embedding p95 | 10.560 ms | 20.120 ms |
| Retrieval p50 | 1.805 ms | 1.480 ms |
| Retrieval p95 | 1.905 ms | 1.580 ms |
| Same-page end-to-end p50 | 10.520 ms | 20.540 ms |
| Same-page end-to-end p95 | 12.405 ms | 21.660 ms |

Retrieval-only remains comfortably below the 8 ms engineering gate in both
browsers. The Chrome same-page p95 does not meet the 10 ms combined gate. This
supersedes the earlier `9.387 ms` and `9.750 ms` estimates that summed
independently measured embedding and retrieval boundaries. Those estimates
remain valid descriptions of their separate runs but are not production
end-to-end measurements.

Firefox WASM is a correctness and compatibility tier, not a low-latency
embedding tier. The 50K ingestion cost also remains material and separate from
query latency.

## Safari boundary

The matching system `safaridriver` started successfully, but Safari refused
session creation with:

```text
You must enable 'Allow remote automation' in the Developer section of
Safari Settings to control Safari via WebDriver.
```

The repository did not change that owner-controlled OS setting. Safari
qualification remains open until the owner enables Safari → Develop →
Developer Settings → Allow Remote Automation. Safari has no headless mode, so
the eventual run will open a visible browser window.

## Commands

```sh
cd "/Users/gungorbasa/.codex/worktrees/34d6/Vector Search"

cd wrappers/browser-embedding
npm run check
cd ../browser
npm run check
npm run build
cd ../..

cargo build --locked --release --target wasm32-unknown-unknown \
  -p retrievalkit-wasm
wasm-bindgen \
  target/wasm32-unknown-unknown/release/retrievalkit_wasm.wasm \
  --target web --typescript \
  --out-dir target/browser-desktop-qualification/portable

CARGO_TARGET_DIR=target/browser-desktop-qualification/simd-target \
  cargo build --locked --release --target wasm32-unknown-unknown \
  -p retrievalkit-wasm --features wasm-simd128
wasm-bindgen \
  target/browser-desktop-qualification/simd-target/wasm32-unknown-unknown/release/retrievalkit_wasm.wasm \
  --target web --typescript \
  --out-dir target/browser-desktop-qualification/simd128

node scripts/embedding/qualify-browser-desktop-matrix.mjs \
  --artifacts target/python-node-embedding-cold-cache/sentence-transformers_all-MiniLM-L6-v2/c9745ed1d9f207416be6d2e6f8de32d1f16199bf \
  --output target/browser-desktop-matrix-50k.json \
  --browsers chrome,firefox,safari \
  --execution auto \
  --chunks 50000 \
  --timeout-ms 1800000

node --test \
  scripts/embedding/test_qualify_browser_embedding.mjs \
  scripts/embedding/test_qualify_browser_embedding_webgpu.mjs \
  scripts/embedding/test_qualify_browser_desktop_matrix.mjs
```

## Evidence and remaining gates

The final matrix JSON SHA-256 is
`ffb80633fe00239b42c45428ef2829f4f379e3ecd164a8ea2448d854be47a38e`.

Remaining gates:

- enable and run the installed Safari production matrix;
- run mobile-browser/device qualification;
- characterize private-browsing cache behavior and naturally occurring
  quota/eviction behavior on target browsers;
- address or explicitly accept the material 50K ingestion, model download,
  cache, and peak-memory costs.

## 2026-07-28 performance-policy addendum

The original report correctly records that Chrome did not pass the former
sub-10-ms combined gate. After the dated hot-path investigation, the owner
accepted provider-tiered budgets on this fixed reference contract: WebGPU
embedding plus SIMD128 retrieval p95 `<=15 ms`, WASM compatibility embedding
plus SIMD128 retrieval p95 `<=25 ms`, and retrieval-only p95 `<=8 ms`. The
final uninstrumented Chrome rerun passed at `12.460 ms`; Firefox passes the
compatibility tier at `21.660 ms`. Performance is no longer an open desktop
release gate for these qualified providers. A new `--require-all` Safari 50K
attempt on 2026-07-28 again failed before session creation because Safari's
Developer setting **Allow remote automation** remains disabled. No benchmark
sample was produced by that attempt.

After the owner enabled WebDriver, the unchanged Safari matrix completed.
Safari selected WebGPU and SIMD128, passed every correctness/cache gate, and
measured embedding/retrieval/end-to-end p95
`16.520/1.940/18.380 ms`. The owner accepted a Safari-specific `20 ms`
reference budget on 2026-07-28, so Safari passes and optimization is deferred.
The general Chrome WebGPU `15 ms` budget remains unchanged. Evidence SHA-256:
`80adf52555758ff168e2a39411cedff16c0b4bba15339417cc8279c72f68bec3`.
