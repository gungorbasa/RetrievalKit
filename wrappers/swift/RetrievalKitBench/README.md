# RetrievalKit Swift Benchmark Harness

> [RetrievalKit](../../../README.md) › Tooling › macOS benchmark harness

This SwiftPM package is a macOS command-line harness for the Rust FFI benchmark.
It is contributor tooling, not an application SDK or a source of public claims
without the matching frozen benchmark contract.

## Build the native library

Build the Rust FFI library first:

```bash
MACOSX_DEPLOYMENT_TARGET=14.0 cargo build -p retrievalkit-ffi --release
```

To build the Apple XCFramework used by future app targets:

```bash
rustup target add aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The script writes `target/apple/RetrievalKitFFI.xcframework`.

## Run

Run a small link/smoke benchmark:

```bash
cd wrappers/swift/RetrievalKitBench
swift run retrievalkit-bench --small-smoke
```

Run the default device benchmark:

```bash
cd wrappers/swift/RetrievalKitBench
swift run -c release retrievalkit-bench
```

## Default workload

The default config is owned by `retrievalkit-ffi` and currently runs:

- `24K` chunks
- `384d` and `768d`
- `f32`, `f16`, and `i8`
- unfiltered search
- filtered search with `filter_every=10`
- persistence save/load metrics and post-load search latency
- full BM25 persistence by default

You can override it with either `--config '<json>'` or
`--config-file config.json`. Add `--raw` to print compact JSON.

Use `{"persist_bm25":false}` to measure the compact vector-only persistence
profile.
