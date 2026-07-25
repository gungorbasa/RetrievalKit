# Changelog

All notable user-facing changes and persistence migrations are recorded here.

## 0.1.0 - Unreleased preview

### Added

- Crash-safe transactional index saves. New data is written to an immutable
  generation and synced before `manifest.json` publishes it.
- Cross-process save locking prevents concurrent writers from publishing and
  cleaning generations out of order. Locks are released by the operating system
  if a process exits or crashes.
- Explicit Rust, Swift, and Python index compaction removes tombstoned payloads,
  preserves active chunk IDs, reports estimated reclaimed memory, and remains a
  cheap no-op when there is nothing to reclaim.
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
  storage. Hybrid queries default to 50/50 candidates and RRF with `rrf_k=60`,
  matching the measured V1 quality configuration. F32 and weighted fusion
  remain explicit options.
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

Compaction is a synchronous maintenance operation. It temporarily retains old
and replacement structures to guarantee an all-or-nothing swap.
- Token-aware Swift and Python ingestion pipelines with custom chunker support.

### Compatibility

- Search result buffers now use a packed UTF-8 arena and offset/length ranges
  instead of separately allocated C strings. The graph aggregate ABI version is
  10. Native libraries, headers, and Swift wrappers from different ABI versions
  must not be mixed.
- Existing persistence format V1 indexes remain readable.
- Saving a V1 or V2 index writes format V3 and migrates or upgrades its payload
  into a checksummed generation under `.snapshots`.
- Index directories should be treated as RetrievalKit-owned. Applications must
  not modify `.snapshots` or `manifest.json` directly.

### Upgrade

No Swift application source changes are required. Rebuild and upgrade the Rust
native artifact, C headers, and Swift wrapper together. Then load the existing
index normally and call the existing save API when ready to migrate its
persistence format.
