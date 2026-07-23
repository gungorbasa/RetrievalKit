# Contributing to RetrievalKit

RetrievalKit is preparing its first preview distribution. Bug reports,
reproduction cases, documentation corrections, and focused design discussion
are welcome.

## Before opening a change

1. Read [AGENTS.md](AGENTS.md), the
   [product specification](docs/product/retrievalkit-product-spec.md), and any
   language-specific guidance under `docs/agents/`.
2. Open an issue for public API, persistence-format, architecture, or product
   scope changes before implementing them.
3. Keep changes small, deterministic, and inside the V1 scope of exact and
   hybrid local retrieval for fewer than 50K chunks.

## Verification

Run the checks relevant to the change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python3 benchmarks/publication/validate_readme.py --repo .
python3 scripts/release/validate_release.py --repo .
```

Python changes should also run `scripts/check-python-wrapper.sh` and
`scripts/check-python-graph-wrapper.sh`. Swift or FFI changes should run
`scripts/verify-swift-graph-wrapper.sh` on a supported Mac.

## Legal status

The repository does not yet contain an owner-approved root license. Do not
assume that source availability grants redistribution rights. Until the owner
adds a license and contribution terms, discuss any non-trivial code
contribution with the maintainer before submitting it. Never add or change a
license on the project's behalf.

By submitting a change, you confirm that you have the right to submit its
contents. A future contribution policy may require additional confirmation
before a public release.
