# retrievalkit-embedding

Optional local text embeddings for RetrievalKit. This crate is separate from
`retrievalkit-core`: constructing an embedder may download a pinned model,
while database initialization, indexing, and search never do.

```rust
use retrievalkit_embedding::{OnnxTextEmbedder, TextEmbedder};

let embedder = OnnxTextEmbedder::builder()
    .runtime_library_path("/app/lib/libonnxruntime.dylib")
    .build()?;
let vector = embedder.embed("fast local retrieval")?;
// Pass `vector` to the unchanged RetrievalKit search API.
# Ok::<(), retrievalkit_embedding::EmbeddingError>(())
```

FP32 is the canonical default profile. FP16 and dynamic signed-INT8 Q8 are
explicit, opt-in model-weight formats. All three profiles return 384 finite
normalized `f32` values, use masked mean pooling, and truncate at 256
WordPiece tokens. Batch inputs pad only to the longest item in the batch.

`EmbeddingProfile::Q8` describes the quantized weights used during ONNX model
inference. It is independent of RetrievalKit's
`VectorEncoding::I8ScalarQuantized`, which controls how already-produced
embeddings are stored and scored in a RetrievalKit database. Selecting Q8
model weights does not select I8 database storage, and selecting I8 database
storage does not change the embedding model profile.

The built-in artifacts are pinned to commit
`617ce926c1f9e0289365d3e999474cc28b1645d4` of
`gungorbasa/retrievalkit-minilm`. The selected model, all common tokenizer
files, and `manifest-v1.json` are verified by exact byte size and SHA-256
before an atomic cache publish. Use `DownloadPolicy::LocalOnly` for offline
applications or `ModelStore::prefetch_all()` to fetch every profile explicitly.

The default CPU session uses up to four intra-operation threads and one
inter-operation thread. Both are overrideable on `OnnxTextEmbedderBuilder`.
The application must bundle the official ONNX Runtime 1.24.3 shared library
and pass its path to the builder, or set
`RETRIEVALKIT_ONNX_RUNTIME_LIBRARY`. The crate uses `ort` 2.0.0-rc.12's
API-24 dynamic-loading boundary and never substitutes a different prebuilt
runtime.

Run the ignored live-download test explicitly:

```bash
RETRIEVALKIT_EMBEDDING_TEST_PROFILE=fp32 \
RETRIEVALKIT_ONNX_RUNTIME_LIBRARY=/app/lib/libonnxruntime.dylib \
cargo test -p retrievalkit-embedding --test published_model --release \
  -- --ignored --nocapture
```

Run the 50-warmup, 750-sample latency matrix from a verified cache:

```bash
RETRIEVALKIT_ONNX_RUNTIME_LIBRARY=/app/lib/libonnxruntime.dylib \
cargo run --release -p retrievalkit-embedding --example benchmark -- fp32
```

Run the frozen cross-provider vectors through RetrievalKit's actual F32/I8
vector, hybrid, BM25, and graph-scoped ranking paths:

```bash
cargo run --release -p retrievalkit-embedding \
  --example qualify_retrieval_policy -- \
  target/embedding-provider-conformance-input.json \
  target/embedding-provider-vectors/cpu-fp32.json \
  target/embedding-compute-vectors/direct-coreml-all-fp32.json
```

This example is qualification tooling only. It does not execute embedding
inference inside `retrievalkit-core` or change any public retrieval API.
