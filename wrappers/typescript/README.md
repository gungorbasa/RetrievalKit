# RetrievalKit for Node.js

This directory contains two repository-local, provisional packages:

- `retrievalkit-node-local`: retrieval-only native aggregate.
- `retrievalkit-node-graph-local`: graph-only and combined graph/retrieval
  native aggregate.

The initial supported target is Node.js LTS on macOS arm64. Browser, WebAssembly,
other operating systems, and public npm distribution are not claimed. Package
names remain provisional until naming clearance.

## Build and verify

From this directory:

```bash
npm install
npm run build
npm run typecheck
npm run lint
npm test
npm run verify:contents
npm run smoke:install
```

`build:native` compiles the same napi-rs crate twice. The base build has no
`retrievalkit-graph` dependency; the graph build enables the off-by-default
`graph` feature. The resulting `.node` files are copied into their respective
packages. Do not import both packages in one process. Both loaders enforce this
rule with a process-global aggregate guard.

All filesystem, graph construction, graph traversal, persistence, and search
work runs on N-API worker tasks. `close()` is asynchronous: await it to release
native state deterministically after any in-flight work. `Symbol.asyncDispose`
awaits the same operation. `Symbol.dispose` initiates release for synchronous
`using` blocks; prefer `await using` when the runtime supports it.

Search, filters, graph queries, candidate projection, and results cross N-API as
typed values rather than JSON. Embeddings use `Float32Array`. Signed 64-bit
record and metadata integers cross the boundary as decimal typed fields and are
presented as JavaScript `bigint`, so values above `Number.MAX_SAFE_INTEGER`
never round silently.

See [base/README.md](base/README.md) and [graph/README.md](graph/README.md) for
API examples and lifecycle details.
