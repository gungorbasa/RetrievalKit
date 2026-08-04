# RetrievalKit iOS Benchmark App

> [RetrievalKit](../../../README.md) › Tooling › iOS benchmark app

This Xcode project runs the Rust FFI benchmark through
`RetrievalKitFFI.xcframework`.

> [!CAUTION]
> The Phase 4b physical-device collection is closed. Do not install, launch, or
> resume its device workloads. The commands below document validation and
> development surfaces; they do not authorize new physical-device execution.

## Qualification boundary

Phase 4 uses two separate release products in this project. `RetrievalKitIOSBench`
links only `RetrievalKitFFI` and records zero graph state creations, graph file
opens, and graph dispatches. `RetrievalKitIOSGraphBench` links only the aggregate
`RetrievalKitGraphFFI` and preflights the frozen workload/encoding,
fresh-process, stage, lifecycle, RSS, session, device, power, and thermal
protocol. Runtime flags never turn the graph-linked product into the graph-free
lane.

Every automated launch waits until UIKit reports the app as active before it
enters the benchmark runner. The wait is bounded at 30 seconds, occurs before
any measured FFI operation or timer starts, and fails closed without emitting
benchmark evidence if the app never reaches the foreground. Manual launches
do not use this gate. The Foundation-only gate has a deterministic standalone
test that can be run with:

```bash
mkdir -p target/phase4b/foreground-gate-tests
swiftc \
  wrappers/swift/RetrievalKitIOSBench/Shared/ForegroundExecutionGate.swift \
  benchmarks/device-graph/tests/ForegroundExecutionGateTests.swift \
  -o target/phase4b/foreground-gate-tests/ForegroundExecutionGateTests
target/phase4b/foreground-gate-tests/ForegroundExecutionGateTests
```

Phase 4b resumed after the foreground fix under authorization v4 while
preserving the completed v3 paths in the same artifact root. That collection
is now closed. Do not install, launch, or resume either physical-device
benchmark for Phase 4b. Final split-lineage validation supplies both historical
binary generations plus the validation-only device-safety cancellation
authorization:

```bash
python3 benchmarks/device-graph/validate_artifacts.py \
  --mode phase4b \
  --repo . \
  --artifact-root target/phase4b/device-results-v3-02b8971 \
  --authorization benchmarks/device-graph/phase4b-execution-authorization-v4.json \
  --base-binary target/phase4b/final-9201410/Build/Products/Release-iphoneos/RetrievalKitIOSBench.app/RetrievalKitIOSBench \
  --graph-binary target/phase4b/final-9201410/Build/Products/Release-iphoneos/RetrievalKitIOSGraphBench.app/RetrievalKitIOSGraphBench \
  --base-framework target/apple/RetrievalKitFFI.xcframework/ios-arm64/RetrievalKitFFI.framework/RetrievalKitFFI \
  --graph-framework target/apple/RetrievalKitGraphFFI.xcframework/ios-arm64/RetrievalKitGraphFFI.framework/RetrievalKitGraphFFI \
  --prior-authorization benchmarks/device-graph/phase4b-execution-authorization-v3.json \
  --prior-base-binary target/phase4b/final-cb87477/Build/Products/Release-iphoneos/RetrievalKitIOSBench.app/RetrievalKitIOSBench \
  --prior-graph-binary target/phase4b/final-cb87477/Build/Products/Release-iphoneos/RetrievalKitIOSGraphBench.app/RetrievalKitIOSGraphBench \
  --prior-base-framework target/apple/RetrievalKitFFI.xcframework/ios-arm64/RetrievalKitFFI.framework/RetrievalKitFFI \
  --prior-graph-framework target/apple/RetrievalKitGraphFFI.xcframework/ios-arm64/RetrievalKitGraphFFI.framework/RetrievalKitGraphFFI \
  --stress-cancellation-authorization benchmarks/device-graph/phase4b-device-safety-cancellation-authorization-v1.json
```

## Build and linkage checks

Build both release products and inspect their arm64 symbols with:

```bash
scripts/verify-ios-benchmark-linkage.sh
```

## Historical Phase 4 protocol

The graph-capable release launch requires `--phase4-graph-preflight`, one
`--phase4-workload`, and one `--phase4-encoding f32|i8`. Add
`--physical-device-required` only in Phase 4b. Simulator output identifies
itself and cannot satisfy the physical-device contract.

A final query session uses `--phase4-query-session`, `--phase4-session`, and
`--phase4-device-role` with the same workload, encoding, and physical-device
flags. The graph-only product builds one frozen configuration, runs every
query category with 100 excluded warmups and exactly 1,000 measured samples,
serializes raw stage and direct end-to-end timings, then exits. Run one session
per fresh app process and preserve the complete stdout JSON atomically.

Lifecycle launches use `--phase4-lifecycle-sample`, `--phase4-sample`, and
`--phase4-operation`. Supported operations are `prepare`, `build`, `save`,
`read_only_validation`, `cold_load`, `warm_load`, and `replay`. Every launch
records raw 1 ms RSS samples and uses the app's isolated Application Support
directory; `save` always targets a unique sample directory and refuses to
overwrite prior evidence.

## Local app setup

Build the XCFramework first:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The simulator framework slice is arm64-only. There is no
`x86_64-apple-ios` target in this project.

Open or build the project:

```bash
open wrappers/swift/RetrievalKitIOSBench/RetrievalKitIOSBench.xcodeproj
```

## Benchmark modes

The app has six benchmark modes:

- `Real Data`: loads the bundled Social Network index from
  `RetrievalKitIOSBench/Resources/social-network-index/`, runs vector, keyword,
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

The release harness contains the historical experimental
`100k-384d-v3-stress-f32` and `100k-384d-v3-stress-i8` presets for iPhone 17
Pro Max. They are not V1 product workloads. Phase 4b execution of those presets
was permanently canceled for device safety under Contract V1 Amendment 3. Do
not run the preflight or either preset for the closed qualification. The
following describes the historical, pre-cancellation protocol only:

```bash
cargo run --release -p retrievalkit-cli -- bench phase4 preflight \
  --manifest target/phase4a-100k/a/manifest.json \
  --mac-report target/phase4a-100k/mac/mac-correctness-report.json
```

Only when `safe_to_attempt` is `true`, launch one encoding in a fresh process
with both `--memory-scenario 100k-384d-v3-stress-i8` and
`--phase4-100k-preflight-safe`. Use the same fresh-process, thermal, 1 ms RSS
sampling, five-memory-repetition, and three-final-session protocol as the
supported workloads. If preflight is unsafe, launch with
`--phase4-100k-preflight-unsafe`; the harness emits a `stress` row with status
`not_run_memory_safety` without allocating the index. A 100K row is rejected
unless it is non-marketing, 384d, exactly 100,000 chunks, and F32 or I8.

Launch-argument runs write the complete JSON report to standard output and exit
with status `0` on success or `2` when a configured budget is exceeded. This
allows `devicectl --console` and CI/device-lab jobs to collect results without
copying text from the UI.

See `docs/product/memory-benchmark.md` for the report schema, budgets, CLI
command, and measurement limitations.

For the validation report, run `Device` on physical iPhone/iPad hardware and
capture the summary plus JSON output. The summary includes latency, persisted
size, load time, and resident memory after load. The raw JSON includes the same
metrics plus additional memory snapshots around build, search, save, load, and
post-load search.
