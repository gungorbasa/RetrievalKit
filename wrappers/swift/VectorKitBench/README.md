# VectorKit Swift Benchmark Harness

This SwiftPM package is a macOS command-line harness for the Rust FFI benchmark.

Build the Rust FFI library first:

```bash
MACOSX_DEPLOYMENT_TARGET=14.0 cargo build -p vectorkit-ffi --release
```

To build the Apple XCFramework used by future app targets:

```bash
rustup target add aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The script writes `target/apple/VectorKitFFI.xcframework`.

Run a small link/smoke benchmark:

```bash
cd wrappers/swift/VectorKitBench
swift run vectorkit-bench --small-smoke
```

Run the default device benchmark:

```bash
cd wrappers/swift/VectorKitBench
swift run -c release vectorkit-bench
```

The default config is owned by `vectorkit-ffi` and currently runs:

- `24K` chunks
- `384d` and `768d`
- `f32`, `f16`, and `i8`
- unfiltered search
- filtered search with `filter_every=10`

You can override it with either `--config '<json>'` or
`--config-file config.json`. Add `--raw` to print compact JSON.
