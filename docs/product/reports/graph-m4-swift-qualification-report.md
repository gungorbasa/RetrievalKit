# Graph M4 Swift Qualification Report

Date: 2026-07-12

## Outcome

The generic optional graph package is qualified through the Rust, C ABI, and
Swift boundary. The next implementation boundary is the optional Python graph
wrapper; no additional Swift graph feature work is required before it starts.

## Environment

- Host: Apple arm64, macOS 26.5.2 (25F84)
- Swift: Apple Swift 6.3.3
- Rust: rustc 1.92.0
- Build modes: Rust Apple artifacts in `release`; Swift verification in the
  SwiftPM test/run default debug mode

## Artifact qualification

Clean full XCFramework builds passed for both packages with these slices:

- `aarch64-apple-darwin`
- `aarch64-apple-ios`
- `aarch64-apple-ios-sim`

The verification script proves:

- base `VectorKitFFI` exports the core ABI and no graph ABI symbol;
- aggregate `VectorKitGraphFFI` exports the same core ABI plus graph ABI;
- base and graph Swift packages link and test in separate processes;
- the deterministic quickstart produces exactly the checked-in output.

The qualification found and fixed a macOS Bash 3 `set -u` failure when the
base build expanded an empty Cargo feature array. Both graph-free and graph
build modes now work through the same script.

## Contract coverage

- Canonical generic schema and record ingestion
- Node-ID and exact property seeds
- Bounded outgoing/incoming traversal, typed paths, and provenance
- Cancellation, typed truncation, projection diagnostics, and stable errors
- Generation-bound scoped exact, BM25, and hybrid retrieval
- Metadata filters, candidate limits, RRF, weighted fusion, and ranking traces
- Composite persistence, validation, reopen, and incompatible-version errors
- Explicit idempotent lifecycle closure and synchronized use-after-close safety
- Concurrent immutable reads with writer-preferring exclusive save/close
- One canonical V1 fixture producing identical Rust and Swift node, path,
  projection, filtered exact, and keyword results

## Commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
scripts/build-xcframework.sh
scripts/build-xcframework.sh --graph
scripts/verify-swift-graph-wrapper.sh --skip-build
```

All commands passed.

## Performance applicability

M4 changes are wrapper ownership, typed materialization, packaging, tests, and
documentation. They do not modify graph-free exact, BM25, or hybrid search
bodies. The M3 interleaved local measurements therefore remain the applicable
graph-free gate: exact +0.44%, BM25 +1.01%, hybrid +0.73%, all below 3%.
Pinned-hardware release qualification remains required before publishing a
device-wide performance claim.

## Python handoff contract

The Python wrapper must:

1. remain optional and leave the existing base Python distribution graph-free;
2. load exactly one compatible native core/graph implementation per process;
3. marshal typed values only and leave schema/query/ranking semantics in Rust;
4. expose the same stable error, cancellation, lifecycle, filter, trace, and
   persistence behavior as Swift;
5. consume `benchmarks/graph-conformance/v1/fixture.json` unchanged and match
   its Rust/Swift expectations;
6. provide a deterministic no-model quickstart before customer validation;
7. make no customer capacity or migration-equivalence claims until a private
   sanitized customer fixture is supplied.
