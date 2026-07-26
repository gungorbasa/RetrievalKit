# RetrievalKit Node graph aggregate

`retrievalkit-graph` is the approved graph-capable aggregate package name. The
package remains private in the source workspace until release assembly removes
that safety gate. The initial supported runtime is Node.js LTS on macOS arm64.

It contains two products:

- `GraphDatabase`: canonical corpus plus graph; no vectors or BM25.
- `GraphRetrievalDatabase`: one canonical corpus with graph and retrieval.

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
await using database = await builder.build();
await using selection = await database.graph.query({
  seed: {
    kind: "equals",
    nodeType: "Topic",
    field: ["title"],
    values: ["Local"]
  }
});
const results = await database.retrieval.search({
  mode: "text",
  text: "retrieval",
  within: selection
});
```

Graph queries return typed matches, complete typed path edges and provenance,
truncation state, and trace counts. Selections are opaque generation-bound
native objects. `projectCandidates()` is the only public operation that
materializes stable `(recordId, chunkKey)` identities; it preserves Rust-owned
corpus/generation validation, metadata filtering, lexical ordering, and counts
before and after filtering.

All potentially blocking work returns promises. Await `close()` on builders,
databases, and selections, or use `await using`. Errors are mapped to typed
classes while retaining actionable Rust messages. Integer fields use `bigint`
for exact signed 64-bit transport.

Do not load this package together with `retrievalkit` in one process.
The loader rejects the second aggregate.
