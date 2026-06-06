# VectorKit iOS Benchmark App

This Xcode project runs the Rust FFI benchmark through
`VectorKitFFI.xcframework`.

Build the XCFramework first:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The simulator framework slice is arm64-only. There is no
`x86_64-apple-ios` target in this project.

Open or build the project:

```bash
open wrappers/swift/VectorKitIOSBench/VectorKitIOSBench.xcodeproj
```

The app has two benchmark modes:

- `Smoke`: small link and UI smoke test.
- `Default`: the full FFI default benchmark, currently `24K` chunks,
  `384d`/`768d`, `f32`/`f16`/`i8`, filtered and unfiltered.
