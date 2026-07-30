# Browser Embedding Production Implementation And Qualification — 2026-07-27

Status: implementation complete; the WebGPU production path is qualified on
the named Chrome reference environment. A later same-page desktop matrix also
qualified Firefox through the deterministic WASM fallback and real
CacheStorage in both browsers. The actual Chrome combined p95 was `12.405 ms`,
and the final uninstrumented 2026-07-28 rerun measured `12.460 ms`. The owner
accepted reference budgets of `15 ms` for WebGPU and `25 ms` for WASM
compatibility, with retrieval-only held at `8 ms`; Chrome and Firefox pass
their respective tiers. Safari, mobile-device, private-mode/cache-pressure,
and release-distribution qualification remain open. Safari 26.5.2 later passed
functional qualification and measured `18.380 ms` end-to-end p95. The owner
accepted a Safari-specific `20 ms` reference budget, so Safari passes and
optimization is deferred. No package was published. See
`browser-desktop-matrix-qualification-2026-07-27.md`.

## Decision and boundary

The browser provider is the independent
`wrappers/browser-embedding` package. It is not part of
`wrappers/browser`, the Node N-API packages, `retrievalkit-wasm`, or
`retrievalkit-core`. Retrieval APIs continue accepting caller-produced F32
vectors and continue using RetrievalKit's independent default
`I8ScalarQuantized` database encoding.

One dedicated module Worker owns:

1. verified model acquisition;
2. tokenization;
3. ONNX Runtime Web session creation and warmup;
4. FIFO batch-one or dynamic-batch inference;
5. one contiguous `Float32Array` result transfer.

The public surface is `BrowserEmbedder.load`, `prefetch`, `embed`,
`embedBatch`, immutable `modelInfo`, selected `provider`, and explicit close.
Cancellation, Worker failure, session failure, and operations after close use
typed errors.

## Frozen dependencies and artifacts

- `@huggingface/tokenizers`: exactly `0.1.3`
- `onnxruntime-web`: exactly `1.27.0`
- model: `sentence-transformers/all-MiniLM-L6-v2`
- source revision:
  `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
- artifact repository: `gungorbasa/retrievalkit-minilm`
- immutable artifact commit:
  `617ce926c1f9e0289365d3e999474cc28b1645d4`
- `manifest-v1.json`: 4,797 bytes, SHA-256
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`
- FP32 ONNX model: 90,396,663 bytes, SHA-256
  `beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891`

The cache also pins `tokenizer.json`, `tokenizer_config.json`,
`special_tokens_map.json`, and `vocab.txt` by exact size and SHA-256. URLs use
the immutable commit, never `main`.

The build verifies these ONNX Runtime Web assets before packaging:

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `ort-wasm-simd-threaded.mjs` | 24,180 | `0a1e718d99c41b22c21f2520ff4f9e883a6b5533856e398d21816ee8eb8185d3` |
| `ort-wasm-simd-threaded.wasm` | 13,479,978 | `d1ab1b94b16a65b29d710d0b587b29e7bed336827577623913479b8afe8113e6` |
| `ort-wasm-simd-threaded.asyncify.mjs` | 47,507 | `7236653b8565da4046e459cd0e274123419a1d9f1f8f18fd36c28058346ca655` |
| `ort-wasm-simd-threaded.asyncify.wasm` | 24,254,953 | `7e83cd6cee77e478bc96a7e91b198144fb5e4126287daf1f9b54bb195ebcd55a` |

The standard pair serves explicit WASM. The asyncify pair serves WebGPU/auto.
Dependency licenses and ONNX Runtime third-party notices are included.

## Acquisition and cache behavior

Acquisition is allowed only during `load` or explicit `prefetch`.
`localOnly` runs no artifact fetch and requires a completely verified cache.
Every response and cache hit is rechecked for exact size and SHA-256. The
completion marker is written last. Interrupted downloads, failed publication,
partial state, wrong sizes, wrong hashes, and corrupt cache hits are cleaned.
Module callers share one network acquisition; Web Locks coordinate separate
Workers where supported. Cancelling one waiter does not cancel acquisition
needed by another waiter.

