# AGENTS.md

Canonical guidance for Codex, Claude, Cursor, and other coding agents working in this repository.

Keep this file focused on repo-wide rules. Put language-specific instructions in `docs/agents/` and read only the files relevant to the change.

## Agent Setup

- Codex: read this root `AGENTS.md`.
- Claude: read `CLAUDE.md`, which points back to this file.
- Cursor: read `.cursor/rules/vectorkit.mdc`, which points back to this file.
- Rust changes: also read `docs/agents/rust.md`.

When changing repository guidance, update this file first. Keep tool-specific files as small compatibility entrypoints unless a tool genuinely needs different syntax.

## Product Direction

VectorKit is a local-first retrieval SDK for mobile and desktop apps. The first target is an iOS/macOS SDK with a Rust retrieval core and a Swift wrapper.

The current V1 direction is:

- Small local indexes: fewer than 50K chunks.
- Primary retrieval engine: exact vector search.
- Retrieval modes: exact vector search, BM25 keyword search, and hybrid ranking.
- Core priorities: correctness, speed, filtering, persistence, and Swift/iOS integration.

Do not add HNSW, ANN indexing, server mode, sync, dashboards, or distributed database features unless the product spec is updated first. HNSW research exists in `docs/research/`, but it is deferred until exact/hybrid retrieval is polished and benchmarked.

Use `docs/product/vectorkit-product-spec.md` as the implementation source of truth.

## Engineering Principles

- Prefer simple, explicit designs over broad abstractions.
- Optimize for correctness first, then measured performance.
- Keep public APIs small, stable, and easy to explain.
- Apply SOLID principles pragmatically. Do not add abstractions that do not reduce real complexity.
- Keep domain boundaries clear: retrieval, storage, filtering, ranking, persistence, and language bindings should not be tangled together.
- Avoid speculative features. Build what V1 needs and leave clear extension points where future wrappers or engines are likely.
- Favor deterministic behavior for indexing, searching, ranking, and tests.
- Expose enough trace/debug data to explain retrieval results.
- Treat deleted, outdated, filtered, or dimension-mismatched chunks as correctness failures, not edge cases.

## Performance Expectations

The retrieval path should be designed for low latency on local devices.

- Avoid JSON parsing, SQLite queries, network calls, and avoidable heap allocation on the hot query path.
- Keep index data loaded before search.
- Separate embedding latency from retrieval latency in benchmarks.
- Use contiguous memory layouts for vector data where practical.
- Prefer direct lookup by internal numeric IDs for hot-path metadata needed by ranking or display.
- Use benchmarks before introducing complex optimizations.
- Do not trade correctness for speed unless the behavior is explicitly documented and tested.

## Repository Organization

Expected long-term shape:

```text
crates/
  vectorkit-core/        # Rust retrieval core
  vectorkit-cli/         # Benchmarking and local tooling
wrappers/
  swift/                 # Swift/iOS/macOS wrapper
  python/                # Future wrapper, if added
  node/                  # Future wrapper, if added
docs/
  product/               # Active product decisions
  research/              # Deferred technical explorations
```

This structure is a guideline, not a requirement for early scaffolding. If the actual layout changes, update this file and the docs together.

## Language-Specific Guidance

Language-specific guidance lives in `docs/agents/`.

- Rust: `docs/agents/rust.md`

Add a new language file before adding a substantial wrapper or implementation language.

For every language wrapper, define:

- Public API style and naming conventions.
- Error mapping.
- Memory ownership and lifecycle rules.
- Threading and async behavior.
- Packaging and release process.
- Wrapper-specific tests and examples.
- Compatibility guarantees with the Rust core.

Wrappers should not reimplement retrieval logic. They should call the Rust core and provide idiomatic language bindings.

## Documentation

- Update product docs when behavior, scope, or priorities change.
- Keep `docs/product/working-memory.md` current with short-lived handoff
  context and remove stale notes once they are irrelevant or superseded.
- Keep research notes separate from active implementation decisions.
- Document benchmark methodology before relying on benchmark numbers.
- Include examples that compile or can be run directly once code exists.
- Prefer precise explanations over marketing language.

## Dependency Policy

- Keep dependencies minimal and justified.
- Prefer mature, well-maintained crates for serialization, error handling, testing, and benchmarking.
- Do not add a dependency to avoid writing a small amount of straightforward code.
- Do add a dependency when it materially improves correctness, safety, performance, or platform integration.
- Performance-critical code may use external crates when they are mature, benchmarked, and likely faster or safer than local code.
- Avoid custom implementations for complex performance-sensitive primitives when a proven crate exists; prefer local code for simple logic.
- Avoid dependencies that make iOS/macOS packaging difficult unless the tradeoff is documented.

## Agent Workflow

When making changes:

1. Read the active product spec and nearby code before editing.
2. Keep changes scoped to the requested behavior.
3. Update tests and docs with behavior changes.
4. Run the relevant checks.
5. Report what changed, what was verified, and any remaining risk.

If a request conflicts with the V1 product direction, call that out before implementing it.
