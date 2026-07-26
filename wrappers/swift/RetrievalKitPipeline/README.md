# RetrievalKit Pipeline

`RetrievalKitPipeline` is the optional high-level Swift package that composes:

- `RetrievalKit` for shared Rust text chunking, indexing, persistence,
  filtering, and hybrid retrieval.
- `EmbeddingKit` for provider-neutral batch and query embeddings.

The lower-level packages remain independently usable.

## Usage

Run the complete deterministic example from the repository root:

```bash
swift run --package-path wrappers/swift/RetrievalKitPipeline retrievalkit-pipeline-example
```

```swift
import EmbeddingKit
import RetrievalKit
import RetrievalKitPipeline

let index = try VectorIndex(dimension: embedder.modelInfo.dimension)
let pipeline = Pipeline(index: index, embedder: embedder)

try await pipeline.add(
    document: Document(
        id: "note-42",
        text: noteText,
        metadata: ["source": .string("notes")]
    )
)

let hits = try await pipeline.search("pricing decisions", topK: 5)
```

The pipeline creates every chunk embedding and validates all dimensions before
calling `VectorIndex.upsert`. An embedding failure therefore leaves the
previous indexed version of the document unchanged. Empty documents are
rejected rather than interpreted as deletion.

When an embedder exposes both `tokenCounter` and `modelInfo.maxInputTokens`, the
default pipeline recursively subdivides Rust-produced chunks until every chunk
fits the model's exact token budget. Character counts remain the fallback for
providers that do not expose their tokenizer.

Generated chunks include namespaced metadata for their position and source
range:

- `retrievalkit.chunk.index`
- `retrievalkit.chunk.start_byte`
- `retrievalkit.chunk.end_byte`

## Custom Chunking

Applications can replace the built-in Rust chunker by implementing
`DocumentChunker`, which is owned and validated by `RetrievalKitPipeline`:

```swift
import RetrievalKit

struct MarkdownChunker: DocumentChunker {
    func chunks(for text: String) throws -> [TextChunk] {
        // Preserve headings, code blocks, and application-specific boundaries.
    }
}

let pipeline = Pipeline(
    index: index,
    embedder: embedder,
    chunker: MarkdownChunker()
)
```

Custom chunkers return the same `TextChunk` values, including UTF-8 byte ranges,
so downstream metadata and indexing behavior remain consistent. Pipeline
rejects empty chunks, invalid or non-character-boundary ranges, mismatched
source text, and out-of-order chunks before calling the embedding provider.
