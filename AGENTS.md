# AGENTS.md

Canonical guidance for Codex, Claude, Cursor, and other coding agents working in this repository.

Keep this file focused on repo-wide rules. Put language-specific instructions in `docs/agents/` and read only the files relevant to the change.

## Agent Setup

- Codex: read this root `AGENTS.md`.
- Claude: read `CLAUDE.md`, which points back to this file.
- Cursor: read `.cursor/rules/retrievalkit.mdc`, which points back to this file.
- Rust changes: also read `docs/agents/rust.md`.
- Python changes: also read `docs/agents/python.md`.

When changing repository guidance, update this file first. Keep tool-specific files as small compatibility entrypoints unless a tool genuinely needs different syntax.

## Product Direction

RetrievalKit is a local-first retrieval SDK for mobile and desktop apps. The first target is an iOS/macOS SDK with a Rust retrieval core and a Swift wrapper.

The current V1 direction is:

- Small local indexes: fewer than 50K chunks.
- Primary retrieval engine: exact vector search.
- Public retrieval modes: semantic exact-vector search and hybrid ranking.
  BM25 remains the measured lexical component of hybrid ranking, not a
  standalone high-level product capability.
- Core priorities: correctness, speed, filtering, persistence, and Swift/iOS integration.

Do not add HNSW, ANN indexing, server mode, sync, dashboards, or distributed database features unless the product spec is updated first. HNSW research exists in `docs/research/`, but it is deferred until exact/hybrid retrieval is polished and benchmarked.

Use `docs/product/retrievalkit-product-spec.md` as the implementation source of truth.

## Engineering Principles

- Prefer simple, explicit designs over broad abstractions.
- Optimize for correctness first, then measured performance.
- Keep public APIs small, stable, and easy to explain.
- Apply SOLID principles pragmatically. Do not add abstractions that do not reduce real complexity.
- Keep domain boundaries clear: retrieval, storage, filtering, ranking, persistence, and language bindings should not be tangled together.
- Keep the canonical corpus independent from optional derived capabilities.
  `CorpusIndex` owns records, chunks, stable identities, and generations;
  retrieval and graph indexes build on that state without becoming payload
  owners.
- Avoid speculative features. Build what V1 needs and leave clear extension points where future wrappers or engines are likely.
- Favor deterministic behavior for indexing, searching, ranking, and tests.
- Expose enough trace/debug data to explain retrieval results.
- Treat deleted, outdated, filtered, or dimension-mismatched chunks as correctness failures, not edge cases.

## Cross-Language Architecture And Native APIs

Rust and every language wrapper must present the same underlying system
architecture, capability boundaries, ownership model, persistence behavior,
query semantics, filtering behavior, ranking behavior, and correctness
guarantees. A wrapper must not quietly combine components that are separate in
Rust or another wrapper, split components that are intentionally unified, or
reimplement core behavior in its own language.

Architectural parity does not mean identical syntax. Every wrapper must feel
native and idiomatic to developers in its language and follow that language's
established best practices. For example, Python APIs should be Pythonic, Swift
APIs should follow Swift conventions, and Rust APIs should be idiomatic Rust.
Names, builders, value types, lifecycle patterns, error surfaces, sync/async
interfaces, and packaging may differ when required for a high-quality native
developer experience, while preserving the same system concepts and behavior.

Depart from the shared architecture only when the difference materially
improves correctness, performance, safety, platform integration, or developer
experience and genuinely makes sense for that language or platform. Before
doing so, document the reason, tradeoffs, and compatibility impact in the
active product documentation and add tests proving the intended behavior. Do
not introduce architectural differences merely for implementation convenience.

Speed and quality are first-class requirements across Rust and all wrappers.
Keep performance-sensitive retrieval work in Rust, minimize wrapper overhead
and data copying, use native language best practices, and verify parity,
correctness, performance-sensitive behavior, and API ergonomics before treating
a wrapper change as complete.

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
  retrievalkit-core/        # Rust retrieval core
  retrievalkit-cli/         # Benchmarking and local tooling
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
- Python: `docs/agents/python.md`

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

## Licensing

- RetrievalKit and every first-party language wrapper or distribution are
  licensed under Apache-2.0.
- The copyright holder is EGGYOLK YAZILIM TİCARET LİMİTED ŞİRKETİ, as recorded
  in the root `NOTICE`.
- Every new wrapper must declare `Apache-2.0` in its package metadata and
  distribute the root `LICENSE` and `NOTICE` with its release artifacts. The
  Python wrappers keep byte-identical copies of both files next to their
  `pyproject.toml` so wheels embed them; `validate_release.py` fails closed
  if the copies drift from the root files.
- `THIRD_PARTY_NOTICES.md` is generated by
  `scripts/release/generate_third_party_notices.py` from `Cargo.lock`.
  Regenerate it whenever dependencies change; release validation requires it.
- Do not introduce a different or additional project license without explicit
  owner approval.

## Documentation

- Update product docs when behavior, scope, or priorities change.
- `docs/product/working-memory.md` is the shared agent memory file for this
  repo. All agents (Codex, Claude, Cursor) record durable session learnings and
  short-lived handoff context there — never only in tool-private memory — and
  remove stale notes once they are irrelevant or superseded.
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
