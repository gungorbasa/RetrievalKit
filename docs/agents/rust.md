# Rust Agent Guidance

Rust is the core implementation language for RetrievalKit. Read this file before creating or modifying Rust code.

## Role of the Rust Core

- Keep the Rust core independent from Swift, Python, Node, HTTP, or UI concerns.
- Implement retrieval, filtering, ranking, persistence, and trace generation in Rust.
- Expose stable boundaries that wrappers can call without reimplementing retrieval logic.
- Keep wrapper ergonomics in wrapper code, not in the Rust retrieval core.

## Design

- Model the domain with explicit types for documents, chunks, chunk IDs, metadata, filters, scores, and traces.
- Use internal numeric IDs for hot-path operations. Keep caller-provided document IDs as external identifiers.
- Keep search, ranking, filtering, persistence, and benchmark logic separate as the codebase grows.
- Prefer immutable search structures after load. Isolate mutation to indexing, updates, deletes, compaction, and persistence flows.
- Validate vector dimension, metric, normalization, and format compatibility at API boundaries.
- Keep deleted and superseded chunks out of every final result set.
- Avoid broad traits or generic abstractions until there are at least two real implementations that need them.

## Error Handling

- Use typed errors in library crates.
- Include actionable context for load, persistence, dimension, format, and validation failures.
- Do not panic in library code for caller-provided input.
- Reserve panics for internal invariant violations that indicate programmer error.

## Performance

- Avoid cloning vectors, documents, and large text on the hot path.
- Prefer slices, iterators, and borrowed data when ownership is not required.
- Use `Vec` intentionally and preallocate when sizes are known.
- Keep vector data contiguous where practical.
- Consider `mmap`, SIMD, and compact binary formats after baseline behavior is correct.
- Keep scoring code allocation-light and benchmarked.
- Do not hide expensive locks, filesystem reads, database queries, or JSON parsing inside query APIs.
- Gate unsafe code behind a clear module boundary, tests, benchmarks, and comments explaining the invariant.

## Concurrency

- Prefer data-parallel search or indexing only when benchmarks show it helps.
- Keep shared mutable state minimal and explicit.
- Make thread-safety guarantees clear in public types.
- Avoid global mutable state.

## Testing

- Add unit tests for scoring, filters, metadata comparisons, tombstones, updates, deletes, and dimension validation.
- Add integration tests for persistence reload behavior and stable query results.
- Add fixture-based tests for ranking and filtering combinations when practical.
- Use exact search as the recall ground truth for any future approximate engine.
- Keep benchmark datasets and benchmark methodology documented.

## Tooling

Before considering Rust changes complete, run the relevant checks:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

When benchmarks exist, run the relevant benchmark command and report the dataset, device, build mode, and result summary.
