# Swift Core ML Production Implementation and Qualification — 2026-07-27

## Outcome

Swift production embedding now uses direct Core ML through the provider-neutral
`EmbeddingKit` package. The canonical profile is FP32
`all-MiniLM-L6-v2`, fixed at 256 input tokens, with Core ML compute units
`.all`. Every production result is required to contain exactly 384 finite F32
values and is returned with unit L2 norm.

This does not change RetrievalKit's public vector boundary or persistence
format. Applications continue passing F32 vectors to RetrievalKit, and the
database continues using its existing `I8ScalarQuantized` default. No embedding,
Core ML, download, HTTP, or ONNX Runtime dependency was added to
`retrievalkit-core`.

The experimental Swift `EmbeddingKitONNX` package, its C bridge, and its Apple
ONNX Runtime XCFramework builder/settings were removed after the direct Core ML
replacement passed. The optional Rust `retrievalkit-embedding` ONNX provider
remains the cross-platform/non-Apple provider. Historical Swift ONNX evidence
remains in the dated experiment report.

## Reference environment

- Apple M1 Max MacBook Pro, 10 CPU cores, 32 GB memory
- macOS 26.5.2 (25F84)
- Xcode 26.6 (17F113), Swift 6.3.3
- Rust/Cargo 1.92.0
- Python 3.14.4
- Node.js 24.18.0 for the supported Node wrapper run
- JDK 17.0.19 and Android NDK 26.1.10909125
- Release mode for Core ML latency and retrieval qualification

Host serial numbers and machine identifiers are intentionally not recorded.

## Immutable production artifact

The canonical package, tokenizer assets, Apache-2.0 attribution, and versioned
manifest are published in an uncompressed deterministic POSIX ustar archive.
The builder fixes entry order, timestamps, permissions, ownership, and archive
format, and permits regular files only.

| Property | Production value |
| --- | --- |
| Repository | `gungorbasa/retrievalkit-minilm` |
| Immutable commit | `405818d6afef1aaf2fc8da67da6caf20b55f0a28` |
| Archive | `all-MiniLM-L6-v2-coreml-fp32-v1.tar` |
| Archive bytes | `90,664,960` |
| Archive SHA-256 | `e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f` |
| Manifest | `archive-manifest-v1.json` |
| Manifest bytes | `2,029` |
| Manifest SHA-256 | `085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60` |
| Canonical payload-tree SHA-256 | `29f56defb74316d8491e7fba4eeba98cf24dc10b0e2b5b1df4a2d4e352f5fe5c` |

Two independent archive builds were byte-identical. Before pinning the
artifact, the immutable public HTTPS URL was downloaded into a clean directory,
safely extracted, and compared against the complete expected canonical tree.
The public response identified the exact immutable commit, and archive and
manifest byte counts and SHA-256 values matched.

The earlier loose Core ML directories remain historical artifacts. Their
Core ML Tools-rewritten package `Manifest.json` representation is not used by
production Swift.

## Acquisition and cache contract

`CoreMLEmbedder.load(...) async throws` is the small production entry point.
The existing local and bundled model initializers remain available.
`CoreMLEmbedder.prefetch(...)` provides explicit acquisition, and `localOnly`
guarantees that no network request is attempted.

Network activity is confined to `load(...)` and `prefetch(...)`. Retrieval
database construction, indexing, and search never acquire a model. The store:

- accepts only the pinned HTTPS artifact;
- shares one in-process concurrent acquisition task;
- downloads to a temporary file;
- verifies exact archive bytes and SHA-256 before extraction;
- rejects path traversal, links, duplicates, malformed headers, non-regular
  files, and any unexpected file;
- verifies the versioned manifest plus every expected payload size and hash;
- removes corrupt or partial state and atomically publishes the verified cache;
- compiles the `.mlpackage` locally with `MLModel.compileModel`;
- keys compiled state by immutable artifact identity and OS/Core ML
  compatibility; and
- deletes and recompiles once when compiled-cache loading fails.

The compiled model is not a portable artifact and is never uploaded.

## Model and retrieval conformance

The frozen fixture contains 48 corpus items and 42 queries. The production
Core ML `.all` FP32 vectors were compared with ONNX CPU FP32, and both provider
directions were exercised through RetrievalKit's actual signed-I8 database
encoding and graph-aware APIs.

| Comparison | Median cosine | Mean Top-10 | Exact Top-10 sets | Minimum |
| --- | ---: | ---: | ---: | ---: |
| ONNX CPU FP32 vs Core ML `.all` FP32 | 1.000000 | 100% | 100% | 100% |
| ONNX I8 database, Core ML query | 1.000000 | 99.76% | 97.62% | 90% |
| Core ML I8 database, ONNX query | 1.000000 | 99.76% | 97.62% | 90% |

The actual RetrievalKit paths were identical in both provider directions:

| Retrieval path | Mean Top-10 | Exact Top-10 sets | Minimum |
| --- | ---: | ---: | ---: |
| Vector | 99.76% | 97.62% | 90% |
| Hybrid | 100% | 100% | 100% |
| Graph-scoped vector | 100% | 100% | 100% |
| Graph-scoped hybrid | 99.29% | 92.86% | 90% |

