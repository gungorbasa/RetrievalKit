# Changelog

All notable user-facing changes and persistence migrations are recorded here.

## Unreleased

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

Compaction is a synchronous maintenance operation. It temporarily retains old
and replacement structures to guarantee an all-or-nothing swap.
- Token-aware Swift and Python ingestion pipelines with custom chunker support.

### Compatibility

- Existing persistence format V1 indexes remain readable.
- Saving a V1 or V2 index writes format V3 and migrates or upgrades its payload
  into a checksummed generation under `.snapshots`.
- Index directories should be treated as VectorKit-owned. Applications must
  not modify `.snapshots` or `manifest.json` directly.

### Upgrade

No application code changes are required. Rebuild the Rust core or wrapper,
load the existing index normally, and call the existing save API when ready to
migrate it.
