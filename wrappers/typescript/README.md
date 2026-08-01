# RetrievalKit for Node.js

This directory contains three owner-approved packages:

- `@gungorbasa/retrievalkit`: retrieval-only native aggregate.
- `@gungorbasa/retrievalkit-graph`: graph-only and combined graph/retrieval
  native aggregate.
- `@gungorbasa/retrievalkit-embedding`: independent local FP32 MiniLM
  embedding provider.

The initial implemented native target is Node.js LTS on macOS arm64. The
separate browser/WebAssembly runtime and browser embedding provider live at
`wrappers/browser` and `wrappers/browser-embedding`; they do not load these
N-API packages. Browser embedding joins the v0.1.0 release inventory; browser
retrieval remains unpublished. Other native operating systems and
public npm distribution are not claimed. All checked-in Node package manifests
remain private until closed release assembly.

## Build and verify

The initial target requires macOS arm64, Node.js 22.13+ LTS or Node.js 24 LTS,
and Rust `cargo`. Node.js 24 LTS is recommended for a new setup. Current,
odd-numbered, and end-of-life Node.js releases are rejected even when their
major version is numerically newer. From this directory:

```bash
npm ci
npm run preflight
npm run build
npm run typecheck
npm run lint
npm test
npm run verify:contents
npm run smoke:install
```

`preflight` prints the detected Node.js, Rust, and host values and exits before
compilation when a requirement is not met. `build:native` invokes the same
preflight, so it cannot accidentally bypass the platform check.
Supported LTS ranges are centralized in `scripts/node-support.mjs`; its tests
also require every package's `engines.node` declaration to match.

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

## Assemble npm release tarballs

The checked-in package names remain private placeholders to prevent accidental
publication. Release assembly therefore requires three names approved by the
owner; it never guesses or reserves names:

```bash
python3 ../../scripts/release/assemble_node_packages.py \
  --base-name @gungorbasa/retrievalkit \
  --graph-name @gungorbasa/retrievalkit-graph \
  --embedding-name @gungorbasa/retrievalkit-embedding \
  --names-approved \
  --version 0.1.0 \
  --output ../../dist/release/node
```

The assembler builds all three native aggregates and TypeScript distributions,
checks the graph-free base dependency tree and Mach-O architecture, rewrites
only staged package metadata, and runs `npm pack`. It emits three macOS arm64
tarballs plus `inventory.json`, `SHA256SUMS`, and `SHA512SUMS`. The source
packages retain `"private": true`; only the verified staged tarballs remove the
publication blocker.

`--names-approved` records the explicit owner assertion for these three fixed
identities. The assembler rejects alternative names and fails before changing
staged metadata when the assertion is absent.

Run the deterministic package-content test after building the native addons:

```bash
python3 ../../scripts/release/test_assemble_node_packages.py
```

Assembly does not publish. npm account ownership of both selected names and a
trusted-publisher configuration are external release prerequisites. The
inventory marks the tarballs `artifactReady` after inspection while keeping
`publicationReady` false until that external upload authorization exists.