An actual cold request through the package's default HTTPS fetcher downloaded
and verified all six files from the immutable public commit in
`10,547.053 ms`; the verified model contained exactly `90,396,663` bytes.
This proves the pinned public URL. It did not publish or modify any artifact.

The Worker/package/runtime assets are application assets, not model-cache
entries. A fully offline application must serve or service-worker-precache
those assets separately.

## Environment and method

- Hardware: Apple M1 Max, 10 cores, 32 GB RAM.
- Operating system: macOS 26.5.2 (25F84), arm64.
- Rust: 1.92.0.
- Node: 25.9.0 for the local qualification harness.
- Browser: Chrome 150.0.7871.126, headless, loopback origin.
- Browser runtime: a real dedicated module Worker with `navigator.gpu`.
- Build mode: TypeScript production build; Cargo release for the retrieval
  performance and I8 policy runs.
- Embedding benchmark: exact tokenizer-verified 32-token input, batch one,
  50 warm-ups, 750 measurements, monotonic browser/runtime time.
- Retrieval benchmark: 50K chunks, 384 dimensions, cosine,
  `I8ScalarQuantized`, top 10, SIMD128 tier, 50 warm-ups, 750 measurements.

The Chrome harness serves only built package files and the frozen local
artifact tree over loopback. Hostname resolution outside loopback is blocked.
It forces `execution: "webgpu"` and fails if the provider falls back. The WASM
harness forces the provider-specific WASM entrypoint.

## FP32 output and ranking conformance

The WASM provider produced all 94 frozen vectors through the actual
ONNX Runtime Web boundary:

| Gate | Result |
|---|---:|
| vectors | 94 / 94 valid |
| dimension | exactly 384 |
| dtype origin | F32 |
| finite and unit-normalized | pass |
| median cosine versus Rust FP32 | `0.9999999999996866` |
| minimum cosine | `0.9999999999991718` |
| mean Top-10 overlap | `100%` |
| exact Top-10 sets | `100%` |
| minimum per-query Top-10 overlap | `100%` |

The fixture includes Unicode, empty-input rejection, and a long truncation
diagnostic. Direct tokenizer tests prove 254 content tokens plus `[CLS]` and a
terminal `[SEP]`, with batch-longest right padding and a maximum length of
256.

## Actual RetrievalKit I8 paths

Browser-produced vectors and the frozen Rust FP32 reference were run in both
database/query directions through RetrievalKit's actual
`GraphRetrievalDatabase`. Both directions produced the same results:

| Path | Mean Top-10 | Exact Top-10 sets | Minimum |
|---|---:|---:|---:|
| vector | `99.7619%` | `97.6190%` | `90%` |
| hybrid | `100%` | `100%` | `100%` |
| graph-scoped vector | `100%` | `100%` | `100%` |
| graph-scoped hybrid | `99.2857%` | `92.8571%` | `90%` |

BM25, graph-scoped BM25, and graph-only selection were exactly identical.
The existing persistence regression
`persisted_i8_vectors_contain_one_byte_per_dimension_and_one_scale_per_row`
passed, proving that persisted I8 storage contains no duplicate F32 vector
payload. The shared Rust example predates this wrapper and names its positional
second-provider JSON fields `coreml`; for this invocation that second provider
is the browser vector file.

## Performance

### Embedding

| Boundary | Chrome WebGPU | WASM fallback |
|---|---:|---:|
| cached initialization | 756.400 ms | 411.042 ms |
| first inference | 25.300 ms | 22.201 ms |
| warm p50 | 7.100 ms | 19.226 ms |
| warm p95 | 7.500 ms | 19.804 ms |
| warm p99 | 11.100 ms | 20.356 ms |
| warm maximum | 22.200 ms | 40.480 ms |

Chrome selected `webgpu` in a real Worker and returned a 384-value finite unit
vector. The WASM provider passed correctness but is a compatibility fallback,
not a sub-10-ms performance tier.

