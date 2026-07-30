# Python and Node Embedding Production Implementation and Qualification — 2026-07-27

## Decision

RetrievalKit now has production-quality optional local embedding packages for
Python and Node.js in the same source repository as the SDK, while retaining
separate distributable artifacts:

| Language | Public package | Source | Native aggregate |
| --- | --- | --- | --- |
| Python | `retrievalkit-embedding` | `wrappers/python-embedding` | `retrievalkit-python-embedding` |
| Node.js | `@gungorbasa/retrievalkit-embedding` | `wrappers/typescript/embedding` | `retrievalkit-node-embedding` |

Both native aggregates bind the existing optional
`crates/retrievalkit-embedding` ONNX provider. Neither depends on
`retrievalkit-core`, `retrievalkit-graph`, or a retrieval wrapper. They may be
installed beside a retrieval package, but embedding and retrieval remain
separate application-owned components.

The packages are provisional and unpublished. This work did not create a
RetrievalKit release, registry publication, Git tag, SDK upload, or browser
embedding provider.

## Production contract

Both wrappers expose only the canonical FP32 profile:

- model: `sentence-transformers/all-MiniLM-L6-v2`
- source revision:
  `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
- artifact repository: `gungorbasa/retrievalkit-minilm`
- immutable artifact commit:
  `617ce926c1f9e0289365d3e999474cc28b1645d4`
- artifact manifest SHA-256:
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`
- maximum input: fixed 256 WordPiece tokens
- output: exactly 384 finite F32 values with unit L2 norm
- runtime: official ONNX Runtime 1.24.3

Python provides synchronous `OnnxEmbedder.load`, `prefetch`, `embed`, and
`embed_batch`. Cache/model loading and inference release the GIL. Node provides
promise-based `OnnxEmbedder.load`, `prefetch`, `embed`, and `embedBatch`;
native work uses N-API worker tasks, and the public object supports `close`,
`Symbol.dispose`, and `Symbol.asyncDispose`.

FP32 model precision is independent from RetrievalKit database-vector
encoding. RetrievalKit continues to accept F32 input and use
`I8ScalarQuantized` storage by default. The new packages do not change search,
ranking, persistence, graph behavior, or database formats, and no database
migration or duplicate stored F32 vector payload is introduced.

## Acquisition and runtime boundary

Model acquisition is possible only during embedder construction/loading or
explicit prefetch. `local_only` in Python and `localOnly` in Node prohibit
network access. Both wrappers use the shared Rust provider's:

- immutable HTTPS URLs rather than a mutable branch;
- OS-default or caller-selected cache;
- exact artifact size and SHA-256 verification;
- cross-process exclusive acquisition lock;
- temporary files plus file and parent-directory synchronization;
- atomic rename publication;
- corrupt and interrupted-download cleanup.

A clean Python prefetch into
`target/python-node-embedding-cold-cache` completed in `9.016 s`. A Python
local-only load then produced a 384-value normalized Unicode embedding. Node
subsequently loaded and used the exact same cache with `localOnly` in
`378.212 ms`, proving that the language wrappers share the artifact identity
and cache layout without another download.

The clean cache contained the following verified FP32 payload:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `manifest-v1.json` | 4,797 | `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2` |
| `onnx/all-MiniLM-L6-v2-fp32.onnx` | 90,396,663 | `beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891` |
| `tokenizer/tokenizer.json` | 466,247 | `be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037` |
| `tokenizer/tokenizer_config.json` | 350 | `acb92769e8195aabd29b7b2137a9e6d6e25c476a4f15aa4355c233426c61576b` |
| `tokenizer/special_tokens_map.json` | 112 | `303df45a03609e4ead04bc3dc1536d0ab19b5358db685b6f3da123d05ec200e3` |
| `tokenizer/vocab.txt` | 231,508 | `07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3` |

Application-managed runtime paths remain supported. A distributable macOS
arm64 wheel or npm package may instead contain the qualified package-local
runtime, but its build and load boundaries verify:

```text
filename: libonnxruntime.1.24.3.dylib
bytes: 27,724,968
SHA-256: b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729
```

Package validation requires the ONNX Runtime license and
`ThirdPartyNotices.txt` beside that binary. The generated Python release wheel
was `10,635,304` bytes and passed a clean installed-wheel smoke test. Node
package-content and clean local-install tests passed with the same runtime and
legal-file requirements. Generated native libraries, runtimes, wheels, npm
archives, caches, and virtual environments remain ignored; no runtime binary is
tracked in source.

## Conformance

The frozen role-aware fixture contains 48 corpus texts, 42 queries, and four
diagnostics. The shared validator verifies exact metadata, ordered IDs, exactly
384 finite Float32-origin values, unit norm within `1e-4`, cosine agreement,
and role-aware Top-10 ranking. The reference is the frozen Rust ONNX CPU FP32
output.

| Candidate | Median cosine | Minimum cosine | Mean Top-10 | Exact Top-10 sets | Minimum query overlap |
| --- | ---: | ---: | ---: | ---: | ---: |
| Python | 1.0 | 0.9999999999998386 | 100% | 100% | 100% |
| Node | 1.0 | 0.9999999999998386 | 100% | 100% | 100% |

Both candidates pass the required median cosine `>= 0.9999`, mean Top-10
overlap `>= 99%`, exact Top-10-set fraction `>= 90%`, and per-query minimum
overlap `>= 90%`. Python and Node output vectors are exactly equal to each
other after JSON decoding.

Qualification evidence:

