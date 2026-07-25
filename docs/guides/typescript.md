# TypeScript/Node Guide

The repository contains two provisional, source-only packages for Node.js LTS
on macOS arm64:

- `retrievalkit-node-local` for retrieval without graph state.
- `retrievalkit-node-graph-local` for graph-only and combined graph/retrieval
  applications.

Install or load exactly one package in a process. The graph package already
contains retrieval capability, and its loader rejects mixing native aggregates.
Browser and WebAssembly builds are not part of this target.

## Retrieval-only quickstart

Embeddings are supplied by the application as `Float32Array`. The first
document fixes dimension in Rust; callers do not configure it separately.

```ts
import {
  RetrievalDatabaseBuilder,
  timestampMillis
} from "retrievalkit-node-local";

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
import { GraphRetrievalDatabaseBuilder } from
  "retrievalkit-node-graph-local";

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

The initial target requires macOS arm64, Node.js 20 or newer, and Rust `cargo`.
The preflight prints detected values and exits before compilation when they do
not match.

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
