# VectorKit Implementation Roadmap

This roadmap translates the product specification into the remaining engineering
work. It prioritizes correctness and usable local SDK functionality before new
retrieval engines.

## Current Baseline

Completed:

- Exact vector search with F32, F16, BF16, and I8 storage.
- Semantic exact-vector search and weighted/RRF hybrid ranking, with BM25 kept
  as hybrid's directly tested lexical component.
- Typed metadata filters and retrieval traces.
- Rust text chunking with Swift and Python wrappers.
- Token-aware pipeline orchestration and custom chunkers.
- Crash-safe generation-based persistence with cross-process save locking.
- Explicit compaction with stable active chunk IDs.
- Swift, Python, FFI, benchmark CLI, and Apple XCFramework paths.
- Synthetic, fixture-backed, macOS, iOS, and persistence benchmarks.

The remaining V1 work is production hardening, measurement, and distribution.

The implementation and qualification plan for advertising VectorKit as a
complete semantic, hybrid, and graph-scoped retrieval package is maintained in
`docs/product/complete-retrieval-benchmark-and-marketing-roadmap.md`. Its public
quality, target-device, external-baseline, and claim gates are additional to
the core V1 engineering phases below; benchmark-only code must remain outside
production APIs and wrappers. The implementation-ready Phase 0 contract is
`docs/product/graph-retrieval-evaluation-contract-v3.md`. The owner approved it
and selected iPhone 14 Pro Max with iOS 26 or later as the conservative device.
Its third focused revision passed two fresh isolated implementation-author
reviews on 2026-07-16, closing the benchmark roadmap's Phase 0. Graph-aware
evaluation-artifact Phase 1 is active. Its checked-in V3 conformance fixture,
whole-corpus A-C retrieval, graph-only D selection, and graph-scoped E-G
retrieval qualifications are complete. Phase 1.2c includes paired metrics,
combined persistence, an independent Python oracle, pinned `ir_measures`, and
byte-identical canonical artifact sets. Official `trec_eval` and final public
manifest assembly remain open, so overall Phase 1 has not exited. This is
separate from the completed core V1 “Phase 1: Corruption Detection” below.

## Priority Summary

| Order | Workstream | Why now | Gate to finish |
|---:|---|---|---|
| 1 | Corruption detection | Persistence must fail clearly on damaged data | Every persisted payload is verified before use |
| 2 | Thread-safety contract | Wrapper behavior must be predictable under concurrency | Supported concurrent operations are documented and tested |
| 3 | Memory-budget validation | Compaction and load can temporarily increase RSS | 24K/50K target scenarios fit documented device budgets |
| 4 | Real retrieval-quality benchmark | Candidate defaults need evidence from realistic data | Recall and ranking quality are measured against exact ground truth |
| 5 | Release and distribution | Source-only SDKs cannot be adopted reliably | CI builds, tests, signs, and publishes supported artifacts |
| 6 | Exact-search scaling gate | Determine whether >50K needs a new engine | Measurements choose parallel exact scan or ANN research |

## Phase 1: Corruption Detection

Status: complete.

Goal: detect damaged index files before they can produce incorrect results.

Rust core:

- Add per-file checksums to a new manifest format while retaining V1/V2 load
  compatibility.
- Verify vectors, chunks, BM25, and tombstones before constructing a loaded
  index.
- Reject invalid tombstone values rather than treating every non-zero byte as
  deleted.
- Validate manifest filenames, counts, dimensions, compression fields, and
  snapshot ownership as one explicit preflight step.
- Add a public read-only `validate_index(path)` operation so applications can
  diagnose an index without loading it for search.
- Return errors containing the damaged file, expected value, actual value, and
  recovery action.

Wrappers:

- Expose validation through Swift and Python without reimplementing checks.
- Preserve typed Python format/persistence errors.
- Improve Swift error mapping so persistence and invalid-format failures are
  distinguishable instead of sharing one generic core case.

Tests:

- Flip bytes independently in every persisted file.
- Truncate and append data to every file.
- Corrupt checksums and manifest fields.
- Verify V1 and V2 indexes still load and migrate normally.
- Verify validation never mutates or cleans the index directory.

Exit criteria:

- No damaged payload can load silently.
- Every failure identifies a recovery path: restore, rebuild, or remove the
  damaged index.
