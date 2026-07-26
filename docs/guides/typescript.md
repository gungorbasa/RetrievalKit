# TypeScript/Node Guide

The selected npm identities are:

- `@gungorbasa/retrievalkit` for retrieval without graph state.
- `@gungorbasa/retrievalkit-graph` for graph-only and combined graph/retrieval
  applications.

The equivalent unscoped base name was rejected by npm as too similar to an
existing package, so both Node packages use the release owner's public scope.
Neither scoped package is published yet. From source or after publication,
install or load exactly one package in a process. The graph package already
contains retrieval capability, and its loader rejects mixing native aggregates.
Browser and WebAssembly builds are not part of this target.

## Installation status

The eventual shortest install will be:

```bash
# PENDING — npm bootstrap and publication are not complete.
npm install @gungorbasa/retrievalkit-graph
```

Choose `@gungorbasa/retrievalkit-graph` when relationships matter; it already
includes retrieval. Choose `@gungorbasa/retrievalkit` for a flat corpus.
Install exactly one native aggregate in a process.

The available route is the repository source build:

```bash
cd wrappers/typescript
npm ci
npm run preflight
npm run build
node graph/examples/graph-retrieval.mjs
```

The initial qualified target is macOS arm64 with Node.js 22.13+ LTS or Node.js
24 LTS. Browser, WebAssembly, Windows, Linux, and other architectures are not
claimed. The selected names are package identities, not npm availability
claims.

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

await using database = await builder.build();

const hits = await database.search({
  mode: "hybrid",
  text: "Why did we choose Swift?",
  embedding: new Float32Array([1, 0, 0]),
  alpha: 0.6,
  where: { kind: "equals", field: "project", value: "apollo" },
  limit: 5
});

console.log(hits[0]?.documentId);
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

await using database = await builder.build();
await using selection = await database.graph.query({
  seed: {
    kind: "equals",
    nodeType: "Decision",
    field: ["project"],
    values: ["apollo"]
  }
});

const projection = await database.graph.projectCandidates(selection);
const hits = await database.retrieval.search({
  mode: "hybrid",
  text: "native integration",
  embedding: new Float32Array([1, 0, 0]),
  alpha: 0.6,
  within: selection
});
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