### I8 retrieval

| Boundary | p50 | p95 |
|---|---:|---:|
| vector | 1.686 ms | 1.887 ms |
| BM25 | 0.151 ms | 0.182 ms |
| hybrid | 1.927 ms | 2.250 ms |

Module load was 4.557 ms, 50K ingestion was 62.087 s, and final build was
0.334 ms. The ingestion limitation remains tracked by the browser retrieval
plan and is not hidden in query latency.

The separately measured p95 sums are 9.387 ms for WebGPU embedding plus vector
retrieval and 9.750 ms for WebGPU embedding plus hybrid retrieval.
Retrieval-only stays below 8 ms. These are sums of independently measured
provider and Node/WASM retrieval boundaries. The later production same-page
run measured `12.405 ms` p95, so these sums must not be presented as an
end-to-end gate pass.

## Package and regression results

- Browser embedding: strict typecheck, ESLint, 19 offline tests, production
  build, package validation, and `npm audit --omit=dev` passed.
- Qualification harnesses: 12 Node tests passed.
- Dry-run package: 64 files, 9,385,943 bytes compressed and 38,262,039 bytes
  unpacked.
- Browser retrieval package: 12 tests, typecheck, lint, build, legal-file
  identity, and dry-run pack passed.
- Generated WASM: portable and SIMD128 runtime smokes and direct result
  conformance passed.
- Rust: 139 core unit tests plus integration tests, 11 embedding tests, all
  graph tests, and 8 WASM tests passed. The one public-download Rust test
  remains intentionally ignored.
- Relevant Rust Clippy, workspace check, formatting, README claim validation,
  release metadata validation, and release-validator unit tests passed.
- CI YAML parsed successfully.
- Production dependency audit reported zero known vulnerabilities.

The pre-change 652-file checksum inventory changed only for the intentionally
edited browser benchmark, browser README, and active TypeScript/browser product
guidance. No existing Swift, Python, Node, Kotlin/JVM, Android, browser
retrieval source, or package manifest changed. `Cargo.toml`, `Cargo.lock`, and
the generated root third-party notice state did not change in this slice.

## Reproduction commands

```sh
cd "/Users/gungorbasa/.codex/worktrees/34d6/Vector Search"

cd wrappers/browser-embedding
npm run check
npm run check:package
npm audit --omit=dev

cd ../..
node --test \
  scripts/embedding/test_qualify_browser_embedding.mjs \
  scripts/embedding/test_qualify_browser_embedding_webgpu.mjs

node --input-type=module -e 'import {performance} from "node:perf_hooks"; import {acquireArtifacts,defaultArtifactFetcher} from "./wrappers/browser-embedding/dist/acquire.js"; import {PINNED_ARTIFACTS} from "./wrappers/browser-embedding/dist/constants.js"; import {MemoryArtifactStore} from "./wrappers/browser-embedding/dist/store.js"; const start=performance.now(); const acquired=await acquireArtifacts({artifacts:PINNED_ARTIFACTS,store:new MemoryArtifactStore("public-cold-download"),fetcher:defaultArtifactFetcher,localOnly:false}); const model=await acquired.read("onnx/all-MiniLM-L6-v2-fp32.onnx"); console.log(JSON.stringify({files:PINNED_ARTIFACTS.length,model_bytes:model.byteLength,cold_download_verify_ms:performance.now()-start}));'

node scripts/embedding/qualify-browser-embedding.mjs \
  --input target/python-node-embedding-qualification/input.json \
  --artifacts target/python-node-embedding-cold-cache/sentence-transformers_all-MiniLM-L6-v2/c9745ed1d9f207416be6d2e6f8de32d1f16199bf \
  --output target/browser-embedding-qualification/browser-output.json \
  --benchmark-output target/browser-embedding-qualification/browser-benchmark.json

python3 scripts/embedding/validate-python-node-wrapper-conformance.py \
  --input target/python-node-embedding-qualification/input.json \
  --reference "/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/rust-cpu-fp32.json" \
  --candidate browser=target/browser-embedding-qualification/browser-output.json \
  --output target/browser-embedding-qualification/conformance-report.json

node -e 'const fs=require("fs"); const input=JSON.parse(fs.readFileSync("target/python-node-embedding-qualification/input.json")); const browser=JSON.parse(fs.readFileSync("target/browser-embedding-qualification/browser-output.json")); fs.writeFileSync("target/browser-embedding-qualification/texts.json",JSON.stringify(input.items.map(x=>x.text))); fs.writeFileSync("target/browser-embedding-qualification/browser-vectors.json",JSON.stringify(browser.items.map(x=>x.embedding)));'

cargo run --locked --release \
  -p retrievalkit-embedding \
  --example qualify_retrieval_policy -- \
  target/browser-embedding-qualification/texts.json \
  "/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/rust-cpu-fp32.json" \
  target/browser-embedding-qualification/browser-vectors.json

node scripts/embedding/qualify-browser-embedding-webgpu.mjs \
  --artifacts target/python-node-embedding-cold-cache/sentence-transformers_all-MiniLM-L6-v2/c9745ed1d9f207416be6d2e6f8de32d1f16199bf \
  --output target/browser-embedding-qualification/chromium-webgpu-benchmark.json

scripts/benchmark-browser-wasm.sh 50000 384 750 i8 simd128 50
scripts/check-browser-wasm.sh
cargo test --locked \
  -p retrievalkit-core \
  -p retrievalkit-graph \
  -p retrievalkit-embedding \
  -p retrievalkit-wasm
```