- Existing persisted fixtures remain compatible.

## Phase 2: Thread-Safety and Lifecycle Contract

Status: complete. Rust, C/FFI, Swift, and Python guarantees are implemented;
Swift passes Thread Sanitizer, and Python thread-pool tests verify GIL-free
parallel reads plus safe conflicting-mutation rejection.

Goal: make concurrency behavior explicit across Rust, FFI, Swift, and Python.

Decisions to lock:

- Whether V1 supports concurrent searches only, or concurrent search plus
  mutation.
- Whether Swift's actor serialization is the intended V1 guarantee or whether
  immutable search snapshots should allow parallel queries.
- Whether Python releases the GIL during search, save, and compaction.
- What synchronization C/FFI callers must provide.

Recommended V1 contract:

- Concurrent read-only searches are supported after indexing/loading.
- Mutation, compaction, save, and destruction require exclusive access.
- Do not add hidden locks to the hot search path until a benchmark demonstrates
  that an immutable snapshot design is insufficient.

Implementation and tests:

- Add compile-time `Send`/`Sync` assertions for the supported Rust types.
- Add deterministic multi-thread read-only search tests.
- Add race tests covering search versus delete/upsert/compact/save according to
  the chosen contract.
- Document handle lifetime and synchronization in the C header.
- Add Swift task-group and Python thread-pool integration tests.
- Run Thread Sanitizer where supported.

Exit criteria:

- Every public operation is classified as concurrent, serialized, or forbidden.
- Unsupported races fail at the wrapper boundary or are impossible by type.
- No use-after-free or partial mutation is possible through supported APIs.

## Phase 3: Memory-Budget Hardening

Status: the full 24K 384d/768d F32/F16/I8 matrix and 50K I8 profiles are
validated on iPhone 17 Pro Max. Older device classes remain; 50K F16/F32 are
outside the compact-size scope.

Goal: prove the index is safe on target mobile hardware, including maintenance
operations rather than search alone.

Work:

- Add isolated one-scenario-per-launch iOS benchmark presets.
- Measure build, load, search, save, and compaction peak RSS separately.
- Cover 24K and 50K chunks at 384d and 768d using F32, F16, and I8 where
  practical.
- Measure hybrid-enabled and vector-only persistence independently.
- Add compaction benchmarks at 10%, 25%, and 50% tombstone ratios.
- Decide whether all-or-nothing in-memory compaction needs a lower-memory
  streaming alternative.
- Add configurable benchmark budget failures so regressions fail checks rather
  than only appearing in reports.

Implemented harness:

- One scenario per CLI process or iOS app launch.
- Sampled RSS checkpoints for build, search, save, unload, load, delete, and
  compaction.
- Cold search plus warmed P50/P95/P99 latency.
- Machine-readable budget violations and nonzero CLI failure.
- iOS presets for the target chunk/dimension/encoding/workload matrix and
  10%/25%/50% compact-target tombstone ratios.

Remaining validation:

- Repeat the compact target on older supported iPhone/iPad classes.
- Decide whether compaction needs a streaming alternative.

Exit criteria:

- 24K × 384d I8 meets the 20 MiB persisted target with documented RSS.
- 50K scenarios meet the configured retrieval latency target or produce a
  documented scope limit.
- Compaction has a safe operating recommendation for target devices.

## Phase 4: Real Retrieval-Quality Benchmark

Status: V2 fixture and regression runner complete. The active baseline uses 42
graded queries, real MiniLM embeddings, competing documents, filters,
deletion/replacement checks, persistence reload, hybrid candidate-limit gates,
and a BM25-free exact F32/I8 comparison at top 5 and top 10. A later fixture
should add judgments from real application usage.

Goal: tune hybrid retrieval using realistic text and metadata instead of
synthetic score distributions.

Work:

- Create a versioned fixture with representative chunk text, metadata, filters,
  exact-name queries, semantic paraphrases, and expected relevance judgments.
- Compare hybrid candidate pairs such as 10/25, 25/25, 50/50, and 100/100.
- Report recall@k and ranking quality against high-candidate F32 exact search.
- Measure I8 recall against F32 on the same embeddings.
- Record latency, memory, disk size, and post-load results together.
- Use results to choose candidate defaults; keep per-query overrides.

