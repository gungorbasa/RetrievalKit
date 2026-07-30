# RetrievalKit Browser Embedding

Provisional, unpublished browser package for local FP32
`sentence-transformers/all-MiniLM-L6-v2` embeddings. It is private while naming
and release approval remain pending.

This package is deliberately separate from RetrievalKit retrieval packages. It
does not import the Node N-API addon, the browser retrieval package, or the Rust
retrieval core. An application owns a dedicated module Worker; model
acquisition, tokenization, ONNX session creation, warmup, and inference all run
there.

## Frozen contract

- source model revision:
  `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
- artifact repository: `gungorbasa/retrievalkit-minilm`
- immutable artifact commit:
  `617ce926c1f9e0289365d3e999474cc28b1645d4`
- `manifest-v1.json` SHA-256:
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`
- FP32 only, at most 256 tokens, masked-mean pooled by the exported graph
- exactly 384 finite `Float32` values with L2 norm within `1e-4`
- WebGPU preferred, with the same verified model falling back to WASM

No production path selects another model, precision, or quantized profile.

## Worker setup

Create a module Worker entry:

```ts
// embedding.worker.ts
import { installBrowserEmbeddingWorker } from
  "@gungorbasa/retrievalkit-browser-embedding/worker";

installBrowserEmbeddingWorker();
```

Then construct the UI-thread client:

```ts
import { BrowserEmbedder } from
  "@gungorbasa/retrievalkit-browser-embedding";

const embedder = await BrowserEmbedder.load({
  worker: () =>
    new Worker(new URL("./embedding.worker.js", import.meta.url), {
      type: "module"
    }),
  execution: "auto"
});

const query = await embedder.embed("local semantic search");
const documents = await embedder.embedBatch(["first", "second"]);
await embedder.close();
```

`execution: "auto"` first requires `navigator.gpu.requestAdapter()` to return
an actual adapter, then creates and warms a WebGPU session with WASM operator
fallback. If no adapter is available or that attempt fails, it creates a
WASM-only session.
`"webgpu"` is strict and does not switch providers; `"wasm"` is deterministic
CPU/WASM execution. The selected provider is exposed as `embedder.provider`.

`embedBatch` performs one dynamic, batch-longest ONNX call and one contiguous
Worker transfer. Requests execute FIFO against the single Worker-owned
session, and returned row views are validated independently. Empty and
whitespace-only inputs are rejected. Inputs are right-truncated to 254 content
tokens before `[CLS]` and terminal `[SEP]` are added, preserving the fixed
256-token ceiling even though the source `tokenizer.json` contains stale
128-token export metadata.

## Acquisition and offline behavior

Acquisition occurs only in `BrowserEmbedder.load` or
`BrowserEmbedder.prefetch`. `embed` and `embedBatch` never download model
files. The cache inventory is closed to six immutable files:

- artifact manifest
- FP32 ONNX model
- `tokenizer.json`
- `tokenizer_config.json`
- `special_tokens_map.json`
- `vocab.txt`

Every HTTPS response and every cache hit is checked for exact byte size and
SHA-256. A completion marker is written last; interrupted or corrupt state is
removed before reuse. A module-level single-flight and the Web Locks API (when
available) share concurrent acquisition across callers and Workers.

`localOnly: true` prohibits model-artifact network requests and requires a
fully verified cache. The module Worker and packaged ONNX Runtime loader/WASM
are application assets, not model artifacts. For startup with no network at
all, serve or service-worker-precache the Worker entry, package JavaScript, and
the `dist/runtime/` assets as well as prefetching the model cache.

```ts
await BrowserEmbedder.prefetch({
  worker: () =>
    new Worker(new URL("./embedding.worker.js", import.meta.url), {
      type: "module"
    })
});
```

## Lifecycle, errors, and cancellation

All public work is asynchronous. Pass an `AbortSignal` through load, prefetch,
embed, or embedBatch options. Cancellation suppresses stale Worker responses;
it does not cancel a shared artifact acquisition needed by another waiter.

`close()` is idempotent, releases the ONNX session, rejects pending calls, and
terminates the owned Worker. Calls after close and Worker crashes fail
deterministically with exported typed errors and stable `RK_EMBEDDING_*` codes.
`Symbol.dispose` and `Symbol.asyncDispose` are also supported.

## Deployment

The package build verifies and copies only the pinned ONNX Runtime Web 1.27.0
standard WASM and WebGPU/asyncify loader/WASM pairs into `dist/runtime/`;
generated runtime and model binaries are not kept in source. The implementation
uses the provider-specific `onnxruntime-web/wasm` and `/webgpu` entrypoints so
explicit WASM does not inherit the WebGPU bundle. Bundle the dedicated Worker
as a static module asset. A typical strict policy needs `worker-src 'self'`,
`script-src 'self'`, and permission for WebAssembly compilation; exact CSP
syntax depends on the host and browser. WebGPU availability and policy still
vary by browser.

The package is Apache-2.0. Package contents include the project `LICENSE` and
`NOTICE`, dependency licenses, ONNX Runtime third-party notices, declarations,
source maps, and the verified runtime assets. It has not been published.

## Development checks

```sh
npm install
npm run check
npm run check:package
```

All default unit tests are offline and use injected stores, fetchers, and
runtimes. A live qualification may inject a local frozen artifact directory
into `EmbeddingWorkerService` and run the actual ORT-WASM boundary without
widening the public client API.
