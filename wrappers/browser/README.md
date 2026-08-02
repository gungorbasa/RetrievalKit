# RetrievalKit for browsers

An additive browser wrapper for RetrievalKit. Retrieval, filtering, ranking,
graph traversal, and generation validation run in Rust/WebAssembly inside a
dedicated Web Worker; the UI thread only performs typed marshaling and receives
results.

The published v0.1.0 identity is `@gungorbasa/retrievalkit-browser`. It does
not import, modify, or bundle the existing Node.js/N-API
wrapper.

Applications that need local MiniLM embeddings may use the independent
`wrappers/browser-embedding` package. The two packages are deliberately not
coupled: this retrieval API continues accepting caller-produced
`Float32Array` values, and no database operation loads or downloads a model.

## Runtime status

The initial adapter reports its capabilities at startup:

```ts
{
  execution: "dedicated-worker",
  performanceTier: "simd128",
  persistence: false,
  threads: false,
  simd: boolean,
  structuredDtos: true,
  bulkFloat32Embeddings: true
}
```

Persistence and threaded WASM are intentionally not claimed by this first
slice. `simd` and `performanceTier` describe the generated adapter actually
loaded by the Worker. `performanceTier` is either `"portable"` or
`"simd128"`.

## Worker setup

The release tarball includes qualified portable and SIMD128 `wasm-bindgen`
artifacts. The application owns a small Worker entry and wires those package
exports into the adapter:

```ts
// retrievalkit.worker.ts
import { installRetrievalKitWorker } from "@gungorbasa/retrievalkit-browser/worker";
import { createAdaptiveGeneratedWasmAdapter } from "@gungorbasa/retrievalkit-browser/adapter";

installRetrievalKitWorker(
  createAdaptiveGeneratedWasmAdapter({
    portable: async () => {
      const generated = await import("@gungorbasa/retrievalkit-browser/wasm/portable");
      await generated.default();
      return generated;
    },
    simd128: async () => {
      const generated = await import("@gungorbasa/retrievalkit-browser/wasm/simd128");
      await generated.default();
      return generated;
    }
  })
);
```

Create the browser client with a dedicated module Worker:

```ts
import { RetrievalKitBrowser } from "@gungorbasa/retrievalkit-browser";

const kit = await RetrievalKitBrowser.create({
  worker: () =>
    new Worker(new URL("./retrievalkit.worker.js", import.meta.url), {
      type: "module"
    })
});
```

The adaptive adapter validates SIMD128 support inside the Worker and loads
exactly one artifact before a database is created. Unsupported browsers load
the portable artifact. Both generated modules implement the narrow
`RetrievalKitWasmAdapter` contract exported from
`@gungorbasa/retrievalkit-browser/adapter`. Tests inject a mock adapter, so the
TypeScript package remains buildable before generated WASM artifacts exist.

## Retrieval

```ts
const builder = kit.retrievalDatabase({
  corpusId: "notes",
  metric: "cosine",
  encoding: "f32"
});

await builder.add([
  {
    id: "note-1",
    text: "A local-first search note",
    embedding: new Float32Array([0.2, 0.4, 0.8])
  }
]);

const database = await builder.build();

const vector = await database.search({
  mode: "vector",
  embedding: new Float32Array([0.2, 0.4, 0.8]),
  limit: 5
});

const text = await database.search({ mode: "text", text: "local search" });

const hybrid = await database.search({
  mode: "hybrid",
  text: "local search",
  embedding: new Float32Array([0.2, 0.4, 0.8]),
  alpha: 0.6
});
```

Documents are flattened into one contiguous `Float32Array` per add operation.
The wrapper transfers an owned copy to the Worker, so the caller's arrays are
not detached. Query embeddings follow the same rule.

## Graph selection and scoped retrieval

`GraphDatabase` exposes `graph` operations only.
`GraphRetrievalDatabase` exposes separate `graph` and `retrieval` views:

```ts
const builder = kit.graphRetrievalDatabase({
  corpusId: "knowledge",
  schema
});

await builder.add(records);
const database = await builder.build();

const selection = await database.graph.query({
  seed: {
    kind: "equals",
    nodeType: "Person",
    field: ["name"],
    values: ["Ada"]
  },
  traverse: [{ relationship: "AUTHORED" }]
});

const results = await database.retrieval.search({
  mode: "hybrid",
  text: "analytical engine",
  embedding: queryEmbedding,
  within: selection
});

await selection.close();
await database.close();
kit.close();
```

Selections are opaque, generation-bound handles. A selection from another
client, or one that has been closed, is rejected before a Worker request.

## Cancellation and type-ahead

Every query accepts an `AbortSignal`. A `supersedeKey` additionally cancels the
previous in-flight request with the same key:

```ts
await database.search(query, {
  signal: controller.signal,
  supersedeKey: "search-box"
});
```

Cancellation is cooperative across the Worker/WASM boundary. It prevents stale
results from being delivered and lets the adapter stop between WASM phases; a
single synchronous WASM call cannot be interrupted mid-instruction.

Calling `close()` immediately makes an object unavailable to new operations.
Repeated closes share one Promise, and post-close operations fail with
`RetrievalKitLifecycleError`.

## Development

```sh
npm install
npm run check
npm run build
```

The package has its own scripts and dependency lockfile. It is deliberately not
added to the existing Node wrapper workspaces. Release construction first runs
`scripts/check-browser-wasm.sh` with an output directory, then passes those
portable and SIMD128 artifacts to
`scripts/release/assemble_browser_package.py`; neither command publishes.