| Evidence | SHA-256 |
| --- | --- |
| input fixture | `89eb32325523dd25dc35a9c0b6588dc0bf5da25d0f67b2a8c2b8da625b1d6a27` |
| frozen Rust reference | `bcbb4c124e1bee90d425d07c9d5ec6ed71abc80bb94bcd5f73f62d621a9a9391` |
| Python output | `035b6f083d36918ffd71ff23ff297d81517d81953eb8375b262407aa2734a1e8` |
| Node output | `55743533ba01bf302408ddb656627c822d5fb132e7b813aa6aca1c0926213fcd` |
| shared conformance report | `28c5162e3427e929e768ed2fbb992ca613d9193cb5fbd07ec548387308011471` |

Because both wrappers return the same Rust-provider F32 values and do not
alter retrieval, the already-frozen actual RetrievalKit I8 qualification
continues to apply without a new database format or scoring path. Its vector,
hybrid, graph-scoped vector, and graph-scoped hybrid rows passed in both
provider directions; BM25 and graph-only results were exactly identical; and
persisted I8 files contained only one signed byte per dimension plus one F32
scale per row, with no duplicate F32 vector payload. See
`fp32-embedding-i8-database-qualification-2026-07-27.md`.

## Release benchmark

The benchmark ran on:

```text
host: Apple M1 Max, 32 GiB, arm64
OS: macOS 26.5.2 (25F84)
Rust: rustc/cargo 1.92.0
Python: CPython 3.14.4
Node: 24.18.0
build mode: release
query: 32 tokens
warm-ups: 50
measured embeddings: 750
```

| Boundary | Python | Node |
| --- | ---: | ---: |
| cached initialization | 365.349 ms | 363.429 ms |
| first inference | 6.265 ms | 4.584 ms |
| warm embedding p50 | 5.964 ms | 6.004 ms |
| warm embedding p95 | 6.222 ms | 6.207 ms |
| warm embedding p99 | 6.827 ms | 6.583 ms |

The Python and Node benchmark JSON files have SHA-256
`8ffde43d10824d4ae94c59787a93647653f8ad44c14d4f8025ea8c8090e896b2`
and
`94e9c1da8a92c199c0dee0f04a42c1daaec96bfea37f45bed6def863954e38ad`.
Both warm embedding p95 values remain below 10 ms before adding the frozen
10K I8 retrieval p95 of `0.218 ms`; the combined measured boundaries remain
below 10 ms. Retrieval-only remains below the 8 ms gate through the frozen 50K
diagnostic. The public retrieval API does not expose query quantization as a
separate timer, so its retrieval measurement includes query validation,
quantization, and scoring.

## Commands and regression results

Representative qualification commands:

```sh
scripts/check-python-embedding-wrapper.sh

PATH=target/toolchains/node24/node_modules/node/bin:$PATH \
RETRIEVALKIT_BUNDLE_ONNX_RUNTIME=1 \
RETRIEVALKIT_REQUIRE_BUNDLED_ONNX_RUNTIME=1 \
npm --prefix wrappers/typescript run typecheck

python3 scripts/embedding/generate-python-node-wrapper-conformance-input.py

python3 scripts/embedding/validate-python-node-wrapper-conformance.py \
  --input target/python-node-embedding-qualification/input.json \
  --reference target/embedding-provider-vectors/rust-cpu-fp32.json \
  --candidate python=target/python-node-embedding-qualification/python.json \
  --candidate node=target/python-node-embedding-qualification/node.json \
  --output target/python-node-embedding-qualification/report.json

cargo fmt --all -- --check
cargo clippy --locked \
  -p retrievalkit-embedding \
  -p retrievalkit-python-embedding \
  -p retrievalkit-node-embedding \
  --all-targets -- -D warnings
cargo test --locked \
  -p retrievalkit-embedding \
  -p retrievalkit-python-embedding \
  -p retrievalkit-node-embedding
cargo test --locked \
  -p retrievalkit-core -p retrievalkit-graph -p retrievalkit-wasm
```

Results:

- shared Rust embedding provider: 11 passed; one explicit public-download test
  ignored by the offline default;
- Node native embedding aggregate: 2 passed;
- Python embedding: Ruff and strict mypy passed; 3 offline tests passed and one
  explicit live test skipped in the offline run; release wheel build, runtime
  contents, and installed-wheel smoke passed;
- Node workspace: typecheck and lint passed; base 6 tests, embedding 4 passed
  with 2 explicit live tests skipped, graph 7 tests, and 4 preflight tests
  passed; package-content and clean-install smoke passed;
- Rust core: 150 unit/integration tests passed;
- Rust graph: 33 unit/integration tests passed;
- Rust Browser/WASM aggregate: 8 tests passed;
- Python base: Ruff, strict mypy, and 27 tests passed;
- Python graph: Ruff, strict mypy, and 8 tests passed;
- wrapper `LICENSE` and `NOTICE` files are byte-identical to the repository
  copies;
- Cargo dependency-tree inspection found no retrieval core or graph dependency
  in either new embedding aggregate.

Dependency notices were regenerated after the workspace dependency change.
Static release metadata validation and publication-claim tests pass after the
new optional packages are included. No publication command was run.

## Remaining risks

- Executed package qualification is macOS arm64 on Python 3.14 and Node 24.
  The declared Python 3.10–3.14 and maintained Node LTS ranges still require
  their normal CI matrices before any registry release.
- The qualified runtime adds 27.7 MB uncompressed and the FP32 model cache adds
  about 91 MB. Package and model size should remain visible in future release
  notes.
- Python intentionally exposes a synchronous API while releasing the GIL.
  Applications requiring an async surface should schedule it with their normal
  executor rather than receiving a second wrapper-specific concurrency model.
- Browser embedding is a distinct future package and must not reuse the Node
  N-API addon. Kotlin/Android embedding also remains a separate future slice.
- Registry names remain subject to the repository's documented naming and
  trademark clearance. Nothing here authorizes publication.
