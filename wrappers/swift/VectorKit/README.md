# VectorKit Swift

Swift wrapper for VectorKit's Rust retrieval core.

The wrapper is intentionally thin:

- `VectorIndex` owns an opaque Rust index handle.
- `Filter` owns an opaque Rust filter handle.
- Retrieval, filtering, ranking, persistence, and traces stay in Rust.
- Swift provides Apple-platform API shape, ownership, and error mapping.

## Build And Test

The default package manifest consumes the built XCFramework at:

```text
target/apple/VectorKitFFI.xcframework
```

For a local macOS release-packaging smoke test:

```bash
cd ../../..
scripts/verify-swift-wrapper.sh
```

For the full Apple artifact, build all supported slices:

```bash
cd ../../..
rustup target add aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The script writes:

```text
target/apple/VectorKitFFI.xcframework
```

`Package.local.swift` exists for low-level development against
`target/debug/libvectorkit_ffi.a`, but release validation should use the
default `Package.swift` and the XCFramework.

## Usage

```swift
import VectorKit

let index = try VectorIndex(dimension: 3)

try await index.upsert(
    document: Document(id: "note-1", metadata: ["source": .string("notes")]),
    chunks: [
        ChunkInput(text: "local private notes", embedding: [1, 0, 0])
    ]
)

let filter = Filter.equals("source", .string("notes"))
let results = try await index.hybridSearch(
    text: "private notes",
    embedding: [1, 0, 0],
    topK: 5,
    filter: filter
)

for result in results {
    print(result.documentID, result.text, result.score)
}
```

## Current API Surface

- Create/load/save `VectorIndex`.
- Upsert and delete documents.
- Exact vector search.
- BM25 keyword search.
- Hybrid vector + keyword search.
- Typed metadata values.
- Typed filter builders: equals, not-equals, exists, range, in-values, all, any.
- Structured Swift errors mapped from Rust/FFI failures.

`VectorIndex` is an actor. Mutating and query operations are isolated to the
index instance and are called with `await` from outside the actor. `Filter` is
an immutable `Sendable` value; temporary Rust filter handles are built inside
the actor call and freed before returning.

The source package currently expects the XCFramework to be built in this
repository before `swift build` or `swift test`. A public binary release should
publish `VectorKitFFI.xcframework` and switch the binary target to a URL plus
checksum for tagged distribution.
