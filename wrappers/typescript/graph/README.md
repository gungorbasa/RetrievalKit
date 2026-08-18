# RetrievalKit Node graph aggregate

> [RetrievalKit](../../../README.md) › SDKs › Node.js graph aggregate

```bash
npm install @gungorbasa/retrievalkit-graph@0.1.0
```

`@gungorbasa/retrievalkit-graph` is the approved graph-capable aggregate
package name. The package remains private in the source workspace until release
assembly removes that safety gate. The initial supported runtime is Node.js LTS
on macOS arm64.
The assembled v0.1.0 preview is published on npm.

## Choose a database

It contains two products:

- `GraphDatabase`: canonical corpus plus graph; no vectors or BM25.
- `GraphRetrievalDatabase`: one canonical corpus with graph and retrieval.

`GraphRetrievalDatabaseBuilder` accepts `bm25: { k1, b, stopWords }`; Rust
applies it to unscoped and graph-scoped BM25 and preserves it across reloads.

## Quickstart

Builders use ordinary records. `GraphDatabaseBuilder.add()` accepts graph-only
records. `GraphRetrievalDatabaseBuilder.add()` optionally pairs record content
with one embedding or accepts embedded documents. Rust derives all hidden
chunk identities and infers dimension from the first embedding.

```ts
const builder = new GraphRetrievalDatabaseBuilder({
  corpusId: "topics",
  schema: {
    recordNodes: [
      { recordType: "Topic", nodeType: "Topic", queryableFields: [["title"]] }
    ]
  }
});
await builder.add([
  {
    id: "local",
    type: "Topic",
    fields: { title: "Local" },
    content: "Local retrieval",
    retrieval: { kind: "content", embedding: new Float32Array([1, 0]) }
  }
]);
const database = await builder.build();
try {
  const selection = await database.graph.query({
    seed: {
      kind: "equals",
      nodeType: "Topic",
      field: ["title"],
      values: ["Local"]
    }
  });
  try {
    const results = await database.retrieval.search({
      mode: "text",
      text: "retrieval",
      within: selection
    });
  } finally {
    await selection.close();
  }
} finally {
  await database.close();
}
```

## Results and lifecycle

Graph queries return typed matches, complete typed path edges and provenance,
truncation state, and trace counts. Selections are opaque generation-bound
native objects. `projectCandidates()` is the only public operation that
materializes stable `(recordId, chunkKey)` identities; it preserves Rust-owned
corpus/generation validation, metadata filtering, lexical ordering, and counts
before and after filtering.

All potentially blocking work returns promises. Await `close()` in `finally` on
builders, databases, and selections. Node.js 24 callers may use `await using`
instead. Errors are mapped to typed classes while retaining actionable Rust
messages. Integer fields use `bigint` for exact signed 64-bit transport.

Do not load this package together with `@gungorbasa/retrievalkit` in one process.
The loader rejects the second aggregate.
