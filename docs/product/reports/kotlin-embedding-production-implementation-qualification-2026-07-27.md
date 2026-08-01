# Kotlin Embedding Production Implementation and Qualification — 2026-07-27

## Decision

RetrievalKit now has separate, optional Kotlin/JVM and Android arm64-v8a
embedding packages. They expose the canonical FP32
`sentence-transformers/all-MiniLM-L6-v2` provider through an isolated JNI
aggregate over the existing `retrievalkit-embedding` Rust crate.

Embedding remains outside `retrievalkit-core`, every retrieval database, and
the existing base/graph wrappers. RetrievalKit still accepts F32 vectors
publicly and independently defaults database storage to
`I8ScalarQuantized`; existing databases and persistence formats do not change.
No artifact was published.

> Status update (2026-08-01): the owner designated Android API 24+
> arm64-v8a as an explicit v0.1.0 preview. The host-side evidence in this report
> remains valid, but live-device inference, compatibility, and performance are
> unqualified and deferred rather than a v0.1.0 publication blocker. This
> report does not claim that Android device inference passed.

## Environment

- Host: Apple M1 Max, arm64
- OS: macOS 26.5.2, build 25F84
- Xcode: 26.6, build 17F113
- Rust: `rustc 1.92.0`, `cargo 1.92.0`
- Gradle: 8.10.2
- Kotlin plugin: 1.9.22
- Build JDK: Android Studio JBR 17.0.11
- Android target: API 24, NDK 26.1.10909125, arm64-v8a
- Build mode: Rust and package release builds
- Android device: none attached; Android evidence is cross-compilation and
  package inspection, not live inference

## Architecture and API

```text
Kotlin/JVM :embedding ───────┐
                             ├─ retrievalkit-jni-embedding
Android :android-embedding ──┘             │
                                           └─ retrievalkit-embedding

retrievalkit-core / graph / retrieval JNI: no dependency
```

`OnnxEmbedder` is blocking and `AutoCloseable`. Its public operations are
`load`, `prefetch`, `embed`, `embedBatch`, immutable `modelInfo`, and
deterministic `close`. Calls on one instance are serialized; the package adds
no coroutine dependency. `AndroidOnnxEmbedder` supplies the application cache
directory and returns the same provider.

The production surface exposes only FP32. It rejects empty or blank text and
empty batches, fixes model input at 256 WordPiece tokens, and requires exactly
384 finite, L2-normalized F32 values. Model precision is not a database vector
encoding option.

The JNI layer contains no retrieval or graph symbols. It uses opaque handles,
panic containment, primitive arrays, stable typed exception categories, and
deterministic invalid/closed-handle failures.

## Immutable Model and Runtime Identities

Model source revision:

```text
sentence-transformers/all-MiniLM-L6-v2
c9745ed1d9f207416be6d2e6f8de32d1f16199bf
```

Model artifact pin:

```text
repository: gungorbasa/retrievalkit-minilm
commit: 617ce926c1f9e0289365d3e999474cc28b1645d4
manifest-v1.json SHA-256:
b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2
```

Official ONNX Runtime 1.24.3 identities:

| Artifact | Exact bytes | SHA-256 |
|---|---:|---|
| macOS arm64 `libonnxruntime.1.24.3.dylib` | 27,724,968 | `b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729` |
| official Android AAR | 40,948,335 | `67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce` |
| selected Android arm64-v8a `libonnxruntime.so` | 25,831,632 | `4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad` |

The Android AAR also matched the published SHA-1
`e17cad728482733e3787abaf2a0bbe1b8122ff8a`. Runtime preparation verifies the
source archive, selected library, and exact ONNX Runtime license and
third-party notices before replacing generated build state. No native runtime
binary is checked into source.

## Model Acquisition and Network Boundary

Only `load` and explicit `prefetch` may acquire model artifacts.
`localOnly=true` performs no download. The Kotlin wrapper uses the Rust
provider's immutable HTTPS URLs, exact size and SHA-256 validation,
cross-process file lock, per-download temporary file, file and directory
sync, atomic rename, corrupt/partial cleanup, and verified cache reuse.
Inference never reads the model store or performs network work.

A genuine empty-cache run through the packaged JAR measured verified public
prefetch separately:

| Stage | Time |
|---|---:|
| Verified cold prefetch | 31,957.196 ms |
| Local cached session load and provider warmup | 1,391.191 ms |
| First measured inference after load | 6.951 ms |

The cache contained only the pinned manifest, FP32 ONNX model, and tokenizer
inventory. The downloaded manifest and model hashes were respectively:

```text
b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2
beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891
```

A packaged-JAR load against an empty cache with `localOnly=true` failed with
the typed `ModelAcquisitionException`. It did not fall back to network or
another provider.

