# FP32 Embedding and I8 Database Qualification — 2026-07-27

## Decision

FP32 is the canonical embedding-model profile for both ONNX and Core ML.
RetrievalKit continues accepting 384-dimensional normalized F32 embeddings at
the public boundary and storing them with the existing
`I8ScalarQuantized` database encoding by default.

These are independent choices. Q8 model weights reduce model-artifact size;
they do not reduce a database that already stores one signed byte per
dimension plus one F32 scale per vector.

The optional Rust `retrievalkit-embedding` crate defaults to FP32. Production
Swift uses the direct `CoreMLEmbedder` FP32 path; the completed Swift ONNX
comparison is preserved as historical evidence but its package is retired.
The retrieval core, database formats, search methods, and non-embedding wrapper
behavior are unchanged.

## Cross-provider qualification

The frozen comparison uses 48 corpus items, 42 queries, and four diagnostics.
Both providers return exactly 384 finite values and normalize them to unit
length.

| Comparison | Median cosine | Mean Top-10 | Exact Top-10 | Minimum | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| ONNX CPU FP32 vs direct Core ML FP32 | 1.000000 | 100% | 100% | 100% | pass |
| ONNX-built I8 database, Core ML query | 1.000000 | 99.76% | 97.62% | 90% | pass |
| Core ML-built I8 database, ONNX query | 1.000000 | 99.76% | 97.62% | 90% | pass |

The production gates are median cosine at least 0.9999, mean Top-10 overlap at
least 99%, exact Top-10 sets on at least 90% of queries, and minimum per-query
overlap of 90%. Both Core ML CPU-only reference qualification and the
production `.all` compute-unit qualification passed. The `.all` report is
generated under `target` with SHA-256
`71e864a8445faae9933e196119a5343af2ebec446eb6bc20b30c564c264b8f42`.

The same frozen vectors were then passed through the real
`GraphRetrievalDatabase` implementation. Each direction compares the
provider's own F32 database/query ranking with its I8 database queried by the
other FP32 provider. The graph-scoped rows use a real graph query selecting 32
of the 48 corpus records before RetrievalKit projects and ranks the candidate
scope.

| Actual RetrievalKit path | Mean Top-10 | Exact Top-10 sets | Minimum | Result |
| --- | ---: | ---: | ---: | --- |
| Vector | 99.76% | 97.62% | 90% | pass |
| Hybrid | 100% | 100% | 100% | pass |
| Graph-scoped vector | 100% | 100% | 100% | pass |
| Graph-scoped hybrid | 99.29% | 92.86% | 90% | pass |

The rows are identical in both directions. BM25 hits, including scores and
matched terms, were exactly equal across all four provider/storage
databases for both full-corpus and graph-scoped searches. The graph-only
selection was also identical because it does not consume embeddings. The
generated qualification report has SHA-256
`7eb3cf309cd6b2e3fd08d8a28da4cae74f4478f68422146d4c4ec3ae32de3bfc`.

## Persisted size and retrieval latency

The existing release CLI built and persisted F32 and I8 indexes over the same
synthetic corpus on the M1 Max. Each row used 384 dimensions, cosine distance,
top 10, and 10 measured queries. Total database bytes include the manifest,
vectors, compressed chunks, canonical records, BM25 state, and tombstones.

| Chunks | Encoding | Vector bytes | Total database bytes | Retrieval p95 | Recall@10 vs F32 |
| ---: | --- | ---: | ---: | ---: | ---: |
| 10,000 | F32 | 15,360,000 | 15,609,765 | 0.945 ms | 1.00 |
| 10,000 | I8 | **3,880,000** | **4,129,778** | **0.181 ms** | 0.99 |
| 25,000 | F32 | 38,400,000 | 39,037,765 | 2.203 ms | 1.00 |
| 25,000 | I8 | **9,700,000** | **10,337,778** | **0.427 ms** | 1.00 |
| 50,000 | F32 | 76,800,000 | 78,058,810 | 4.811 ms | 1.00 |
| 50,000 | I8 | **19,400,000** | **20,658,824** | **0.791 ms** | 0.98 |

