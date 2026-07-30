# Shared ONNX Embedding Experiment — 2026-07-26

## Historical status

This is a completed experiment report. On 2026-07-27, RetrievalKit retired the
experimental Swift `EmbeddingKitONNX` package and its Apple ONNX Runtime
XCFramework build material after the direct Core ML FP32 path was selected for
production Swift embedding. The optional Rust `retrievalkit-embedding` ONNX
provider remains available for cross-platform, non-Apple use. Results below are
preserved as historical qualification evidence and do not describe an active
Swift package or production fallback.

## Scope and invariants

This experiment evaluated the optional Rust `retrievalkit-embedding` crate and
the formerly separate Swift `EmbeddingKitONNX` package. It did not add
embedding execution to `retrievalkit-core`, change a RetrievalKit
database/search method, replace `CoreMLEmbedder`, or add ONNX dependencies to
an existing Swift, Python, Node, Kotlin, Android, or browser package.

The frozen contract is all-MiniLM-L6-v2 source revision
`c9745ed1d9f207416be6d2e6f8de32d1f16199bf`, at most 256 WordPiece tokens,
masked mean pooling, L2 normalization, and exactly 384 finite F32 output
values.

## Immutable artifacts

- Public repository: `gungorbasa/retrievalkit-minilm`
- Artifact commit: `617ce926c1f9e0289365d3e999474cc28b1645d4`
- `manifest-v1.json` SHA-256:
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`
- ONNX FP32: 90,396,663 bytes,
  `beaa83a6670eb0ddae4d7c6f7a89acf69ed5d1fd747b083fa6f9f0145b2ee891`
- ONNX FP16: 45,317,052 bytes,
  `105482078caa44c0b57a70545207feb6b1a27bd36353a5cbeb6f2577eb409675`
- ONNX Q8: 30,040,323 bytes,
  `0017d61f7a597949b62c14cec764bc971f5b451483597686b6a304920f3a9250`

Matching fixed-256 FP32, FP16, and weight-only INT8 Core ML packages and all
common tokenizer files are covered by the same manifest.

## Verification methodology

- Artifact export: locked Python 3.11 environment, deterministic repeated
  export, manifest validation, and dynamic ONNX sequence checks at lengths
  1, 8, and 256.
- Provider correctness: cold download, verified cached load, local-only mode,
  corruption recovery, interrupted-transfer cleanup, concurrent preparation,
  profile isolation, empty input, Unicode, and 256-token truncation.
- Vector and ranking conformance: FP32 reference, median cosine gates of
  0.9999/0.999/0.995 and Top-10 overlap gates of 99%/98%/95% for
  FP32/FP16/Q8.
- Latency harness: M1 Max, release build, with configurable token lengths
  16/32/64/128/256 and batches 1/8/32. The acceptance slice used 50 warm-ups
  and 750 measured calls at the common 32-token, batch-one query shape. A
  smaller 5-warm-up/10-sample run exercised every token/batch shape. The full
  50/750 matrix remains an explicit long-running qualification command rather
  than evidence claimed by this report.
- Retrieval index for end-to-end measurements: 10,000 chunks, 384 dimensions,
  per-vector symmetric signed-I8 storage, exact dot-product top-10.

## Results

### Runtime and artifact qualification

The Rust provider loaded the official ONNX Runtime 1.24.3 arm64 dynamic
library through `ort` 2.0.0-rc.12's API-24 boundary. The Swift package linked a
locally built official 1.24.3 XCFramework containing macOS arm64, iOS arm64,
and Apple-Silicon iOS Simulator slices with CPU, XNNPACK, and Core ML execution
providers. The XCFramework remains under `target/apple` and is not published.

The initial published FP16 Swift integration test passed cold preparation,
verified cache-only reload, single inference, and unequal-length batch
inference. After FP32 became canonical, the same live integration test passed
against the pinned FP32 artifact.
The Rust live tests passed the same pinned repository for FP32, FP16, and Q8.
A representative Rust FP16 cold load took 6,647.62 ms, cached load 214.19 ms,
first inference 4.02 ms, and subsequent warm inference about 4.02 ms. Download
and load numbers are environment-specific and are not part of the search
latency gate.

### Embedding and ranking conformance

All outputs contained exactly 384 finite F32 values and normalized to unit
length. The first artifact-level validation, using the Python ONNX Runtime
wheel and Core ML Tools, passed every export gate:

| Runtime | Profile | Median cosine | Gate | Mean Top-10 overlap | Gate |
| --- | --- | ---: | ---: | ---: | ---: |
| ONNX CPU | FP32 | 1.000000 | 0.9999 | 100.00% | 99% |
| ONNX CPU | FP16 | 0.999999 | 0.999 | 99.76% | 98% |
| ONNX CPU | Q8 | 0.995259 | 0.995 | 95.95% | 95% |
| Core ML CPU | FP32 | 1.000000 | 0.9999 | 100.00% | 99% |
| Core ML CPU | FP16 | 0.999988 | 0.999 | 99.52% | 98% |
| Core ML CPU | Q8 | 0.999017 | 0.995 | 96.43% | 95% |

The ONNX Q8 gate requires seven quality-sensitive MatMul nodes to remain in
full precision. Quantizing those nodes failed the frozen quality gate; the
selective recipe above is therefore part of the artifact contract. This
artifact-level result is not sufficient to qualify every packaged provider:
the unified SDK-boundary comparison below found provider-specific Q8 drift.

Retrieval receives only the resulting F32 vectors. Existing Rust conformance
tests cover exact, BM25, hybrid, graph-only, and graph-scoped retrieval, and
the optional embedding crate does not participate in BM25 or graph traversal.
Consequently BM25 and graph-only results are byte-for-byte on the existing
paths; vector differences are bounded by the profile ranking gates above.

### Unified 32-token provider comparison

The final provider comparison runs every Swift path through one release
executable: direct fixed-256 Core ML through the unchanged `CoreMLEmbedder`,
and the ONNX artifact through CPU, XNNPACK plus CPU fallback, or Core ML EP plus
CPU fallback. Each latency is nearest-rank p95 from 50 warm-ups and 750
measurements at 32 tokens and batch one. Ranking uses 48 corpus items and 42
queries, with Swift ONNX CPU FP32 as the frozen reference.

`Exact Top-10` is the fraction of queries whose complete result set matched.
`Mean Top-10` averages per-query set overlap. Profile gates remain
99%/98%/95% mean overlap and 0.9999/0.999/0.995 median cosine for
FP32/FP16/Q8:

| Swift path | Profile | Load ms | First ms | Warm p95 ms | Median cosine | Mean Top-10 | Exact Top-10 | Minimum | Gate |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Direct Core ML | FP32 | 434.05 | 1517.39 | 3.225 | 1.000000 | 100.00% | 100.00% | 100% | pass |
| Direct Core ML | FP16 | 1869.85 | 12.89 | 3.029 | 0.999983 | 99.05% | 90.48% | 90% | pass |
| Direct Core ML | Q8 | 2012.45 | 9.55 | 3.032 | 0.999007 | 96.19% | 64.29% | 80% | pass |
| ONNX CPU | FP32 | 156.75 | 3.74 | 3.697 | 1.000000 | 100.00% | 100.00% | 100% | pass |
| ONNX CPU | FP16 | 139.20 | 4.59 | 4.144 | 0.999999 | 99.76% | 97.62% | 90% | pass |
| ONNX CPU | Q8 | 105.65 | 3.02 | **2.241** | 0.995231 | 94.76% | 50.00% | 80% | **fail** |
| ONNX XNNPACK | FP32 | 148.03 | 13.64 | 12.268 | 1.000000 | 100.00% | 100.00% | 100% | pass |
| ONNX XNNPACK | FP16 | 133.59 | 7.92 | 7.607 | 0.999986 | 99.52% | 95.24% | 90% | pass |
| ONNX XNNPACK | Q8 | 106.60 | 4.45 | 4.113 | 0.995204 | 95.00% | 52.38% | 80% | pass at boundary |
| ONNX Core ML EP | FP32 | 1059.22 | 108.73 | 15.759 | 1.000000 | 100.00% | 100.00% | 100% | pass |
| ONNX Core ML EP | FP16 | 231.34 | 4.86 | 4.491 | 0.999999 | 99.76% | 97.62% | 90% | pass |
| ONNX Core ML EP | Q8 | 631.77 | 101.31 | 14.720 | 0.995271 | 93.57% | 38.10% | 80% | **fail** |

Direct Core ML FP16 is the fastest Apple row, and direct Core ML Q8 provides
effectively no latency improvement while reducing ranking stability. ONNX CPU
Q8 is the fastest raw row but misses its mean Top-10 gate by 0.24 percentage
points, so it is not qualified. XNNPACK Q8 only touches the gate and is slower
than direct Core ML FP16. The ONNX Core ML execution provider is both slower
and, for Q8, below the quality gate. A later policy qualification selected FP32
as the canonical cross-runtime profile because its measured rankings were
identical across ONNX and direct Core ML and its latency remained below the
product target.

ONNX Runtime reported partial Core ML partitioning—12/327 supported nodes for
FP16, 176/326 for FP32, and 231/435 for Q8—so the Core ML execution-provider
rows are not full-model Core ML execution.

### Direct Core ML compute-unit comparison

A follow-up M1 Max run on 2026-07-27 compared the fixed-256 FP16 Core ML
package with every public Core ML compute-unit combination. These are allowed
compute sets, not proof of per-node placement: Core ML exposes CPU-only,
CPU+GPU, CPU+Neural Engine, and all units, but not strict GPU-only or
Neural-Engine-only execution.

Three independent 32-token, batch-one runs each used 50 warm-ups and 750
measurements:

| Allowed compute units | Warm p95 runs (ms) | Median p95 | Range | Median cosine | Mean Top-10 | Gate |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| CPU only | 5.968 / 5.994 / 5.932 | 5.968 | 5.932–5.994 | 0.999988 | 99.52% | pass |
| CPU + GPU | 3.303 / 4.835 / 3.205 | 3.303 | 3.205–4.835 | 0.999999 | 99.52% | pass |
| CPU + Neural Engine | 3.200 / 3.231 / 3.193 | 3.200 | 3.193–3.231 | 0.999983 | 99.05% | pass |
| All | 3.066 / 3.118 / 3.436 | **3.118** | 3.066–3.436 | 0.999983 | 99.05% | pass |

The full FP16 token-length sweep, also with 50 warm-ups and 750 measurements
per cell, produced:

| Tokens | CPU only | CPU + GPU | CPU + Neural Engine | All |
| ---: | ---: | ---: | ---: | ---: |
| 16 | 5.970 | 3.227 | 3.197 | **3.150** |
| 32 | 6.087 | 4.115 | **3.224** | 3.234 |
| 64 | 6.294 | 3.359 | 3.344 | **3.296** |
| 128 | 6.307 | 3.501 | **3.477** | 3.483 |
| 256 | 6.590 | 3.850 | **3.676** | 3.704 |

Acceleration is therefore useful for this Core ML package, but a GPU-focused
policy is not the best default. `all` has the best repeated median and gives
Core ML freedom to schedule for each Apple device. CPU+Neural Engine is the
most stable explicit policy on this host. CPU+GPU is faster than CPU-only but
more variable. The existing `.all` Core ML behavior remains unchanged.

The same conclusion does not transfer to ONNX Runtime: Swift ONNX CPU FP16
measured 4.144 ms versus 4.491 ms for its partially partitioned Core ML
execution provider, so ONNX CPU remains the deterministic default. Retrieval
itself remains CPU-resident; its approximately 0.218 ms p95 leaves no
justification for GPU transfer and launch overhead at the qualified 10K scale.

Direct Core ML Q8 with CPU+GPU was also attempted twice and aborted both times
inside Apple's Metal Performance Shaders graph compiler with an MLIR pass
manager assertion (process status 134). CPU-only, CPU+Neural Engine, and all
completed, but Q8 remains less ranking-stable and no faster than FP16 on the
accelerated path. It is not recommended for Apple production use.

The actual Rust API-24 boundary was then run with the same texts and official
1.24.3 dynamic library. Retrieval is the unchanged exact search over 10K
384-dimensional signed-I8 stored vectors:

| Rust ONNX CPU | Embedding p95 | Retrieval p95 | End-to-end p95 | Mean Top-10 | Exact Top-10 | Quality |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| FP32 | 3.689 ms | 0.218 ms | 3.967 ms | 100.00% | 100.00% | pass |
| FP16 | 5.797 ms | 0.218 ms | 6.019 ms | 100.00% | 100.00% | pass |
| Q8 | **2.216 ms** | 0.214 ms | **2.561 ms** | 94.76% | 50.00% | **fail** |

Every Rust row meets the latency target, but Q8 does not meet the frozen
ranking gate. The Q8 result was repeated and was deterministic. FP16 therefore
remains the default; Q8 is an explicit experimental performance profile rather
than a qualified general-use profile.

### Token and batch scaling smoke

The following p95 milliseconds are a 5-warm-up/10-sample Q8 smoke run. They
validate dynamic input and batch operation, not the 50/750 acceptance gate:

| Tokens | Rust b1 | Rust b8 | Rust b32 | Swift CPU b1 | Swift CPU b8 | Swift CPU b32 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 1.545 | 7.148 | 24.451 | 1.554 | 8.166 | 22.296 |
| 32 | 2.143 | 12.869 | 40.284 | 2.360 | 11.819 | 47.799 |
| 64 | 3.637 | 20.470 | 79.379 | 3.726 | 22.832 | 151.887 |
| 128 | 6.700 | 41.988 | 174.685 | 6.878 | 43.076 | 168.886 |
| 256 | 14.102 | 91.008 | 429.736 | 15.343 | 94.624 | 374.737 |

The sub-10 ms end-to-end target is a warm single-query target; it is not
claimed for maximum-length inputs or large batches.

### Fixed versus flexible Core ML

The requested 50-warm-up/750-sample Core ML comparison completed across the
token-length groups. The fixed `[1,256]` package measured 2.915 ms p95 and
3.038 ms p99. The flexible `[1,2...256]` candidate measured 6.518 ms p95 and
7.612 ms p99. Although result conformance passed, p95 regressed by 123.58% and
both performance gates failed. The existing fixed-256 Core ML model remains
unchanged.

### Regression and packaging checks

- `cargo fmt --all -- --check` and strict Clippy passed for
  `retrievalkit-core`, `retrievalkit-graph`, and `retrievalkit-embedding`.
- Targeted Rust suites passed: core 149 integration/unit checks, embedding 10
  offline checks, and graph 33 integration/unit checks. The three-profile
  published-model test also passed separately against official Runtime 1.24.3.
- `EmbeddingKitONNX` passed 12 offline tests with one network test skipped,
  then passed the canonical FP32 live pinned-download test separately; its clean release
  build passed.
- The artifact exporter has 10 passing unit tests. Artifact-level ONNX/Core ML
  conformance passed; the SDK-boundary matrix then correctly rejected ONNX CPU
  and ONNX Core ML EP Q8.
- `retrievalkit-core`, existing `EmbeddingKit`, and existing Python, Node,
  Kotlin, Android, and browser package manifests contain no ONNX embedding
  dependency.
- `THIRD_PARTY_NOTICES.md` was regenerated from the additive Cargo lockfile.

Two unrelated checked-in regression fixtures remain red. The full Cargo
workspace reaches 47 passing CLI tests before the quality-v3 tests fail because
`benchmarks/retrieval-quality/v3/collection.json` records
`manifests/chunking.json` as 715 bytes while the checked-in file is 718 bytes.
The unchanged `EmbeddingKit` suite passes its other tests but its tiny
WordPiece fixture expects the word “RetrievalKit” to split into two known
tokens even though the fixture vocabulary produces `[UNK]`. Neither baseline
was altered as part of this additive experiment.

## Release decision

No RetrievalKit package release, version tag, Core ML deletion, or existing
provider replacement was authorized by this experiment. The measured Swift
ONNX comparison is complete and its package and Apple XCFramework build
material were retired on 2026-07-27. The optional Rust ONNX provider remains
unchanged with FP32 as its default, while production Swift uses the direct
fixed-256 Core ML FP32 path.