Two independent JVM processes also started verified prefetch against one
empty cache concurrently. They completed in `14,848.206` and `15,129.522 ms`,
emitted byte-identical vector files with SHA-256
`6dee748d33c107537ddb8cd503a2e9c531b174b3d33186a514fb644872aab57e`,
and left no `.tmp` or `.partial` file. This exercises the shared cross-process
lock and atomic publication at the Kotlin/JNI boundary.

## Frozen FP32 Conformance

The frozen input contains 48 corpus items, 42 ranking queries, and four
diagnostics, including Unicode and truncation coverage. The reference is the
frozen Rust ONNX CPU FP32 output. Gates were median cosine at least 0.9999,
mean Top-10 overlap at least 99%, exact Top-10 sets at least 90%, and no query
below 90% overlap.

| Metric | Kotlin/JVM result | Gate |
|---|---:|---:|
| Vectors satisfying 384/finite/normalized contract | 94/94 | 94/94 |
| Median cosine | 1.0 | >= 0.9999 |
| Minimum cosine | 0.9999999999998386 | diagnostic |
| Mean Top-10 overlap | 100% | >= 99% |
| Exact Top-10 sets | 100% | >= 90% |
| Minimum per-query Top-10 overlap | 100% | >= 90% |

The packaged cold-cache and explicit-runtime qualification runs emitted
byte-identical JSON vectors. Evidence SHA-256:

```text
Kotlin FP32 vectors:
6dee748d33c107537ddb8cd503a2e9c531b174b3d33186a514fb644872aab57e

Conformance report:
ab47f656537b8011c1aefdbff7cdc7be7c4cce3cacca9b694c8691665f4bae98

Final packaged-JAR benchmark:
a585321f2680f6d789f8da666b1a2805e9d018ca7e9db7aad2d95de00267ceaa
```

## Actual RetrievalKit I8 Qualification

The Rust reference and Kotlin vectors were exercised in both database/query
directions through actual `GraphRetrievalDatabase` vector, hybrid,
graph-scoped vector, and graph-scoped hybrid APIs using
`I8ScalarQuantized`. Query quantization is included.

| Path | Mean Top-10 overlap | Exact sets | Minimum overlap |
|---|---:|---:|---:|
| Vector | 99.76% | 97.62% | 90% |
| Hybrid | 100% | 100% | 100% |
| Graph-scoped vector | 100% | 100% | 100% |
| Graph-scoped hybrid | 99.29% | 92.86% | 90% |

The metrics were symmetric in both directions and passed all gates. BM25,
graph-scoped BM25, and graph-only selection were exactly identical. The report
SHA-256 is:

```text
7eb3cf309cd6b2e3fd08d8a28da4cae74f4478f68422146d4c4ec3ae32de3bfc
```

The persisted I8 regression passed and proves one signed byte per dimension
plus one F32 scale per vector, with no duplicate F32 payload.

## Latency

The release benchmark used exactly 50 warm-ups and 750 measured queries. The
input is fixed at 32 tokens, intra-operation threads are four, and
inter-operation threads are one. The final packaged artifact's cached
initialization was `1,667.211 ms` and its first measured inference was
`8.906 ms`.

| Boundary | p50 | p95 | p99 |
|---|---:|---:|---:|
| Final packaged-JAR warm embedding | 6.627 ms | 8.175 ms | 9.337 ms |

The frozen actual native 10K×384d I8 query validation, quantization, and
retrieval p95 is 0.218 ms. Retrieval sources and native paths did not change,
and the Kotlin vectors are byte-identical to the Rust FP32 reference.
Separately adding the packaged-JAR embedding and retrieval boundaries gives
8.393 ms p95, below the 10 ms combined gate. Retrieval-only remains below the
8 ms gate.

## Package Qualification

The deterministic JVM JAR contains only Kotlin embedding classes, the macOS
arm64 JNI aggregate, official ONNX Runtime, project legal files, and ONNX
Runtime legal files. It is 10,248,900 bytes with SHA-256:

```text
5f358cfa7a9a4d403223d26106629adebacf51c60e16fe213007f03427c3015d
```

The Android AAR contains only these native entries:

```text
jni/arm64-v8a/libretrievalkit_embedding_jni.so
jni/arm64-v8a/libonnxruntime.so
```

Project and runtime legal files are deterministic entries inside
`classes.jar`. The AAR is 12,061,602 bytes with SHA-256:

```text
ecc7a93ce6917f3887cf560355c11d1a97a87b15f9cea8449a036c39e79ea996
```

