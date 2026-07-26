# Changelog

All notable user-facing changes and persistence migrations are recorded here.

## 0.1.0 - Unreleased preview

### Added

- Repository-local, provisionally named TypeScript/Node base and graph
  aggregates for Node.js LTS on macOS arm64. Promise-based N-API operations use
  typed values, `Float32Array`, exact `bigint` transport, Rust-owned search and
  graph semantics, deterministic async disposal, package-content checks, and
  local-install smoke tests.
- Repository-local Kotlin/JVM and Android base and graph aggregates with typed
  JNI transport, `FloatArray` embeddings, `AutoCloseable` lifecycle, opaque
  synchronized handles, and Android arm64-v8a AAR packaging.
- Python graph queries, results, and stable candidate projection now cross
  PyO3 as typed values without JSON. Projection filtering, stale-selection
  checks, ordering, and counts remain owned by the canonical Rust corpus.
- Progressive Python, TypeScript, and Kotlin ingestion accepts ordinary
  documents plus direct embeddings and infers dimension in Rust, while
  preserving advanced compatibility APIs where they already existed.
- Crash-safe transactional index saves. New data is written to an immutable
  generation and synced before `manifest.json` publishes it.
- Cross-process save locking prevents concurrent writers from publishing and
  cleaning generations out of order. Locks are released by the operating system
  if a process exits or crashes.
- Explicit Rust, Swift, and Python index compaction removes tombstoned payloads,
  preserves active chunk IDs, reports estimated reclaimed memory, and remains a
  cheap no-op when there is nothing to reclaim. Compaction is a synchronous
  maintenance operation and temporarily retains old and replacement structures
  to guarantee an all-or-nothing swap.
- Checksummed persistence format V3 verifies vectors, chunks, BM25, and
  tombstones with SHA-256 before loading. Rust, Swift, and Python expose
  read-only validation APIs and typed corruption failures.
- Parallel Swift exact, keyword, and hybrid searches on one `VectorIndex`, with
  writer-preferring exclusive access for upsert, delete, save, and compaction.
  The C/FFI threading and handle-lifetime contract is now explicit.
- Python retrieval, persistence, and maintenance release the GIL during
  Rust-only work. Shared-index searches may run across Python threads, while
  PyO3 exclusive borrowing rejects conflicting mutation safely.
- An isolated memory benchmark now measures sampled peak RSS across build,
  cold/warm search, save, unload, load, delete, and compaction. JSON budgets can
  fail the CLI, and the iOS app provides one-scenario-per-launch target presets.
- Automated iOS memory presets print JSON to the attached device console and
  exit with a budget-aware status. iPhone 17 Pro Max measurements now define
  provisional 24K compact-target and 50K extended-capacity budgets.
- A versioned MiniLM retrieval-quality fixture now gates relevance, F32/I8
  overlap, candidate limits, filters, deletions, replacements, persistence
  reload, and latency. Its V1 evidence keeps `50/50` as the hybrid default and
  confirms 98.33% top-5 and 100% top-10 I8/F32 vector-only overlap.
- New Rust, Swift, and Python indexes now default to I8 scalar-quantized vector
  storage. Hybrid queries default to 50/50 candidates and weighted normalized
  score fusion with query-time `alpha = 0.6`, matching the public search API.
  F32, different `alpha` values, and explicit candidate limits remain available.
- Retrieval-quality V2 expands the benchmark to 306 documents and 42 graded,
  ambiguous queries. It adds a human relevance-recall gate while preserving V1
  as a historical baseline.
- A manual-only verification workflow can check Rust, the V2 retrieval-quality
  gates, Python typing/lint/tests and installed wheels, plus Apple XCFramework
  and Swift builds without running automatically or publishing artifacts.
- Evidence-led README with claim-register enforcement and tested
  capability-oriented quickstarts.
- Combined Swift/Python release-candidate tooling with reproducible artifact
  inventory, SBOM, provenance, and fail-closed publication authorization.
- Token-aware Swift and Python ingestion pipelines with custom chunker support.
- Clean-source onboarding qualification now measures Python, Swift, Node.js,
  and Kotlin time to first result, records an explicit evidence schema and
  Swift toolchain metadata, and runs monthly or on demand.
- Source-preview documentation now includes searchable Swift guidance,
  self-contained language examples, responsive mobile actions, and a recovery
  page for unknown routes.
- Node.js wrapper tooling now accepts the maintained Node.js 22.13+ and 24 LTS
  ranges, keeps package engine declarations synchronized, and has zero npm
  audit findings after its development-tool refresh. Kotlin preflight now
  distinguishes the required JDK 17 build toolchain from the Java 11 bytecode
  target and reports the exact selected Java binary with recovery commands.

### Compatibility

- Search result buffers now use a packed UTF-8 arena and offset/length ranges
  instead of separately allocated C strings. Effective result metadata uses a
  flat packed entry table referencing the same arena, and hybrid `alpha` is
  stored once per buffer. Native boundary types and constants now use the
  product-aligned `RetrievalKit`/`RETRIEVALKIT_` prefixes instead of the stale
  pre-rename `Vk`/`VK_` prefixes. The graph aggregate ABI version is 12. Native
  libraries, headers, and wrappers from different ABI versions must not be
  mixed.
- Public hybrid `alpha` endpoints now disable candidate generation from the
  zero-weight source: `alpha = 1` is truly vector-only and `alpha = 0` is truly
  BM25-only. Generic C fusion/RRF entrypoints were removed; RRF remains an
  internal Rust benchmark option.
- Swift and Python exact, BM25, and hybrid hits now share the canonical result
  metadata contract. Hybrid traces expose `alpha`; the constant
  `filterMatched`/`filter_matched` field and public fusion dictionaries were
  removed. Swift graph metadata now uses the shared `MetadataValue`;
  `GraphMetadataValue` remains as a deprecated compatibility alias.
- Swift now publishes `RetrievalKit` and `RetrievalKitGraph` from one package
  backed by one graph-capable native aggregate. Applications may select either
  or both products without linking competing native libraries. `TextChunker`
  is part of `RetrievalKit`; the separate `RetrievalKitIngest` product was
  removed.
- Invalid hybrid `alpha` values are query-argument errors in Rust, Swift, and
  Python rather than being mislabeled as an invalid persisted index format.
- `RetrievalKitPipeline` now accepts the shared typed `DocumentID` used by the
  progressive Swift API while preserving its existing string result surface.
- Existing persistence format V1 indexes remain readable.
- Saving a V1 or V2 index writes format V3 and migrates or upgrades its payload
  into a checksummed generation under `.snapshots`.
- Index directories should be treated as RetrievalKit-owned. Applications must
  not modify `.snapshots` or `manifest.json` directly.

### Upgrade

Rebuild and upgrade the Rust native artifact, C headers, and language wrapper
together. Swift callers should replace `filterMatched` reads with the fact that
every returned hit already passed the filter, and read `trace.alpha` instead of
an internal fusion shape. Python callers should make the equivalent
`filter_matched`/`fusion` migration. Existing indexes remain readable; load and
save normally when ready to migrate their persistence format.
