# Capability-Separated Architecture Qualification

Date: 2026-07-12

Status: local development qualification passed. Repeat the performance gate on
pinned release hardware before publishing a performance claim.

## Qualified Contract

The qualified implementation provides one canonical Rust `CorpusIndex` with
optional derived engines and three concrete Swift database products:

```text
RetrievalDatabase      = CorpusIndex + RetrievalIndex
GraphDatabase          = CorpusIndex + GraphEngine
GraphRetrievalDatabase = CorpusIndex + GraphEngine + RetrievalIndex
```

Graph-only construction accepts no vector configuration or embeddings.
Semantic retrieval persists no BM25 payload and rejects hybrid queries with a
typed error. Hybrid retrieval supports semantic and hybrid queries. Combined
graph selections scope retrieval through generation-bound native candidate IDs
without copying records into Swift.

The base `VectorKitFFI` artifact exports retrieval symbols but no graph ABI.
The optional `VectorKitGraphFFI` aggregate exports one copy of the core and the
graph ABI. The Swift types expose only their enabled engine views:
`GraphDatabase.graph`, `RetrievalDatabase.retrieval`, and both views on
`GraphRetrievalDatabase`.

## Environment

- macOS 26.5.2 (25F84)
- Apple M1 Max
- Rust 1.92.0
- Apple Swift 6.3.3
- release benchmark profile; foreground local process

## Graph-Free p95 Gate

The checked-in `graph_free_regression` harness was compiled from pre-refactor
commit `22f5be9` and candidate commit `203d87d` into separate target
directories. The two binaries ran in five AB/BA interleaved pairs on the same
host to reduce order and thermal bias.

Each process built a deterministic 10,000-chunk, 384-dimensional F32
dot-product index. Each mode used 100 warmups and 1,000 retrieval-only samples,
top-k 10, and nearest-rank-ceil p95. Index construction and embedding generation
were excluded. The table reports the median of five process p95 values.

| Mode | Pre-refactor | Candidate | Delta | +3% gate |
| --- | ---: | ---: | ---: | ---: |
| Exact | 879 us | 874 us | -0.57% | pass |
| Internal BM25 | 2,143 us | 2,142 us | -0.05% | pass |
| Hybrid | 3,137 us | 3,150 us | +0.41% | pass |

The only positive regression was hybrid at +0.41%, leaving 2.59 percentage
points of margin below the approved ceiling.

## Verification

The following checks passed:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- full macOS, iOS device, and iOS simulator XCFramework builds for base and
  graph aggregate artifacts
- `swift test` for `VectorKitShared`, `VectorKit`, and `VectorKitGraph`
- base/graph native symbol-isolation checks
- retrieval-only, graph-only, and combined quickstarts with exact expected
  output

Rust coverage includes scoped/unscoped equivalence, empty/sparse/dense scopes,
filters, stale generation and cross-corpus rejection, bulk hydration, deletes,
superseded chunks, persistence reload, corruption, and crash-safe activation.
Swift coverage adds capability-specific construction, missing/dimension errors,
semantic-without-BM25 persistence, scoped combined retrieval, cross-corpus
selection rejection, persistence reload, and automatic graph-selection
lifetime.

## Result

All seven implementation gates passed. The capability-separated Rust, FFI, and
Swift architecture is qualified for continued development. Customer-specific
fixtures remain deferred evidence and are not required by the generic schema
or package implementation.

## Semantic Base Follow-up

The public retrieval configuration was subsequently simplified to require
semantic vectors and accept `.hybrid` through a bounded `extras` set. The Rust
configuration and FFI now express the same semantic-base/optional-hybrid model;
the graph aggregate ABI is version 5.

The graph-free harness was compiled from baseline commit `3508f11` and the
follow-up worktree, then run in three AB/BA interleaved pairs with the same
fixture and sampling protocol described above. Median p95 remained inside the
gate:

| Mode | Baseline | Semantic-base configuration | Delta | +3% gate |
| --- | ---: | ---: | ---: | ---: |
| Exact | 907 us | 896 us | -1.21% | pass |
| Internal BM25 | 2,223 us | 2,165 us | -2.61% | pass |
| Hybrid | 3,146 us | 3,157 us | +0.35% | pass |