## Evidence hashes

| Evidence | SHA-256 |
|---|---|
| frozen conformance input | `89eb32325523dd25dc35a9c0b6588dc0bf5da25d0f67b2a8c2b8da625b1d6a27` |
| frozen Rust FP32 reference | `bcbb4c124e1bee90d425d07c9d5ec6ed71abc80bb94bcd5f73f62d621a9a9391` |
| browser candidate vectors | `53ef62525efd22aac6bcb9355993f6506b0b473156ac1bf6fc542bbc05a4ae16` |
| conformance report | `adeda5af89934046947a4260db6f970f2dd7ad517ed53102f0d921f5a1a74b2f` |
| WASM benchmark | `85723ef1665abb7c13ebb7fb2d0009f5fcc8688721e27a24a0355f0c4a8f3513` |
| Chrome WebGPU benchmark | `90c84565d59659e4ac62427c6440651be437b528a925651d1dca838b094d5dc0` |
| RetrievalKit I8 policy | `7eb3cf309cd6b2e3fd08d8a28da4cae74f4478f68422146d4c4ec3ae32de3bfc` |
| 50/750 I8 retrieval benchmark | `4dbc7ecda5772b3d0bc6ff14f3f94fb81693f6eba0c6d1f0c4de11e0a548393e` |

## Remaining risks and release boundary

- Chrome is the qualified WebGPU tier. Firefox is qualified through the WASM
  compatibility tier because this host exposes no usable Firefox GPU adapter.
  Safari, versions beyond the named runs, and mobile GPUs remain open.
- The later desktop matrix uses real CacheStorage and proves exact concurrent
  acquisition, cached-only load, and corruption recovery in Chrome and
  Firefox. Private-browsing behavior and naturally occurring quota/eviction
  behavior still require target-browser testing.
- The 90.4 MB FP32 model plus the 38.3 MB unpacked runtime package is material
  for browser download, cache quota, and peak memory. Those costs must be
  disclosed before release.
- WASM fallback warm p95 is about 19.8 ms; the actual Firefox combined p95 is
  `21.660 ms`, which passes the accepted `25 ms` compatibility-tier budget.
- The browser retrieval ingestion path remains too slow at 50K and is a
  separate release blocker from query latency.

No npm package, RetrievalKit release, tag, website, model artifact, or SDK was
published or modified.