ELF inspection identifies both libraries as 64-bit little-endian arm64.
The JNI aggregate has only `libdl.so` and `libc.so` direct dynamic
dependencies; ONNX Runtime is loaded by the approved dynamic boundary. Closed
inventory checks reject extra ABIs, unexpected native files, unsafe ZIP
entries, missing legal files, and runtime hash/size drift.

## Commands

Core, Kotlin, and tooling checks:

```sh
cargo test -p retrievalkit-jni-embedding
cargo clippy -p retrievalkit-jni-embedding --all-targets --all-features -- -D warnings

JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  wrappers/kotlin/gradlew -p wrappers/kotlin \
  :embedding:test \
  :embedding:inspectEmbeddingArtifact \
  :android-embedding:inspectEmbeddingAar \
  --no-daemon

python3 -m unittest \
  wrappers/kotlin/scripts/test_prepare_embedding_runtime.py \
  wrappers/kotlin/scripts/test_verify_embedding_package.py \
  scripts/embedding/test_qualify_kotlin_embedding.py

python3 wrappers/kotlin/scripts/verify-embedding-package.py jvm \
  --archive wrappers/kotlin/embedding/build/libs/embedding-0.1.0.jar \
  --project-license LICENSE --project-notice NOTICE

python3 wrappers/kotlin/scripts/verify-embedding-package.py android \
  --archive wrappers/kotlin/android-embedding/build/outputs/aar/android-embedding-release.aar \
  --project-license LICENSE --project-notice NOTICE
```

Frozen vector and latency run:

```sh
python3 scripts/embedding/qualify-kotlin-embedding.py \
  --input target/python-node-embedding-qualification/input.json \
  --output target/kotlin-embedding-cold-qualification/kotlin-jvm-fp32.json \
  --benchmark-output target/kotlin-embedding-cold-qualification/kotlin-jvm-benchmark.json \
  --embedding-jar wrappers/kotlin/embedding/build/libs/embedding-0.1.0.jar \
  --packaged-libraries \
  --download-if-missing \
  --cache-directory target/kotlin-embedding-cold-cache \
  --java-home "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  --intra-threads 4

python3 scripts/embedding/validate-python-node-wrapper-conformance.py \
  --input target/python-node-embedding-qualification/input.json \
  --reference "/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/rust-cpu-fp32.json" \
  --candidate kotlin-jvm=target/kotlin-embedding-cold-qualification/kotlin-jvm-fp32.json \
  --output target/kotlin-embedding-qualification/conformance-report.json

cargo run --locked --release \
  -p retrievalkit-embedding \
  --example qualify_retrieval_policy -- \
  target/kotlin-embedding-i8-qualification/texts.json \
  "/Users/gungorbasa/.codex/worktrees/a110/Vector Search/target/embedding-provider-vectors/rust-cpu-fp32.json" \
  target/kotlin-embedding-i8-qualification/kotlin-vectors.json
```

## Regression and Scope Audit

- Kotlin embedding unit tests: 13 passed.
- Runtime/package/qualification tooling tests: 13 passed.
- JNI Rust tests: 4 passed; strict Clippy passed.
- Kotlin base/graph JVM and Android packages remain separate.
- Rust core, graph, and optional embedding regressions passed.
- Persisted I8 payload regression passed.
- Dependency trees confirm `retrievalkit-core` gained no JNI, ONNX Runtime,
  tokenizer, HTTP, or embedding dependency.
- Package inventories contain no retrieval or graph classes.
- A before/after SHA-256 audit covered 684 existing source and metadata files.
  Differences were limited to the root Cargo manifests/lock, Kotlin guidance,
  active product docs, Kotlin Gradle settings, and the CI workflow that
  intentionally integrate this slice. Python, Node, Swift, browser/WASM, Rust
  retrieval, and existing Kotlin retrieval source files were byte-identical.
- Dependency notices and release metadata were validated. Nothing was
  published, tagged, staged, or committed.

## Remaining Risks

- No Android device was attached. Live Android model acquisition, inference,
  lifecycle, performance, memory, thermal behavior, and offline restart remain
  unqualified and deferred. Under the 2026-08-01 owner decision, they are not
  v0.1.0 publication blockers and no device pass may be claimed.
- The initial Android artifact supports only arm64-v8a and minSdk 24.
- The model cache is roughly 97 MB and the platform runtime materially
  increases JAR/AAR unpacked size.
- Cold public download time depends on network conditions; the measured
  31.957 seconds is diagnostic, not a latency guarantee.
- macOS JVM evidence is Apple Silicon only. Kotlin Multiplatform, Intel macOS,
  Linux, Windows, and additional Android ABIs are not claimed.
- Maven coordinates remain provisional pending the repository's naming
  clearance. No Maven or RetrievalKit artifact has been published.
