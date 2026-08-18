# RetrievalKit Node base package

> [RetrievalKit](../../../README.md) › SDKs › Node.js base package

```bash
npm install @gungorbasa/retrievalkit@0.1.0
```

`@gungorbasa/retrievalkit` is the approved retrieval-only package name. The
package remains private in the source workspace until release assembly removes
that safety gate. The initial supported runtime is Node.js LTS on macOS arm64.
The assembled v0.1.0 preview is published on npm.

## Quickstart

```ts
import {
  RetrievalDatabaseBuilder,
  timestampMillis
} from "@gungorbasa/retrievalkit";

const builder = new RetrievalDatabaseBuilder({
  corpusId: "notes",
  metric: "cosine"
});
await builder.add([
  {
    id: "welcome",
    text: "Private local retrieval",
    embedding: new Float32Array([0.1, 0.2, 0.3]),
    metadata: { updatedAt: timestampMillis(1_700_000_000_000n) }
  }
]);

const database = await builder.build();
try {
  const results = await database.search({
    mode: "hybrid",
    text: "local search",
    embedding: new Float32Array([0.1, 0.2, 0.3]),
    alpha: 0.6,
    limit: 5
  });
} finally {
  await database.close();
}
```

## Search modes

`search()` is the single retrieval family:

- `{ mode: "vector", embedding }` performs exact vector search.
- `{ mode: "text", text }` calls Rust's direct embedding-free BM25 search.
- `{ mode: "hybrid", text, embedding?, alpha }` uses Rust-owned candidate
  generation and fusion. At `alpha = 0`, the embedding may be omitted.

Builders accept `bm25: { k1, b, stopWords }`. The validated configuration is
owned and persisted by Rust, including when compact snapshots rebuild BM25.

The first document embedding fixes the dimension in Rust. Add documents in
bulk before `build()`. Embeddings always remain caller-provided
`Float32Array`s. Results expose `documentId`; internal chunk IDs and chunk keys
are not part of the common API.

## Lifecycle and persistence

All native operations return promises. Await `close()` in `finally` on every
supported Node.js version. Node.js 24 callers may use `await using` instead.
Operations after close reject with `RetrievalKitLifecycleError`. Persistence
uses `save`, `load`, and read-only `validate`; loaded indexes stay resident
during search.

Metadata supports strings, booleans, integer `bigint`s, floating-point values,
and `timestampMillis`. Plain safe integral numbers become integers; use
`floatingPoint(7)` when an integral-looking value must retain float type.

Do not load this package together with `@gungorbasa/retrievalkit-graph` in one
process. The loader rejects the second aggregate.
