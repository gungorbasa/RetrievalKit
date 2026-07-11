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

The app has six benchmark modes:

- `Real Data`: loads the bundled Social Network index from
  `VectorKitIOSBench/Resources/social-network-index/`, runs vector, keyword,
  hybrid, and filtered keyword searches with a precomputed
  `BAAI/bge-small-en-v1.5` query embedding, and reports top real hits.
- `Smoke`: small link and UI smoke test.
- `Device`: physical-device validation profile. It runs `24K` chunks,
  `384d`/`768d`, `i8`, filtered and unfiltered, persistence enabled, and F32
  recall disabled so RSS is not inflated by ground-truth indexes.
- `Default`: the full FFI default benchmark, currently `24K` chunks,
  `384d`/`768d`, `f32`/`f16`/`i8`, filtered and unfiltered.
- `Compact`: the default benchmark with BM25 persistence disabled so vector-only
  persisted size can be measured.
- `Memory`: isolated lifecycle presets covering `24K`/`50K`, `384d`/`768d`,
  `F32`/`F16`/`I8`, and vector-only/hybrid workloads. Each run measures build,
  cold/warm search, save, unload, load, delete, and compaction. Relaunch before
  selecting another memory preset, and run Memory before any other benchmark
  after launch.

To start one preset automatically, add an Xcode scheme launch argument:

```text
--memory-scenario 24k-384d-i8-hybrid-t25
```

See `docs/product/memory-benchmark.md` for the report schema, budgets, CLI
command, and measurement limitations.

For the validation report, run `Device` on physical iPhone/iPad hardware and
capture the summary plus JSON output. The summary includes latency, persisted
size, load time, and resident memory after load. The raw JSON includes the same
metrics plus additional memory snapshots around build, search, save, load, and
post-load search.
