# Generic Graph Cross-Wrapper Conformance V1

`fixture.json` is synthetic, domain-neutral contract data shared by Rust and
language wrappers. It is not customer evidence and contains no customer data.

The fixture intentionally uses the canonical Rust schema, record, metadata,
chunk, and vector JSON shapes. Every wrapper must decode this file without a
wrapper-specific semantic translation layer and produce the checked-in node,
path, projection, filtered exact, and keyword results.

When the Python graph wrapper is implemented, it must consume this exact V1
file and match the existing Rust and Swift assertions before a V2 fixture is
introduced.
