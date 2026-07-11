# Optional Graph M3 Benchmark Report

Date: 2026-07-11

Status: local development gate passed. Repeat release qualification on pinned
target hardware.

## Environment

- macOS 26.5.2 (25F84)
- Apple M1 Max
- Rust/Cargo 1.92.0
- optimized `cargo bench` profile
- no concurrent-load shape; local foreground process

## Graph-Free Regression Gate

The same checked-in harness was compiled once from pre-M1 commit `c6d0c99` and
once from the M3 worktree. The binaries were then run in five alternating pairs
to reduce compilation, thermal, and run-order bias. Each process built a 10,000
chunk, 384-dimensional F32 dot-product index, warmed each query 100 times, took
1,000 retrieval-only samples, and reported nearest-rank-ceil p95. Embedding and
index construction time were excluded.

Median p95:

| Mode | Pre-M1 | M3 | Delta | Gate |
| --- | ---: | ---: | ---: | ---: |
| Exact | 917 us | 921 us | +0.44% | pass |
| BM25 | 2,283 us | 2,306 us | +1.01% | pass |
| Hybrid | 3,271 us | 3,295 us | +0.73% | pass |

All modes remain below the approved +3% p95 regression ceiling. One earlier
non-interleaved three-run comparison exceeded the gate because the two binaries
were measured in separate blocks. The interleaved result is the accepted local
comparison; pinned-device release runs remain authoritative.

## Composite Persistence

Command: `cargo bench -p vectorkit-graph --bench composite_persistence`

The fixture contains 2,000 canonical records, one chunk per record, four
outgoing references per record, three warmups per measured operation, and 20
timed samples per operation.
Times use nearest-rank-ceil p95 and include filesystem sync and full validation.

| Measurement | Result |
| --- | ---: |
| Composite save p95 | 98 ms |
| Composite open p95 | 10 ms |
| Read-only validation p95 | 10 ms |
| Canonical schema | 367 bytes |
| Graph payload | 502,533 bytes |
| Complete database directory | 612,175 bytes |

The save benchmark rewrites and atomically activates a complete core/graph
generation. Open and validation verify the core manifest/checksums, graph
manifest/checksums, schema hash, corpus/generation binding, and graph payload.

## Bounded Query Check

Command: `cargo bench -p vectorkit-graph --bench bounded_traversal`

For 2,000 nodes and 8,000 edges: build 12 ms, three-hop traversal p95 18 us,
candidate projection p95 2 us, and scoped exact search p95 1,041 ns. Traversal,
projection, and search use 100 warmups and 500 timed samples each.
