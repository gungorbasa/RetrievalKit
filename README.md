# VectorKit

VectorKit is a local-first retrieval SDK for applications. The Rust core owns
exact vector search, BM25 keyword search, hybrid ranking, filtering, and
crash-safe persistence. Swift and Python expose thin, idiomatic wrappers.

V1 targets local indexes with fewer than 50,000 chunks. Exact search remains
the correctness baseline.

## Fastest Working Example

Prerequisites: Rust, Python 3.10 or newer, and a C compiler.

```bash
scripts/check-python-wrapper.sh
target/python-wrapper-check-venv-py*/bin/python \
  wrappers/python/examples/pipeline_quickstart.py
```

Expected final output:

```text
quickstart 1.0 VectorKit connects Rust retrieval to Swift and Python.
```

The check builds the Rust extension, installs it into an isolated environment,
runs lint and type checks, executes the Python tests, builds a wheel, and smoke
tests the installed wheel.

## Swift

Prerequisites: Xcode with the iOS and macOS SDKs plus the Rust Apple targets.

```bash
scripts/verify-swift-wrapper.sh
swift run --package-path wrappers/swift/VectorKitPipeline \
  vectorkit-pipeline-example
```

For a faster macOS-only development build:

```bash
scripts/build-xcframework.sh --macos-only
swift test --package-path wrappers/swift/VectorKit
```

## Rust

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Where to Go Next

- [Python wrapper](wrappers/python/README.md)
- [Swift wrapper](wrappers/swift/VectorKit/README.md)
- [Pipeline orchestration](wrappers/swift/VectorKitPipeline/README.md)
- [Product specification](docs/product/vectorkit-product-spec.md)
- [Documentation index](docs/README.md)
- [Release and migration notes](CHANGELOG.md)

The repository is currently source-first. Public package-registry distribution
and hosted documentation are not available yet.

Index updates and deletes use tombstones. Swift and Python expose explicit
`compact()` operations that reclaim their in-memory payload; saving afterward
publishes a smaller crash-safe snapshot.
