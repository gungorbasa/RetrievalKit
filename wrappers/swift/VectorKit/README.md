# VectorKit Swift

Swift wrapper for VectorKit's Rust retrieval core.

The wrapper is intentionally thin:

- `VectorIndex` owns an opaque Rust index handle.
- `Filter` owns an opaque Rust filter handle.
- Retrieval, filtering, ranking, persistence, and traces stay in Rust.
- Swift provides Apple-platform API shape, ownership, and error mapping.

## Build

Build the Rust FFI static library before using the SwiftPM package locally:

```bash
cd ../../..
MACOSX_DEPLOYMENT_TARGET=14.0 cargo build -p vectorkit-ffi
cd wrappers/swift/VectorKit
swift test
```

For iOS/macOS app integration, build the XCFramework:

```bash
cd ../../..
rustup target add aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim
scripts/build-xcframework.sh
```

The script writes:

```text
target/apple/VectorKitFFI.xcframework
```

## Usage

```swift
import VectorKit

let index = try VectorIndex(dimension: 3)

try index.upsert(
    document: Document(id: "note-1", metadata: ["source": .string("notes")]),
    chunks: [
        ChunkInput(text: "local private notes", embedding: [1, 0, 0])
    ]
)

let filter = try Filter.equals("source", .string("notes"))
let results = try index.hybridSearch(
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

The local SwiftPM package links `../../../target/debug/libvectorkit_ffi.a`
for development. Release packaging should consume `VectorKitFFI.xcframework`
instead of the debug archive.