Every I8 vector file exactly equals
`rows × (384 signed bytes + 4 scale bytes)`. There is no retained F32 vector
copy in the persisted database. I8 reduces the vector section by 74.74% and
the complete synthetic database by about 73.5%.

The synthetic recall column is a small latency/size diagnostic with 10 queries;
the frozen provider-ranking suite above is the cross-runtime correctness gate.

## Latency and compatibility

The earlier 50-warm-up/750-measurement provider run recorded:

| Path | Embedding p95 | Retrieval p95 | End-to-end p95 |
| --- | ---: | ---: | ---: |
| Rust ONNX CPU FP32 | 3.689 ms | 0.218 ms | 3.967 ms |
| Historical Swift ONNX CPU FP32 | 3.697 ms | measured separately | below target with I8 retrieval |
| Direct Core ML FP32 | 3.225 ms | measured separately | below target with I8 retrieval |

The retrieval-only p95 remains below 8 ms through the 50K diagnostic, and the
qualified 32-token warm embedding-plus-retrieval path remains below 10 ms.
BM25 and graph-only operations do not consume embeddings. Vector, hybrid, and
graph-scoped retrieval continue using the unchanged Rust retrieval behavior.
For I8 databases, the reported retrieval timing already includes query
validation and query quantization before scoring. Query quantization is not
exposed as a separate public timing hook; preserving the current API makes the
retrieval p95 the stricter boundary measurement. Browser Worker transfer
remains a separate browser-only measurement and is not part of these native
provider timings.

No migration, RetrievalKit package publication, release tag, registry
publication, or Core ML deletion is part of this qualification. The separately
authorized corrected Core ML artifact publication and production Swift loader
are recorded below.

The original pinned FP32 live-download/cache/inference tests passed through
both the Rust and now-retired Swift ONNX SDK boundaries. Those totals remain
historical experiment evidence. Production `EmbeddingKit` subsequently passed
30 tests with two expected opt-in skips, its release build, and a live
immutable Core ML download/compile/cached-local-only/inference run. The new
archive builder and the existing embedding artifact/qualification scripts pass
26 tests. Fresh Swift retrieval/graph, Rust core/graph/embedding, Python, Node,
Browser/WASM, Kotlin/JVM, Android, and release-metadata results are recorded in
the production implementation report.

The additive Rust qualification example passed against the production Core ML
`.all` vectors in both directions. Strict Rust Clippy, browser package
type-check/lint/tests/build, and the browser portable/SIMD128 smoke and
conformance suite also passed.

## Artifact-distribution resolution

The target-local Core ML packages were restored from the existing canonical
export copy and the complete `manifest-v1.json` validation passed before the
final CPU-only and `.all` qualification runs. No model content was regenerated.

The earlier immutable commit served loose Core ML package `Manifest.json`
files in a Core ML Tools-rewritten representation rather than the canonical
representation covered by the root manifest's tree digest. Production Swift
does not consume those loose directories.

The canonical FP32 package, tokenizer assets, license, notice, and attribution
are now distributed as deterministic uncompressed POSIX ustar archive
`all-MiniLM-L6-v2-coreml-fp32-v1.tar` at immutable public commit
`405818d6afef1aaf2fc8da67da6caf20b55f0a28`. It is `90,664,960` bytes with
SHA-256
`e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f`.
Its `archive-manifest-v1.json` is `2,029` bytes with SHA-256
`085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60`,
and its canonical payload-tree SHA-256 is
`29f56defb74316d8491e7fba4eeba98cf24dc10b0e2b5b1df4a2d4e352f5fe5c`.
Two real builds were byte-identical. A clean public HTTPS re-download, safe
extraction, and full expected canonical-tree comparison passed before the pin
was added to `EmbeddingKit`.