Exit criteria:

- Default candidate limits have a quality/latency justification.
- Exact names, semantic paraphrases, filters, deletes, and replacements are all
  represented in the fixture.
- Benchmark reports are reproducible from a checked-in command.

V1 decision:

- Keep `50/50` as the default candidate pair. I8 recall@5 against the F32
  `100/100` reference measured `0.95`; `25/25` measured `0.90` and `10/25`
  measured `0.7333`.
- Keep candidate limits as public per-query overrides.

Future gold-standard milestone, non-blocking for Phase 5:

- Emit TREC-compatible qrels and run files and validate VectorKit metrics with
  `trec_eval` or `ir_measures`.
- Run at least SciFact and NFCorpus for external BEIR/Moss comparison.
- Build pooled, blind relevance judgments across vector, BM25, RRF, weighted
  fusion, and candidate configurations.
- Grow a locked 300–500-query release set from anonymized application queries.
- Run an appropriate official NIST TREC collection and evaluate participation
  in the TREC RAG track after the release pipeline is stable.

This milestone strengthens claims and later ranking decisions. It does not
delay packaging the already-tested V1 SDK.

## Phase 5: Release and Distribution

Status: a manual-only verification workflow is checked in for Rust, V2
retrieval quality, Python, the Apple XCFramework build, and Swift tests. It does
not run on pushes or pull requests. Automatic CI, artifact uploads, release
checksums, tag automation, and the remote Swift binary target are intentionally
deferred until VectorKit is ready to release.

Goal: make VectorKit installable without cloning the repository or manually
building Rust artifacts.

Work:

- Add CI for Rust formatting, Clippy, tests, Python typing/lint/tests, Swift
  tests, and persistence compatibility fixtures.
- Build versioned Apple XCFramework release artifacts with checksums.
- Switch the public Swift package to a tagged binary target.
- Build and smoke-test Python wheels for supported macOS, Linux, and Windows
  targets if Python remains a release target.
- Add release automation, signed tags, changelog checks, and migration notes.
- Add `LICENSE`, `CONTRIBUTING`, security policy, and issue templates.
- Publish hosted API and getting-started documentation.

Exit criteria:

- A fresh Swift project can add one package URL and run the example.
- A supported Python environment can install one wheel command and run the
  example.
- CI reproduces every published artifact from a tag.

## Phase 6: Exact-Search Scaling Decision

Goal: extend capacity only when measurements show the current engine misses its
latency target.

Decision order:

1. Measure exact search on target devices at 50K and larger realistic corpora.
2. Test parallel exact scanning only if CPU and energy measurements justify it.
3. Test lower-bit candidate storage only if memory is the binding constraint.
4. Start ANN/HNSW research only if exact search still misses the target.

Any future ANN implementation must:

- Live behind the Rust vector-index boundary.
- Use exact search as recall ground truth.
- Perform exact reranking of candidates.
- Preserve filtering correctness and deterministic final ordering.
- Include persistence, migration, recall, memory, and device benchmarks.

ANN does not ship merely because the dataset exceeds 50K. It ships only when it
improves measured latency without violating the agreed recall target.

## Recommended Execution Order

Implement one independently releasable slice at a time:

1. Checksummed manifest and read-only validation API.
2. Typed Swift/Python validation errors and corruption tests.
3. Thread-safety contract and read-only concurrency tests.
4. Isolated memory/compaction device benchmark presets.
5. Real retrieval-quality fixture and candidate-limit report.
6. CI and public Apple package distribution.
7. Add TREC-compatible external and production-derived quality evaluation.
8. Reassess parallel exact search and ANN using the collected evidence.

Each slice should update tests, wrapper docs, the changelog, and working memory,
then pass Rust, Python, Swift, wheel, and Apple packaging checks before commit.

## Explicitly Deferred

- HNSW/ANN implementation before the scaling gate.
- Server mode, networking, synchronization, dashboards, and distributed data.
- Automatic compaction policies before device memory and latency data exists.
- New retrieval abstractions without two concrete implementations.
- Embedding-model execution inside the Rust retrieval core.
- NIST TREC participation before release distribution is working; retain it as
  a committed post-release evaluation milestone.
