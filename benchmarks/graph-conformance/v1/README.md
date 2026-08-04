# Generic Graph Cross-Wrapper Conformance V1

> [RetrievalKit](../../../README.md) › Benchmarks › Graph conformance › V1

**Status:** frozen, synthetic, domain-neutral cross-wrapper contract fixture.

`fixture.json` is synthetic, domain-neutral contract data shared by Rust and
language wrappers. It is not customer evidence and contains no customer data.

The fixture intentionally uses the canonical Rust schema, record, metadata,
chunk, and vector JSON shapes. Every wrapper must decode this file without a
wrapper-specific semantic translation layer and produce the checked-in node,
path, projection, filtered exact, and keyword results.

Rust, Swift, Python, TypeScript, and Kotlin graph wrappers consume this exact
V1 file and must match the checked-in assertions before a V2 fixture is
introduced. Browser parity is covered separately by the browser conformance
suite.

## What this proves

- Every wrapper accepts one canonical schema and record model.
- Node, path, projection, exact, and keyword results remain behaviorally equal.
- Wrapper syntax may differ without introducing wrapper-owned semantics.

The fixture is qualification input, not customer evidence or a performance
benchmark. Do not edit expected output to accommodate an implementation bug.
