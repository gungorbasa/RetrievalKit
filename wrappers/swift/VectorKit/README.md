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

For release-packaging validation across supported arm64 Apple slices:

```bash
cd ../../..
scripts/verify-swift-wrapper.sh
```

To build only the local macOS arm64 slice during quick development:

```bash
cd ../../..
scripts/build-xcframework.sh --macos-only
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
import VectorKitIngest

let index = try VectorIndex(dimension: 3)

let chunker = try TextChunker(
    strategy: .sentence,
    maxCharacters: 500,
    overlapCharacters: 50
)
let textChunks = try chunker.chunks(for: documentText)

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
- Separate `VectorKitIngest` product with shared Rust-backed fixed and
  sentence-aware text chunking.
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

`TextChunker` limits and overlap are measured in Unicode characters. Returned
`startByte` and `endByte` values are UTF-8 byte offsets into the original text.
Sentence mode prefers sentence endings, then whitespace, and falls back to the
hard character limit. Token-aware chunking remains the responsibility of an
embedding-model-specific integration layer.
