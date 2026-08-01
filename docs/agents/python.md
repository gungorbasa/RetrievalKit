# Python Agent Guidance

Python is a public V1 wrapper target released with Swift, initially as macOS
arm64 wheels for CPython 3.10–3.14. Read this file before creating or modifying
Python code.

## Role of the Python Wrapper

- Use Python to validate Rust core behavior, API ergonomics, realistic fixture
  benchmarks, and report generation.
- Keep Python wrapper code thin. Retrieval, filtering, ranking, persistence,
  quantization, and trace generation belong in the Rust core.
- Prefer less Python and more Rust. Python should mainly validate inputs,
  convert Python objects into Rust calls, and return Rust-produced results.
- Treat Python API, error, persistence, and behavior compatibility as a public
  release surface while keeping the implementation a thin Rust wrapper.
- Do not add server, dashboard, sync, ANN, or database behavior through Python.

## Package Shape

- Put the graph-free base wrapper under `wrappers/python/`. Optional aggregate
  capability distributions may use a sibling directory such as
  `wrappers/python-graph/` when they must link a different native artifact.
- Prefer a standard `pyproject.toml` package layout.
- Prefer `pyo3` and `maturin` for Rust-backed bindings unless a concrete
  packaging issue makes another approach better.
- Keep generated build artifacts, wheels, virtual environments, and caches out
  of source control.
- Keep Python examples small and runnable from the repository root once the
  wrapper exists.

## API Design

- Provide idiomatic Python names while preserving RetrievalKit concepts:
  documents, chunks, metadata, filters, searches, hits, scores, and traces.
- Keep public APIs explicit and small. Avoid broad builder frameworks or fluent
  APIs until real usage shows they help.
- Bind directly to Rust core operations. Do not create Python-side duplicates
  of indexing, scoring, filtering, persistence, or ranking behavior.
- Prefer Pythonic method names and keyword arguments. Use `Index.add(...)`,
  `Index.search(...)`, `limit=`, and `where=` at the public Python surface;
  translate to Rust's `upsert_document`, `SearchQuery`, `top_k`, and `Filter`
  types inside the binding.
- Accept normal Python dictionaries and lists for common document/chunk input:
  callers should be able to pass documents with `id`, optional document-level
  `metadata`, and a list of chunks with `text`, `embedding`, and optional
  chunk-level `metadata`.
- Use `where={...}` as the default metadata filter syntax for common equality,
  range, membership, and existence filters. Optional typed helpers such as
  `where.eq(...)`, `where.range(...)`, `where.all(...)`, and `where.any(...)`
  may exist for complex filters.
- Use typed request and result objects where they clarify behavior, especially
  for metadata filters and search results.
- Make vector dimension, metric, encoding, and persistence compatibility errors
  fail at API boundaries with clear messages.
- Return chunks from search. Do not silently group results into documents unless
  the caller explicitly asks for that behavior.
- Expose trace/debug data from the Rust core instead of reconstructing ranking
  explanations in Python.

## Embeddings

- RetrievalKit's core input is embeddings, not raw text. Do not put embedding
  model execution in the Rust retrieval core.
- The Python wrapper may document how callers create embeddings from text, but
  search APIs should keep the embedding boundary explicit:
  `index.search(query_embedding, limit=10, where=...)`.
- If a convenience text API is added, keep it opt-in and provider-based, for
  example `index.search_text("query", embed=embedding_provider, limit=10)`.
  The provider should return a vector with the exact index dimension.
- Do not add a required network dependency for embeddings. Local embedding
  providers and remote embedding providers should both be possible.
- Validate provider output dimensions before calling Rust search.
- The first-party local provider is the separate optional
  `wrappers/python-embedding` distribution (`retrievalkit-embedding`). Keep it
  independent from the base and graph retrieval distributions and bind the
  separate `retrievalkit-embedding` Rust crate, never `retrievalkit-core`.
- The production Python provider exposes only pinned FP32 MiniLM inference:
  fixed 256-token input and exactly 384 finite, L2-normalized F32 values.
  Model acquisition is allowed only during explicit `load` or `prefetch`;
  `local_only` must be network-free. Model precision remains independent from
  RetrievalKit's default signed-I8 database encoding.
- The approved v0.1.0 embedding distribution is `retrievalkit-embedding`.
  Publish it only from the same closed, protected release workflow as the base
  and graph distributions.

## FFI And Ownership

- Keep ownership rules explicit for every Rust object exposed to Python.
- Prefer safe Rust-backed Python classes over raw pointer handling in Python.
- Ensure every long-lived index object has a clear lifecycle and releases Rust
  resources deterministically when closed or garbage-collected.
- Avoid exposing borrowed Rust data that can outlive the owning index.
- Do not copy large vectors, chunk text, or result payloads unless the API
  boundary requires ownership.
- Keep NumPy support optional unless benchmarks show it materially improves
  developer ergonomics or performance.

## Error Handling

- Map Rust errors into specific Python exceptions where practical.
- Include actionable context for dimension mismatches, unsupported formats,
  missing files, persistence failures, filter validation, and deleted or stale
  chunk behavior.
- Do not raise generic `Exception` for known RetrievalKit failures.
- Do not hide Rust validation failures behind Python fallback behavior.

## Performance

- Do not put JSON parsing, filesystem reads, SQLite queries, or broad Python
  object allocation on the hot search path.
- Keep vector search and ranking in Rust. Python should prepare inputs and
  receive already-ranked results.
- Avoid per-query Python loops over all chunks or vectors.
- Prefer bulk Rust calls over chatty Python-to-Rust loops. Batch inserts,
  updates, deletes, and searches when the API can do so clearly.
- Keep Python benchmark/report code outside production wrapper paths.
- Benchmark wrapper overhead separately from Rust retrieval latency.
- Report benchmark device, build mode, corpus shape, vector dimension, encoding,
  top-k, filters, recall baseline, persistence size, and load time.
- Prefer realistic fixture benchmarks before making claims about text, metadata,
  BM25, memory, or package-size behavior.

## Typing And Style

- Use type hints for public Python APIs.
- Keep data models explicit and readable; prefer dataclasses or typed classes
  for public request/result shapes when useful.
- Keep Python code deterministic in tests and benchmarks.
- Avoid hidden global mutable state.
- Avoid metaprogramming and dynamic attribute tricks in public APIs.
- Keep dependencies minimal. Add a dependency only when it materially improves
  packaging, typing, testing, or benchmark/report quality.

## Testing

- Add Python tests for wrapper lifecycle, add/update/delete flows, dimension
  validation, metadata filters, persistence reload, and deterministic results.
- Compare Python wrapper search results against Rust core expectations where
  practical.
- Include at least one fixture-backed benchmark path once realistic fixtures
  exist.
- Keep tests runnable without network access.
- Separate unit tests, integration tests, and benchmark commands.

## Tooling

Before considering Python wrapper changes complete, run the relevant checks once
the package exists:

```bash
maturin develop
python -m pytest
python -m ruff check .
python -m mypy .
```

If the project has not adopted `ruff` or `mypy` yet, do not add them only to
complete a small change. Document the checks that actually exist and run them.
