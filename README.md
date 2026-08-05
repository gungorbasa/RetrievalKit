# ![RetrievalKit — local retrieval without a server](assets/readme/hero.svg)

RetrievalKit is an Apache-2.0 in-process retrieval SDK that replaces the
usual local stack — full-text index, vector extension, and hand-written
fusion code — with three separately callable query paths in one Rust core:
exact vector search, BM25/hybrid with query-time weighting, and graph-scoped
retrieval over relationships you declare in a schema. Native Swift, Python,
TypeScript, Kotlin, Android (packaging preview), and browser APIs.

It is built for 1K to fewer than 50K chunks that live on the device. Build,
persist, and query all run in your process: no account, no API key, no cloud
index build. The core makes no network calls; the optional embedding
packages download a pinned model once, then run offline. Embeddings are
caller-provided by default. The graph path needs no vectors, no metric, and
no embeddings at all, and graph scope is a hard candidate filter over the
unchanged exact ranker — it never changes a score.

<div align="center">

[![Release](https://img.shields.io/github/v/release/gungorbasa/RetrievalKit?include_prereleases&sort=semver)](https://github.com/gungorbasa/RetrievalKit/releases/tag/v0.1.0)
[![CI](https://github.com/gungorbasa/RetrievalKit/actions/workflows/ci.yml/badge.svg)](https://github.com/gungorbasa/RetrievalKit/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-0A0D12.svg)](LICENSE)

**[Quickstart](#quickstart)** · **[Choose an API](#choose-an-api)** ·
**[Install](#install)** · **[Run from source](#run-from-source)** ·
**[See validated benchmarks](#benchmarks)** · **[Documentation](#documentation)** ·
**[Public docs](https://retrievalkit-docs.gungorbasa.chatgpt.site)**

</div>

## Quickstart

The Python base package is the shortest way to see the complete retrieval path.
It accepts your document and query embeddings, stores the index locally, and
runs ranking in-process.

```bash
python -m pip install retrievalkit==0.1.0
```

```python
from retrievalkit import Document, RetrievalDatabaseBuilder

builder = RetrievalDatabaseBuilder(
    corpus_id="project-notes",
    metric="dot_product",
    encoding="f32",
)
builder.upsert(
    Document(
        id="decision-swift",
        text="We chose Swift for Project Apollo's Apple platform client.",
        metadata={"project": "apollo", "status": "approved"},
    ),
    embedding=[1.0, 0.0],
)
builder.upsert(
    Document(
        id="launch-checklist",
        text="Project Apollo launch checklist and release owners.",
        metadata={"project": "apollo", "status": "draft"},
    ),
    embedding=[0.0, 1.0],
)

database = builder.build()
hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    where={"project": "apollo"},
    limit=1,
)

print(hits[0]["document_id"])
```

```text
decision-swift
```

The two-dimensional vectors make the example deterministic; use embeddings
from the same production model for both indexed documents and queries. For a
checked-in runnable version, see
[`database_quickstart.py`](wrappers/python/examples/database_quickstart.py).

Prefer another language? Start with the
[Swift](docs/guides/swift.md), [Python](docs/guides/python.md),
[TypeScript](docs/guides/typescript.md), or [Kotlin](docs/guides/kotlin.md)
guide.

## Choose an API

RetrievalKit has three database types. Choose the smallest one that matches how
your application finds context.

| If you need to… | Use | Search behavior |
| --- | --- | --- |
| Search a flat collection of documents | `RetrievalDatabase` | Exact vector, BM25 text, or hybrid ranking |
| Traverse relationships without embeddings | `GraphDatabase` | Match, traverse, and project related records |
| Find related records, then rank only within that scope | `GraphRetrievalDatabase` | Graph-scoped exact vector, BM25, or hybrid ranking |

For retrieval-capable databases, the query inputs choose the mode:

```text
embedding only          → exact vector search
text only               → BM25 search
text + embedding + alpha → hybrid search
```

Metadata filters are hard constraints in every retrieval mode. Graph scope is
different: it answers “which records are related?” before the ranker runs.
Relationships are supplied by your application; RetrievalKit does not infer or
invent a graph.

## Install

RetrievalKit v0.1.0 is a published preview. Choose a base package for flat
corpora or the graph aggregate when records have useful relationships. Graph
aggregates already include base retrieval.

| SDK | Graph-enabled install | Qualified preview target |
| --- | --- | --- |
| Swift | `.package(url: "https://github.com/gungorbasa/RetrievalKit.git", from: "0.1.0")` | macOS 14+ arm64; iOS 15+ arm64 device and simulator |
| Python | `python -m pip install retrievalkit-graph==0.1.0` | macOS arm64; CPython 3.10–3.14 |
| Node.js | `npm install @gungorbasa/retrievalkit-graph@0.1.0` | macOS arm64; Node.js 22.13+ or 24 LTS |
| Browser | `npm install @gungorbasa/retrievalkit-browser@0.1.0` | Dedicated Worker; portable and SIMD128 WASM tiers |
| Kotlin/JVM | `implementation("io.github.gungorbasa:retrievalkit-graph:0.1.0")` | macOS arm64 native library; JDK 17 build, Java 11+ runtime |
| Android | `implementation("io.github.gungorbasa:retrievalkit-graph-android:0.1.0")` | API 24+ arm64-v8a packaging; live-device behavior unqualified |

Base packages are named `retrievalkit`, `@gungorbasa/retrievalkit`, and
`io.github.gungorbasa:retrievalkit`. Optional embedding integrations are
published separately so applications can bring their own model or use the
first-party local MiniLM provider.

> [!IMPORTANT]
> Python, Node, and Kotlin base and graph native aggregates are mutually exclusive within one process.
> Install exactly one retrieval aggregate; the
> independent embedding package may be used alongside either one.

## How it works

<p align="center">
  <img src="assets/readme/architecture.svg" width="100%"
       alt="One canonical corpus feeds retrieval-only, graph-only, or graph-scoped retrieval paths in the shared Rust core.">
</p>

The canonical corpus owns records, chunks, metadata, stable identities, and
generations. Retrieval and graph indexes are derived capabilities over that
state. The Rust core owns indexing, graph traversal, filtering, ranking,
traces, and persistence; wrappers provide idiomatic APIs and lifecycle
handling without reimplementing retrieval behavior.

Native databases use transactional, checksummed snapshots. Browser databases
are in-memory and owned by a dedicated Worker; browser persistence is not part
of v0.1.0.

### Privacy boundary

Database construction, indexing, filtering, retrieval, graph traversal, and
persistence require no network call. Embeddings remain explicit inputs. Use a
local provider when text must stay on-device; if your app sends text to a
remote embedding API, that embedding step is remote even though RetrievalKit
search remains local.

## Package matrix

Every published wrapper preserves the same corpus ownership, search semantics,
filtering rules, persistence guarantees where supported, and deterministic
ordering. Syntax and lifecycle remain native to each language.

| SDK | Capability | Status |
| --- | --- | --- |
| Swift `RetrievalKit` | Base corpus and retrieval | **Published preview** |
| Swift `RetrievalKitGraph` | Graph aggregate with retrieval | **Published preview** |
| Swift `EmbeddingKit` | Local Core ML embedding integration | **Published preview** |
| Swift `RetrievalKitPipeline` | Chunk → embed → index → search orchestration | **Published preview** |
| Python `retrievalkit` | Base corpus and retrieval | **Published preview** |
| Python `retrievalkit-graph` | Graph aggregate with retrieval | **Published preview** |
| Python `retrievalkit-embedding` | Local FP32 MiniLM embedding integration | **Published preview** |
| TypeScript `@gungorbasa/retrievalkit` | Base corpus and retrieval | **Published preview** |
| TypeScript `@gungorbasa/retrievalkit-graph` | Graph aggregate with retrieval | **Published preview** |
| TypeScript `@gungorbasa/retrievalkit-embedding` | Local FP32 MiniLM embedding integration | **Published preview** |
| Browser `@gungorbasa/retrievalkit-browser` | Worker-owned base, graph, and graph-scoped WASM retrieval | **Published preview** |
| Browser `@gungorbasa/retrievalkit-browser-embedding` | Worker-owned local FP32 MiniLM embedding | **Published preview** |
| Kotlin/JVM `io.github.gungorbasa:retrievalkit` | Base corpus and retrieval | **Published preview** |
| Kotlin/JVM `io.github.gungorbasa:retrievalkit-graph` | Graph aggregate with retrieval | **Published preview** |
| Kotlin/JVM `io.github.gungorbasa:retrievalkit-embedding` | Local FP32 MiniLM embedding integration | **Published preview** |
| Android `io.github.gungorbasa:retrievalkit-android` | Base AAR for arm64-v8a | **Published preview; live-device unqualified** |
| Android `io.github.gungorbasa:retrievalkit-graph-android` | Graph aggregate AAR for arm64-v8a | **Published preview; live-device unqualified** |
| Android `io.github.gungorbasa:retrievalkit-embedding-android` | Local FP32 MiniLM embedding AAR for arm64-v8a | **Published preview; live-device inference unqualified** |

Public SwiftPM, PyPI, npm, and Maven publication completed from the signed
release revision and authorized artifacts. Android qualification covers
cross-compilation, packaging, inventory, ABI/JNI contracts, and fresh consumer
resolution and compilation—not physical-device inference, compatibility,
memory, thermal behavior, or performance.

Swift ships one graph-capable native aggregate so `RetrievalKit` and
`RetrievalKitGraph` can coexist in one app. Selecting only `RetrievalKit` keeps
graph APIs out of the Swift target, although SwiftPM still downloads the shared
binary.

## Run from source

Clone the repository and run one checked quickstart from the repository root.
Each script verifies its required toolchain before building.

<details>
<summary><strong>Python</strong></summary>

Requires Rust and CPython 3.10–3.14.

```bash
PYTHON_BIN=python3 scripts/check-python-graph-wrapper.sh
target/python-graph-wrapper-check-venv-py*/bin/python \
  wrappers/python-graph/examples/graph_retrieval_quickstart.py
```

Expected output: `graph-hybrid=decision-swift`.

</details>

<details>
<summary><strong>TypeScript / Node.js</strong></summary>

Requires Rust and Node.js 22.13+ or 24 LTS.

```bash
cd wrappers/typescript
npm ci
npm run preflight
npm run build
node graph/examples/graph-retrieval.mjs
```

The result contains `documentId: 'local'`.

</details>

<details>
<summary><strong>Kotlin / JVM</strong></summary>

Requires Rust and JDK 17 on macOS arm64. Published bytecode runs on Java 11+.

```bash
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
export PATH="$JAVA_HOME/bin:$PATH"
cd wrappers/kotlin
./scripts/preflight.sh jvm
./scripts/build-native.sh jvm
./gradlew :example-retrieval:run
```

Expected output includes
`kotlin: Kotlin calls the local Rust retrieval core. (1.0)`.

</details>

<details>
<summary><strong>Swift</strong></summary>

Requires Rust, Swift 6.2, and macOS 14+ on Apple silicon.

```bash
scripts/build-xcframework.sh --macos-only --graph
scripts/run-swift-quickstart.sh graph-retrieval
```

Expected output: `graph-hybrid=decision-swift`.

</details>

## Benchmarks

The public benchmark claims below are historical observations from a frozen
Phase 6 workload. They are not measurements of the current checkout. They apply
to RetrievalKit revision `9c784d2f11b91bb907150aa1b6046880ff89fde6`, were
reported on 2026-07-21, and expire on 2027-07-21. Retrieval timings exclude
embedding generation.

<details>
<summary><strong>Exact retrieval on Apple M1 Max</strong></summary>

<!-- claim:P6-MAC-EXACT-001 -->
On the frozen exact F32, 384-dimensional, top-10 benchmark, RetrievalKit revision
`9c784d2` delivered the following P50 unfiltered retrieval ratios versus
sqlite-vec `0.1.9` on an Apple M1 Max running macOS 26.5.2. Each lane used 100
measured queries after 20 warmups; embedding was excluded.

| Corpus | sqlite-vec / RetrievalKit P50 | Observation |
| ---: | ---: | --- |
| 10K | 7.17× | RetrievalKit lower latency |
| 25K | 7.60× | RetrievalKit lower latency |
| 50K | 7.29× | RetrievalKit lower latency |
<!-- /claim -->

<!-- claim:P6-MAC-EXACT-002 -->
With the same frozen filter enabled, the P50 retrieval ratios were 10.38× at
10K, 9.08× at 25K, and 8.43× at 50K versus sqlite-vec `0.1.9`. This was the
same Apple M1 Max exact F32 workload at revision `9c784d2`; embedding was
excluded.
<!-- /claim -->

<!-- claim:P6-MAC-CORRECTNESS-001 -->
RetrievalKit exact F32 and sqlite-vec `0.1.9` both passed the frozen Phase 5
identity, filtering, deletion, determinism, and reload gates at 10K, 25K, and
50K. This result is scoped to the frozen workload and is not proof for every
possible input.
<!-- /claim -->

These ratios describe one exact-search workload, not universal competitor
superiority. See the
[methodology](benchmarks/publication/artifacts/phase6-publication-v1/methodology.md)
and [Mac evidence report](benchmarks/publication/artifacts/phase6-publication-v1/mac-systems-performance.md).

</details>

<details>
<summary><strong>Graph-scoped quality on HotpotQA</strong></summary>

<!-- claim:P6-QUALITY-001 -->
Across the frozen 296-query HotpotQA linked-abstracts test comparison,
graph-scoped weighted-I8 retrieval increased NDCG@10 from 0.858036 to 0.927909
versus whole-corpus weighted-I8 retrieval. There were 121 wins, 157 ties, and
18 losses. This is a scoped quality result, not a universal graph winner or a
latency claim.
<!-- /claim -->

<!-- claim:P6-QUALITY-002 -->
On the same frozen 296-query weighted-I8 comparison, Recall@10 increased from
0.871622 to 0.957770 and complete-evidence recall@10 increased from 0.743243 to
0.922297. Sixteen queries lost on each recall measure; those losses are part of
the result.
<!-- /claim -->

<!-- claim:P6-QUALITY-003 -->
The frozen candidate stage reduced the mean per-query candidate set by 972.65×
while retaining 96.79% candidate recall and 94.26% candidate complete evidence
across 296 valid graph queries, with zero empty scopes. Candidate reduction is
not a retrieval-latency speedup and retention was not perfect.
<!-- /claim -->

The workload contains 12,670 chunks. See the full
[retrieval-quality evidence](benchmarks/publication/artifacts/phase6-publication-v1/retrieval-quality.md).

</details>

<details>
<summary><strong>Physical-device qualification</strong></summary>

<!-- claim:P6-DEVICE-001 -->
On a physical iPhone 17 Pro Max (`iPhone18,2`, `V54AP`), the supported 10K,
25K, and 50K F32/I8 product workflows passed. All six graph-free
candidate-to-baseline median-session P95 ratios were at or below the frozen
1.03 gate. Query/prepare evidence used iOS 26.5.1 (23F81); remaining lifecycle
evidence used iOS 26.5.2 (23F84). This is supported-workload qualification for
that device, with embedding excluded, not a claim about other hardware.
<!-- /claim -->

<!-- claim:P6-DEVICE-SAFETY-001 -->
V1 targets fewer than 50K chunks. The 100K Phase 4b stress workload remains
`not_run_device_safety`, produced zero accepted stress artifacts, and is not
eligible for support, performance, latency, quality, product, or marketing
claims.
<!-- /claim -->

See the
[physical-device evidence report](benchmarks/publication/artifacts/phase6-publication-v1/physical-device-systems-performance.md)
and [Phase 6 validation result](benchmarks/publication/artifacts/phase6-publication-v1-validation.json).

</details>

## Scope and limitations

- V1 is optimized for exact retrieval over local indexes with fewer than 50K
  chunks. HNSW and other ANN indexes are intentionally deferred.
- Native persistence uses transactional, checksummed snapshots. Browser v0.1.0
  databases are Worker-owned and in-memory; portable cross-platform snapshots
  are not claimed.
- The initial native binary targets focus on arm64 Apple platforms. Node.js,
  Python, and Kotlin/JVM packages initially target macOS arm64.
- Android API 24+ arm64-v8a is a packaging-qualified preview. Live-device
  inference, compatibility, lifecycle, memory, thermal behavior, offline
  restart, and performance remain unqualified.
- Embedding latency is separate from retrieval latency. RetrievalKit accepts
  caller-provided embeddings and never hides a hosted inference call inside a
  database operation.
- Benchmark evidence supports the named frozen workloads only; it is not a
  universal performance or quality claim.

## Documentation

### Start by language

- [Swift guide](docs/guides/swift.md)
- [Python guide](docs/guides/python.md)
- [TypeScript and browser guide](docs/guides/typescript.md)
- [Kotlin/JVM and Android guide](docs/guides/kotlin.md)

### Architecture and product decisions

- [Product specification](docs/product/retrievalkit-product-spec.md)
- [Capability-separated architecture](docs/product/capability-separated-architecture.md)
- [Compatibility policy](docs/product/compatibility-policy.md)
- [Release process](docs/product/release-process.md)

### Wrapper references

- [Swift base API](wrappers/swift/RetrievalKit/README.md) and
  [Swift graph API](wrappers/swift/RetrievalKitGraph/README.md)
- [Python base API](wrappers/python/README.md) and
  [Python graph API](wrappers/python-graph/README.md)
- [TypeScript / Node.js API](wrappers/typescript/README.md)
- [Browser / WebAssembly API](wrappers/browser/README.md)
- [Kotlin/JVM and Android API](wrappers/kotlin/README.md)

## Contributing, support, and license

Focused bug reports, documentation corrections, reproduction cases, and
changes within the V1 product scope are welcome. Read
[CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request and use the
[issue templates](.github/ISSUE_TEMPLATE) for bugs or feature requests.

Report vulnerabilities privately through the [security
policy](SECURITY.md). Release history is recorded in the
[changelog](CHANGELOG.md).

RetrievalKit is licensed under the [Apache License 2.0](LICENSE). Copyright and
distribution notices are in [NOTICE](NOTICE).