Full-corpus BM25, graph-scoped BM25, and graph-only selection were exactly
identical. The two generated result files have SHA-256
`71e864a8445faae9933e196119a5343af2ebec446eb6bc20b30c564c264b8f42`
and
`7eb3cf309cd6b2e3fd08d8a28da4cae74f4478f68422146d4c4ec3ae32de3bfc`.

The persisted signed-I8 checks retained exactly 384 signed bytes plus one F32
scale per vector and no duplicate F32 payload. At 10K/25K/50K vectors, vector
files were exactly 3,880,000/9,700,000/19,400,000 bytes.

## Latency

The production Core ML harness used a 32-token query, 50 warm-ups, and 750
measured release-build embeddings.

| Boundary | Result |
| --- | ---: |
| Cold public download, verify, extract, and compile | 19.525 s |
| Cached artifact validation/prefetch | 185.662 ms |
| Verified cached initialization | 436.596 ms |
| First inference | 81.456 ms |
| Warm embedding p95 | 4.527 ms |
| I8 query validation, quantization, and retrieval p95 at 10K | 0.218 ms |
| Conservative sum of the two warm p95 boundaries | 4.745 ms |

RetrievalKit does not expose query quantization as a separate public timing
hook, so the reported retrieval boundary includes validation and
quantization. EmbeddingKit and retrieval are intentionally separate APIs, so
the table does not label the sum of independent p95 values as a directly
sampled end-to-end percentile. Even that conservative sum remains below 10 ms,
and retrieval-only remains below 8 ms.

## Commands and regression coverage

Core ML and deterministic artifact checks:

```sh
python3 -m unittest discover -s scripts/embedding -p 'test_*.py'
swift test --package-path wrappers/swift/EmbeddingKit
swift build -c release --package-path wrappers/swift/EmbeddingKit
EMBEDDINGKIT_LIVE_MODEL_TEST=1 swift test -c release \
  --package-path wrappers/swift/EmbeddingKit \
  --filter CoreMLModelStoreTests/testLivePinnedArtifactColdCachedLocalOnlyAndInference
```

Provider and RetrievalKit policy reruns:

```sh
'/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-runtime-venv/bin/python' \
  scripts/embedding/qualify-minilm-i8-storage-policy.py \
  --artifacts \
  '/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-models/retrievalkit-minilm' \
  --coreml-compute all \
  --output target/swift-coreml-production-qualification/provider-i8-coreml-all.json

cargo run --locked --release -p retrievalkit-embedding \
  --example qualify_retrieval_policy -- \
  '/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-conformance-input.json' \
  '/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/rust-cpu-fp32.json' \
  '/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/direct-coreml-fp32.json' \
  | tee target/swift-coreml-production-qualification/retrieval-policy.json
```

Regression and metadata checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked -p retrievalkit-core -p retrievalkit-graph \
  -p retrievalkit-embedding --all-targets -- -D warnings
cargo test --locked -p retrievalkit-core -p retrievalkit-graph \
  -p retrievalkit-embedding
cargo test --locked -p retrievalkit-embedding --examples
scripts/verify-swift-graph-wrapper.sh
swift test --package-path wrappers/swift/RetrievalKitPipeline
scripts/check-python-wrapper.sh
scripts/check-python-graph-wrapper.sh
scripts/check-browser-wasm.sh
python3 benchmarks/publication/validate_readme.py --repo .
python3 scripts/release/validate_release.py --repo .
python3 -m unittest benchmarks.publication.tests.test_readme_claims \
  scripts.release.test_release
```

The Rust core/graph/embedding suites, Swift retrieval/graph packages and
quickstarts, Python base/graph lint/type/test/wheel smoke, browser portable and
SIMD128 conformance plus package checks, Node base/graph build/type/lint/test
and package-install smoke, Kotlin/JVM tests/examples, Android AAR inspection,
and release metadata validation passed. `EmbeddingKit` ran 30 tests with two
expected opt-in skips; its live pinned-artifact test separately passed. The
embedding artifact scripts ran 26 tests. Existing non-EmbeddingKit wrapper
sources were unchanged, existing manifests gained no Core ML or Swift ONNX
dependency, and the Rust ONNX and Browser/WASM implementations were preserved.

No dependency was introduced by the Swift implementation, so no new dependency
notice regeneration was required. The release validator passed against the
already-current notice state.

## Remaining risks and boundaries

- Latencies are reference-host observations, not cross-device claims. iOS
  devices and future OS/Core ML versions should be qualified independently;
  the compatibility-keyed compiled cache and clean-recompile recovery prevent
  stale compiled state from becoming permanent.
- Cross-browser/device qualification for the pre-existing Browser/WASM work
  remains separate and pending.
- The working tree intentionally includes the owner's uncommitted Browser/WASM
  and shared Rust embedding work. This implementation preserved that state and
  is not a release commit.
- No RetrievalKit release, registry publication, Git tag, SDK upload, Core ML
  deletion, or provider fallback was performed or authorized.
