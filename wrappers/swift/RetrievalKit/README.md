# RetrievalKit Swift

Swift wrapper for RetrievalKit's Rust retrieval core.

For the Project Apollo walkthrough and guidance on choosing hybrid, semantic,
graph scope, and metadata filters, start with the canonical
[Swift guide](../../../docs/guides/swift.md).

The wrapper is intentionally thin:

- `VectorIndex` owns an opaque Rust index handle.
- `Filter` owns an opaque Rust filter handle.
- Retrieval, filtering, ranking, persistence, and traces stay in Rust.
- Swift provides Apple-platform API shape, ownership, and error mapping.

## Build And Test

The default package manifest consumes the built XCFramework at:

```text
target/apple/RetrievalKitFFI.xcframework
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
target/apple/RetrievalKitFFI.xcframework
```

`Package.local.swift` exists for low-level development against
`target/debug/libretrievalkit_ffi.a`, but release validation should use the
default `Package.swift` and the XCFramework.

## Retrieval Database

Use `RetrievalDatabase` when an application needs semantic or hybrid retrieval
without graph traversal. Embeddings are produced by the caller and paired
directly with searchable documents.

```swift
import RetrievalKit

let builder = RetrievalDatabase.Builder(
    corpusID: "knowledge"
)
try await builder.upsert(
    Document(id: "note-42", text: "native retrieval"),
    embedding: embedding
)
let database = try await builder.build()

let semantic = try await database.search(
    embedding: queryEmbedding,
    limit: 10
)
let lexical = try await database.search(
    text: "native retrieval",
    limit: 10
)
let hybrid = try await database.search(
    text: "native retrieval",
    embedding: queryEmbedding,
    alpha: 0.6,
    limit: 10
)
```

The first document embedding fixes the database dimension. Every retrieval
database builds exact-vector and BM25 state. `alpha` is query-time: `1` is
vector-only, `0` is BM25-only, and intermediate values are hybrid.

Run the focused example with:

```bash
swift run --package-path wrappers/swift/RetrievalKit RetrievalKitRetrievalQuickstart
```

## Compatibility API

The explicit-dimension `RetrievalConfiguration`, `RecordInput`, `Chunk`, keyed
embedding maps, and `.retrieval` query namespace remain available while
preview clients migrate.

`VectorIndex` remains temporarily available for existing examples and migration.
New code should use `RetrievalDatabase` so its enabled capability is explicit at
the call site.

```swift
import RetrievalKit
import RetrievalKitIngest

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

- Create/load/save `RetrievalDatabase` and the compatibility `VectorIndex`.
- Upsert and delete documents.
- Separate `RetrievalKitIngest` product with shared Rust-backed fixed and
  sentence-aware text chunking.
- Exact vector search.
- BM25 keyword search.
- Hybrid vector + keyword search.
- Typed metadata values.
- Typed filter builders: equals, not-equals, exists, range, in-values, all, any.
- Structured Swift errors mapped from Rust/FFI failures.

`VectorIndex` is the ownership boundary for one native index handle. Exact,
keyword, and hybrid searches acquire shared read access and execute in detached
tasks, so multiple calls on the same actor can genuinely run in parallel.
Upsert, delete, save, and compaction acquire writer-preferring exclusive access:
they wait for active searches, and later searches wait behind them.

Call every operation with `await` from outside the actor. `Filter` is an
immutable `Sendable` value; each search builds and frees its own temporary Rust
filter handle, status, and output buffers. The actor is retained until detached
native work finishes, so its Rust handle cannot be freed early.

```swift
async let semantic = index.search(embedding: semanticQuery)
async let lexical = index.keywordSearch(text: "exact name")
let (semanticHits, lexicalHits) = try await (semantic, lexical)
```

## Persistence Safety

`save(to:)` publishes a complete immutable snapshot. RetrievalKit writes and syncs
the new generation before atomically switching `manifest.json`, so an
interrupted save leaves the previously published generation loadable. A later
successful save removes abandoned and superseded generations. Existing V1
root-file indexes remain readable and migrate to snapshots on their next save.

Only one writer may save a given directory at a time. RetrievalKit uses an
OS-released lock, so a process crash does not leave the directory permanently
locked; a competing save fails without changing the published generation.

Applications should treat the index directory as RetrievalKit-owned and must not
edit `.snapshots` or `manifest.json` directly.

```swift
import Foundation
import RetrievalKit

let index = try VectorIndex(dimension: 384)
let applicationSupportURL = FileManager.default.urls(
    for: .applicationSupportDirectory,
    in: .userDomainMask
)[0]
let indexURL = applicationSupportURL.appendingPathComponent("search-index")
try await index.save(to: indexURL)

// On the next app launch:
let loadedIndex = try VectorIndex.load(from: indexURL)
```

New indexes default to compact I8 scalar-quantized storage. Hybrid search
defaults to 50 vector candidates, 50 keyword candidates, and `alpha = 0.6`.
Pass `encoding: .f32`, a different `alpha`, or explicit candidate options when
a different tradeoff is required.

New saves use a checksummed V3 manifest. Validate a stored index without
retaining it for search:

```swift
do {
    try VectorIndex.validate(at: indexURL)
} catch RetrievalKitError.corruptIndex(let message) {
    print(message) // restore a known-good copy or rebuild the index
}
```

V1 and V2 indexes remain readable without checksums. Their next save publishes
a V3 generation. Integrity failures are surfaced as
`RetrievalKitError.corruptIndex`.

Filesystem failures surface as `RetrievalKitError.core` values whose message
contains the failed operation, path, operating-system cause, and a recovery
hint.

## Compaction

Updates and deletes create tombstones so search results change immediately
without rewriting the full index. Reclaim their memory before saving a smaller
snapshot:

```swift
if await index.tombstonedChunkCount > 0 {
    let report = try await index.compact()
    print(
        "Removed \(report.chunksRemoved) chunks; "
            + "reclaimed about \(report.estimatedBytesReclaimed) bytes"
    )
    try await index.save(to: indexURL)
}
```

`compact()` is an inexpensive no-op when there are no tombstones. It preserves
all active chunk IDs and never reuses removed IDs. The byte report estimates
in-memory payload savings; call `save(to:)` afterward to publish a compacted
disk snapshot.

Compaction holds exclusive index access and temporarily retains both the current
and replacement structures. Calls on that `VectorIndex` wait until it finishes.
Run it during a maintenance window and leave memory headroom, especially near
the 50K-chunk V1 ceiling. The estimate reports retained payload before and
after compaction; it is not a peak-RSS measurement.

The source package currently expects the XCFramework to be built in this
repository before `swift build` or `swift test`. A public binary release should
publish `RetrievalKitFFI.xcframework` and switch the binary target to a URL plus
checksum for tagged distribution.

`TextChunker` limits and overlap are measured in Unicode characters. Returned
`startByte` and `endByte` values are UTF-8 byte offsets into the original text.
Sentence mode prefers sentence endings, then whitespace, and falls back to the
hard character limit. Token-aware chunking remains the responsibility of an
embedding-model-specific integration layer.
