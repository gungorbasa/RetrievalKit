# Turbovec Review Notes

Reviewed repository:
[`RyanCodrai/turbovec`](https://github.com/RyanCodrai/turbovec)

Snapshot reviewed:

- Git commit: `efe29a1`
- Rust crate: `turbovec` `0.8.0`
- Python package: `turbovec` `0.7.0`

## Summary

Do not adopt `turbovec` as RetrievalKit's V1 retrieval engine.

`turbovec` is a compressed approximate vector index based on TurboQuant.
RetrievalKit V1 is intentionally centered on exact vector search, BM25 keyword
search, hybrid ranking, typed metadata filters, persistence, and Swift/iOS
integration for small local indexes.

The repository is still useful as a source of implementation ideas for future
optimization work, especially around filtered scans, cache warming, stable ID
maps, binary persistence validation, and benchmark instrumentation.

## What Looks Useful

### Filter Inside the Vector Scan

`turbovec` supports allowlist or mask filtering inside the scoring path instead
of scoring all rows and filtering afterward.

RetrievalKit already narrows many filtered searches through metadata candidate
offsets. The useful generalization is to keep pushing filter information closer
to the tight scoring loop:

- Build active/candidate offset lists or bitsets before scoring.
- Skip candidate blocks that cannot contain matching chunks.
- Keep a correctness predicate check after candidate narrowing.
- Report how many rows or blocks were skipped for debugging and benchmarks.

This fits RetrievalKit V1 because it improves exact search rather than replacing
it with approximate retrieval.

### Explicit Cache Warmup

`turbovec` has a `prepare()` method that eagerly initializes derived search
state so the first user query does not pay lazy setup cost.

RetrievalKit can use the same API idea if future search structures have meaningful
load-time or post-mutation cache work. A RetrievalKit version should keep the
contract simple:

- Safe to call more than once.
- Safe after load.
- Invalidated by add, update, delete, or compaction when needed.
- No change to search results.
- Covered by concurrency tests if search state is shared.

### Stable ID Map Over Positional Storage

`turbovec` layers stable external `u64` IDs over positional vector slots and
uses swap-remove for O(1) deletion.

RetrievalKit has richer chunk/document semantics, so the API shape should not be
copied directly. The idea is still relevant for internal compaction:

- Keep hot vector storage positional and compact.
- Maintain explicit mappings from chunk IDs to vector offsets.
- Treat offset movement during delete/compaction as an internal detail.
- Test that removed, superseded, and moved chunks never reappear.

### Versioned Binary Persistence

`turbovec` validates magic bytes, file versions, expected payload lengths, and
some incompatible old formats.

RetrievalKit should continue to be strict in the same spirit:

- Validate magic, version, dimension, metric, vector encoding, counts, and file
  lengths.
- Reject incompatible formats with actionable errors.
- Detect truncated payloads and trailing bytes.
- Keep persistence tests for every format version transition.

### Quantized Search Research

TurboQuant itself is not a V1 replacement for exact search. It may be worth
benchmarking later if RetrievalKit needs stronger compression than the current
`I8ScalarQuantized` path can provide.

Any future exploration should measure:

- Recall versus exact F32 results.
- Latency on target Apple devices.
- Persisted size and resident memory.
- Filtered-search behavior.
- Cost of keeping a second exact or higher-quality rerank store.

Do not add this mode before exact/hybrid V1 behavior is polished and measured
on realistic datasets.

### Benchmark and Debug Counters

`turbovec` exposes low-level behavior around block skipping for tests and
benchmarks.

RetrievalKit could expose similar internal counters in benchmark output or trace
diagnostics:

- Active rows scanned.
- Rows skipped by tombstone/version state.
- Rows skipped by metadata candidate narrowing.
- Blocks skipped by filter bitsets.
- Vector rows actually scored.
- Final candidates materialized.

These counters would make retrieval performance easier to explain without
changing public search semantics.

### Concurrency Tests for Shared Search State

`turbovec` tests concurrent search, concurrent cache initialization, search
after load, and mutation invalidating cached layouts.

If RetrievalKit adds shared immutable search caches or a public warmup method,
copy the testing pattern:

- Same query returns the same hits across threads.
- `prepare()` races do not change results.
- Add/update/delete invalidates only the affected derived state.
- Loaded indexes behave the same as in-memory indexes.

## What Not to Copy

- The primary approximate retrieval algorithm for V1.
- Python or framework integrations.
- Panic-based public API behavior for invalid input.
- The exact file format or public API shape.
- Heavy BLAS/faer/ndarray-style dependency surface for the iOS/macOS SDK path.

## Verification Notes

Commands run against the temporary clone:

```bash
cargo test -p turbovec
cargo clippy -p turbovec --all-targets -- -D warnings
```

Results:

- `cargo test -p turbovec` passed.
- `cargo clippy -p turbovec --all-targets -- -D warnings` failed on lint debt,
  including unused items, style warnings, high argument counts, and type
  complexity. This is not a runtime failure, but it means the crate should not
  be vendored or adopted without cleanup if RetrievalKit keeps strict Rust checks.

## Recommendation

Keep `turbovec` as a deferred research reference. The immediately useful ideas
are filter-aware scoring loops, explicit cache warmup, strict binary format
validation, and better benchmark counters. These support RetrievalKit's exact V1
direction without pulling in an approximate index as a core dependency.
