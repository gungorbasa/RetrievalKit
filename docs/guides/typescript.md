# TypeScript/Node Guide

The selected npm identities are:

- `@gungorbasa/retrievalkit` for retrieval without graph state.
- `@gungorbasa/retrievalkit-graph` for graph-only and combined graph/retrieval
  applications.
- `@gungorbasa/retrievalkit-embedding` for the independent Node local
  embedding provider.
- `@gungorbasa/retrievalkit-browser` for Worker-owned browser retrieval, graph,
  and graph-scoped retrieval with packaged portable and SIMD128 WASM tiers.
- `@gungorbasa/retrievalkit-browser-embedding` for the independent browser
  embedding Worker.

The equivalent unscoped base name was rejected by npm as too similar to an
existing package, so every Node package uses the release owner's public scope.
The four previously approved npm names have bootstrap-only placeholder versions
and GitHub trusted publishers configured. The newly added
`@gungorbasa/retrievalkit-browser` registry record and publisher remain
pending. Those placeholders are not SDK releases; v0.1.0 remains unpublished.
From source or after the real release, install or load exactly one
retrieval package in a process. The independent embedding package may accompany
it. The graph package already contains retrieval capability, and its loader
rejects mixing retrieval native aggregates. A separate
capability-separated browser/WebAssembly runtime is implemented under
`wrappers/browser`, with an independent embedding provider under
`wrappers/browser-embedding`. Neither browser package is a Node.js fallback.

## Installation status

The eventual shortest install will be:

```bash
# PENDING — v0.1.0 is unpublished; these commands describe the approved release.
npm install @gungorbasa/retrievalkit-graph
# Optional independent local embedding provider:
npm install @gungorbasa/retrievalkit-embedding

# Browser applications use two independent Worker packages:
npm install @gungorbasa/retrievalkit-browser
npm install @gungorbasa/retrievalkit-browser-embedding
```

Choose `@gungorbasa/retrievalkit-graph` when relationships matter; it already
includes retrieval. Choose `@gungorbasa/retrievalkit` for a flat corpus.
Install exactly one retrieval native aggregate in a process; the independent
embedding aggregate may accompany it.

Browser retrieval combines base, graph-only, and graph-scoped retrieval in one
Worker/WASM package rather than separate base and graph npm packages. See the
[`wrappers/browser` guide](../../wrappers/browser/README.md) for the packaged
WASM tier imports and Worker entrypoint.

The available route is the repository source build:

```bash
cd wrappers/typescript
npm ci
npm run preflight
npm run build
node graph/examples/graph-retrieval.mjs
```

The initial native target is macOS arm64 with Node.js 22.13+ LTS or Node.js 24
LTS. Browser retrieval and embedding are separate v0.1.0 packages with their
own Worker/runtime qualification. Windows, Linux, and other native
architectures are not claimed. Registry names and bootstrap placeholders are
not SDK availability claims.

## Retrieval-only quickstart

Embeddings are supplied by the application as `Float32Array`. The first
document fixes dimension in Rust; callers do not configure it separately.

```ts
import {
  RetrievalDatabaseBuilder,
  timestampMillis
} from "@gungorbasa/retrievalkit";

const builder = new RetrievalDatabaseBuilder({
  corpusId: "apollo",
  metric: "cosine"
});

await builder.add([
  {
    id: "decision-swift",
    text: "Apollo chose Swift for native platform integration.",
    embedding: new Float32Array([1, 0, 0]),
    metadata: {
      project: "apollo",
      updatedAt: timestampMillis(1_700_000_000_000n)
    }
  },
  {
    id: "unrelated",
    text: "A separate prototype mentioned another language.",
    embedding: new Float32Array([0, 1, 0]),
    metadata: { project: "other" }
  }
]);

const database = await builder.build();
try {
  const hits = await database.search({
    mode: "hybrid",
    text: "Why did we choose Swift?",
    embedding: new Float32Array([1, 0, 0]),
    alpha: 0.6,
    where: { kind: "equals", field: "project", value: "apollo" },
    limit: 5
  });

  console.log(hits[0]?.documentId);
} finally {
  await database.close();
}
```

`search()` is one discriminated family:

- `{ mode: "vector", embedding }` performs exact vector search.
- `{ mode: "text", text }` performs BM25-only search.
- `{ mode: "hybrid", text, embedding, alpha }` combines both in Rust.

Result metadata uses ordinary typed values. Use JavaScript `bigint` for signed
64-bit integers and `timestampMillis(...)` for timestamps; values never pass
through a lossy `number` conversion.

## Graph-only and graph-scoped retrieval

Use `GraphDatabaseBuilder` when the application needs only traversal and stable
candidate projection. Use `GraphRetrievalDatabaseBuilder` when graph scope
should feed the same retrieval ranker. Both are exported by the graph package.

```ts
import {
  GraphRetrievalDatabaseBuilder
} from "@gungorbasa/retrievalkit-graph";

const builder = new GraphRetrievalDatabaseBuilder({
  corpusId: "apollo",
  schema: {
    recordNodes: [
      {
        recordType: "Decision",
        nodeType: "Decision",
        queryableFields: [["project"]]
      }
    ]
  }
});

await builder.add([
  {
    id: "decision-swift",
    type: "Decision",
    fields: { project: "apollo" },
    content: "Apollo chose Swift for native platform integration.",
    retrieval: {
      kind: "content",
      embedding: new Float32Array([1, 0, 0])
    }
  }
]);

const database = await builder.build();
try {
  const selection = await database.graph.query({
    seed: {
      kind: "equals",
      nodeType: "Decision",
      field: ["project"],
      values: ["apollo"]
    }
  });
  try {
    const projection = await database.graph.projectCandidates(selection);
    const hits = await database.retrieval.search({
      mode: "hybrid",
      text: "native integration",
      embedding: new Float32Array([1, 0, 0]),
      alpha: 0.6,
      within: selection
    });
  } finally {
    await selection.close();
  }
} finally {
  await database.close();
}
```

Selections are opaque and generation-bound. Projection, metadata filtering,
stale checks, lexical ordering, complete graph paths, and edge provenance come
from Rust as typed values.

## Build and verify from source

The initial target requires macOS arm64, Node.js 22.13+ LTS or Node.js 24 LTS,
and Rust `cargo`. Node.js 24 LTS is recommended for a new setup. The preflight
rejects Current, odd-numbered, and end-of-life Node.js releases even when their
major version is numerically newer, then prints an actionable LTS recovery
message before compilation.

```bash
cd wrappers/typescript
npm ci
npm run preflight
npm run build
npm run typecheck
npm run lint
npm test
npm run verify:contents
npm run smoke:install
```

These commands build the base and graph native aggregates separately, inspect
their packed contents, and install the packages in isolated temporary
applications. They do not publish to npm. See
[`wrappers/typescript/README.md`](../../wrappers/typescript/README.md) for
lifecycle and native-build details.

The three runnable source examples are:

```bash
node base/examples/retrieval.mjs
node graph/examples/graph-only.mjs
node graph/examples/graph-retrieval.mjs
```
