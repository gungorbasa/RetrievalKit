# RetrievalKit Node base package

`@gungorbasa/retrievalkit` is the approved retrieval-only package name. The
package remains private in the source workspace until release assembly removes
that safety gate. The initial supported runtime is Node.js LTS on macOS arm64.

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

await using database = await builder.build();
const results = await database.search({
  mode: "hybrid",
  text: "local search",
  embedding: new Float32Array([0.1, 0.2, 0.3]),
  alpha: 0.6,
  limit: 5
});
```

`search()` is the single retrieval family:

- `{ mode: "vector", embedding }` performs exact vector search.
- `{ mode: "text", text }` performs BM25-only search (`alpha = 0`).
- `{ mode: "hybrid", text, embedding?, alpha }` uses Rust-owned candidate
  generation and fusion. At `alpha = 0`, the embedding may be omitted.

The first document embedding fixes the dimension in Rust. Add documents in
bulk before `build()`. Embeddings always remain caller-provided
`Float32Array`s. Results expose `documentId`; internal chunk IDs and chunk keys
are not part of the common API.

All native operations return promises. Await `close()` or use `await using` to
release native state after in-flight work. Operations after close reject with
`RetrievalKitLifecycleError`. Persistence uses `save`, `load`, and read-only
`validate`; loaded indexes stay resident during search.

Metadata supports strings, booleans, integer `bigint`s, floating-point values,
and `timestampMillis`. Plain safe integral numbers become integers; use
`floatingPoint(7)` when an integral-looking value must retain float type.

Do not load this package together with `@gungorbasa/retrievalkit-graph` in one
process. The loader rejects the second aggregate.
