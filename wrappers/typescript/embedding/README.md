# RetrievalKit Embedding for Node.js

`@gungorbasa/retrievalkit-embedding` is RetrievalKit's optional Node.js
embedding package. It produces local FP32 MiniLM embeddings through the
official ONNX Runtime 1.24.3 without adding embedding dependencies to the base
retrieval package.

The production contract is fixed: at most 256 tokens and exactly 384 finite,
L2-normalized `Float32Array` values. RetrievalKit databases continue to accept
F32 vectors publicly and may store them with the independent
`I8ScalarQuantized` database encoding.

```ts
import { OnnxEmbedder } from "@gungorbasa/retrievalkit-embedding";

await using embedder = await OnnxEmbedder.load({
  runtimeLibraryPath: "/application/lib/libonnxruntime.1.24.3.dylib"
});
const vector = await embedder.embed("local semantic retrieval");
```

`load()` and `prefetch()` are the only APIs that may acquire verified model
artifacts. `embed()` and `embedBatch()` use an already-loaded session and do not
perform network or model-cache access. Set `localOnly: true` to refuse model
downloads.

The application normally supplies the official runtime with
`runtimeLibraryPath` or `RETRIEVALKIT_ONNX_RUNTIME_LIBRARY`. A repository-local
packaging build can opt into copying a verified runtime:

```sh
RETRIEVALKIT_BUNDLE_ONNX_RUNTIME=1 \
RETRIEVALKIT_ONNX_RUNTIME_LIBRARY=/path/libonnxruntime.1.24.3.dylib \
npm run build:native
```

Only the qualified runtime with exact size 27,724,968 bytes and SHA-256
`b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729`
is accepted for package-local discovery. The binary is not stored in this
repository.

The v0.1.0 preview is published as
`@gungorbasa/retrievalkit-embedding`.
