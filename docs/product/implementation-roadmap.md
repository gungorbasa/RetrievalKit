# RetrievalKit Implementation Roadmap

This roadmap records the completed V1 implementation and orders post-release
hardening work. It prioritizes correctness, adoption evidence, and qualification
gaps before new retrieval engines.

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
- Node.js, Kotlin/JVM, Android arm64-v8a, and browser/WASM wrappers.
- Separate Swift, Python, Node.js, Kotlin, and browser embedding integrations.
- Public SwiftPM, PyPI, npm, and Maven Central preview distribution.
- A public browser demo using live local embedding, RetrievalKit WASM retrieval,
  and browser generation over a versioned first-party corpus.
- Synthetic, fixture-backed, macOS, iOS, and persistence benchmarks.
- A benchmark-only 25K/49,999 hybrid stage profiler and a BM25 trace-allocation
  optimization that brings the independently validated 50K Mac public-path
  retrieval median-session P95 below the 15 ms qualification-boundary target.

V1 implementation and distribution are complete. Remaining work is
post-release automation repair, public-consumer validation, broader platform
qualification, and optional evidence expansion.

The post-release hybrid performance milestone is implemented and Mac-validated.
Physical-iPhone confirmation remains required before replacing the frozen
19.929 ms iPhone 50K hybrid result. See
`docs/product/reports/hybrid-performance-milestone-v1-report.md`.

The implementation and qualification plan for advertising RetrievalKit as a
complete semantic, hybrid, and graph-scoped retrieval package is maintained in
`docs/product/complete-retrieval-benchmark-and-marketing-roadmap.md`. Its public
quality, target-device, external-baseline, and claim gates are additional to
the core V1 engineering phases below; benchmark-only code must remain outside
production APIs and wrappers. The implementation-ready Phase 0 contract is
`docs/product/graph-retrieval-evaluation-contract-v3.md`. The original owner
decision selected iPhone 14 Pro Max as a conservative device; the 2026-07-18
Phase 4 scope amendment makes iPhone 17 Pro Max the sole required current
device and leaves older-device qualification optional.
Its third focused revision passed two fresh isolated implementation-author
reviews on 2026-07-16, closing the benchmark roadmap's Phase 0. Graph-aware
evaluation-artifact Phase 1 is complete. Its checked-in V3 conformance fixture,
whole-corpus A-C retrieval, graph-only D selection, and graph-scoped E-G
retrieval qualifications are complete. Phase 1.2c includes paired metrics,
combined persistence, an independent Python oracle, pinned `ir_measures`, and
byte-identical canonical artifact sets. The publication gate now additionally
pins official NIST `trec_eval`, derives clean release identities, assembles the
exact closed 44-file public layout, and validates two byte-identical emissions
independently. This is separate from the completed core V1 “Phase 1: Corruption
Detection” below.
The Phase 1.2c completeness pass additionally serializes all six canonical
invalid-execution reasons with query/run attribution, reports each Phase 1.2
slice explicitly in the CLI, and enforces the exact 56-file qualification
inventory. Repeated valid and invalid outputs are byte-identical; the valid
artifact hash is unchanged. Benchmark Phase 2 is complete. Phase 2a selected
HotpotQA distractor train/dev over the official linked-abstract corpus and
froze `docs/product/public-graph-collection-adapter-contract-v1.md`. Phase 2b
built the 12,670-record adapter twice from pinned inputs, independently replayed
the upstream seed resolution, validated production-backed ingestion, proved
all generated files byte-identical, and atomically published adapter manifest
SHA-256
`8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6`.
See `docs/product/reports/hotpotqa-graph-adapter-phase-2-report.md`. Benchmark
Phase 3a is complete: a pre-registered 36-candidate Run C development search
selected and froze alpha `0.2` with vector/keyword limits `100`/`100`, and the
development A-G matrix passed independent replay, external metric checks, and
two-root byte determinism. See
`docs/product/reports/hotpotqa-phase-3-development-ablation-report.md`.
Phase 3b locked reporting is complete. Its two-stage pipeline sealed
byte-identical label-free rankings before opening labels, reproduced the
scored root, passed independent and pinned external metric checks, and
published the 39-file canonical root without retuning. See
`docs/product/reports/hotpotqa-phase-3-locked-reporting-report.md`. Benchmark
Phase 3 is complete. Phase 4a now implements deterministic 10K/25K/50K
supported-product target-device graph workloads and the separately classified
`100k-384d-v3-stress` workload. Byte determinism, independent validation,
Apple M1 Max F32/I8 correctness and persistence/replay, staged instrumentation,
device-size/memory preflight, isolated iOS harness/linkage checks, and the
standalone artifact validator pass. Phase 4 is complete. Phase 4b iPhone 17
physical-device execution is closed with a supported-product PASS. Its
10K/25K/50K F32/I8
query matrix has 30 thermally valid sessions, and its complete 816-artifact
supported lifecycle matrix passes independent inventory and split-lineage
validation while preserving the accepted v3 query/prepare/build/save bytes.
All 12 graph-free sessions pass identical-result, zero-counter, and maximum
`1.03` median-P95 ratio gates. The owner permanently canceled eligible 100K
stress execution because of excessive device heat; partial stress files remain
rejected evidence. Contract V1 Amendment 3 adds the validation-only terminal
outcome `not_run_device_safety`. The validator reports supported `passed`,
graph-free `passed`, stress `not_run_device_safety`, and amended Phase 4b
closeout `passed`; omission of the exact cancellation authorization still fails
closed at the absent F32 preflight. See
`docs/product/reports/phase-4b-device-qualification-report.md`. The 100K row is
diagnostic, does not
change the fewer-than-50K V1 capacity envelope, and cannot authorize a public
quality, performance, device, support, or marketing claim. See
`docs/product/reports/phase-4a-deterministic-device-workloads-report.md`.
Benchmark Phase 5 external reference implementations are complete. The exact
and custom-application lanes passed; frozen USearch ANN recall missed its final
gate and its latency is not comparison evidence. The independent validator
passes the 10-file root while preserving `benchmark_acceptance: failed`. See
`docs/product/reports/phase-5-external-reference-implementations-report.md`.
Benchmark Phase 6 publication is complete. Its closed repository-local package
contains separate methodology, quality, Mac, and physical-device reports plus
a machine-readable claim register, evidence and licensing records, canonical
manifest, independent validator, mutation coverage, and deterministic
reproduction. Nine claims are permitted, six prohibited, and four withheld.
Benchmark Phase 7 regression gates are complete: deterministic PR gates,
controlled scheduled/full gates, and manual evidence-only release
qualification now fail closed through a 26-gate registry and independent
validator. See `docs/product/reports/phase-7-regression-gates-report.md` and
`docs/product/reports/phase-6-benchmark-publication-report.md`.
That benchmark phase is distinct from the completed release-and-distribution
Phase 5 below.

## Priority Summary

| Phase | Workstream | Status | Remaining work |
|---:|---|---|---|
| 1 | Corruption detection | Complete | Maintenance only |
| 2 | Thread-safety contract | Complete | Extend tests with future public operations |
| 3 | Memory-budget validation | Complete for the qualified iPhone 17 matrix | Older Apple devices before broader budget claims |
| 4 | Real retrieval-quality benchmark | Complete for V1 defaults and release claims | Optional BEIR/TREC and application-derived judgments |
| 5 | Release and distribution | Complete; v0.1.0 published | Repair recurring automation and validate future releases |
| 6 | Exact-search scaling gate | Not triggered | Reopen only after a measured <50K latency, energy, or recall miss |
| 7 | Website and in-browser demo | Complete; packages and live demo published | Mobile browser and cache-pressure qualification |

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

- Emit TREC-compatible qrels and run files and validate RetrievalKit metrics with
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

Status: the combined `v0.1.0` Swift/Python release surface, automatic
PR CI, one public Swift package backed by one graph-capable binary, macOS arm64
wheel matrix, deterministic bundle metadata, checksums, SBOM, provenance,
governance documents, and guarded publication workflows are implemented. The
exact-revision Phase 7 gates, signed tag, protected approval, and bounded
partial-publication recovery completed; SwiftPM, PyPI, npm, and Maven Central
now carry the v0.1.0 preview. Apache-2.0 licensing and company attribution are
complete.

Goal: make RetrievalKit installable without cloning the repository or manually
building Rust artifacts.

Work:

- Add CI for Rust formatting, Clippy, tests, Python typing/lint/tests, Swift
  tests, and persistence compatibility fixtures.
- Build versioned Apple XCFramework release artifacts with checksums.
- Publish one tagged Swift package with independently selectable
  `RetrievalKit` and `RetrievalKitGraph` products backed by one graph-capable
  binary target.
- Build and smoke-test both Python distributions for CPython 3.10–3.14 on
  macOS arm64. Other operating systems remain future work.
- Add release automation, signed tags, changelog checks, and migration notes.
- Add `LICENSE`, `CONTRIBUTING`, security policy, and issue templates.
- Publish hosted API and getting-started documentation.

Exit criteria:

- A fresh Swift project can add one package URL and run base-only, graph-only,
  or combined examples without linking competing native aggregates.
- A supported Python environment can install one wheel command and run the
  example.
- CI reproduces every published artifact from a tag.

## Phase 6: Exact-Search Scaling Decision

Status: gate not triggered. The qualified fewer-than-50K exact-search product
meets its current release targets, so parallel scanning and ANN/HNSW remain
deferred.

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

## Phase 7: Browser/WebAssembly SDK and Public Demo

Status: complete and published in v0.1.0. The protected release and bounded
publication recovery shipped `@gungorbasa/retrievalkit-browser` and
`@gungorbasa/retrievalkit-browser-embedding`; final release truth is recorded
in `release/publication-v0.1.0.json`.

Goal: publish a public website whose demo proves the local-first claim by
running RetrievalKit retrieval entirely in the visitor's browser, with an
optional on-device LLM answering over the retrieved results.

Delivered:

- A separate `wasm-bindgen` aggregate exposes `RetrievalDatabase`,
  `GraphDatabase`, and `GraphRetrievalDatabase` without changing native Cargo
  defaults, persistence, SimSIMD, or wrapper behavior.
- Dedicated Workers own browser databases and local MiniLM embedding. Retrieval
  has portable and SIMD128 tiers; browser databases remain intentionally
  in-memory for the current page session.
- Checked-in lifecycle, conformance, transfer, cache, and 10K/25K/50K benchmark
  paths qualify Chrome, Firefox, and Safari on the documented desktop reference
  environments and budgets.
- The public Apollo 11 demo uses live browser embedding, RetrievalKit WASM
  vector/graph/combined retrieval, and grounded browser generation. Website
  source and deployment remain in the private `gungorbasa/RetrievalKit-Website`
  repository, as required by the repository boundary.
- Numeric public claims remain mapped to the frozen claim register, and the
  website demo never selects interactive results from canned answers.

Open qualification, not V1 implementation work:

- Physical mobile browsers and private-mode/cache-pressure behavior.
- The full generated-answer demo outside its qualified Apple-silicon
  Chromium/WebGPU environment.
- Portable browser database snapshots; IndexedDB/OPFS persistence requires a
  separate format, integrity, migration, size, and hostile-input design.

## Recommended Execution Order

Complete one evidence-bearing slice at a time:

1. Restore green recurring automation and keep `main` release validators in
   sync with published status wording.
2. Re-run the developer-experience audit from clean, unauthenticated consumers
   of every published package family.
3. Qualify Android model acquisition, inference, lifecycle, offline restart,
   memory, thermal behavior, and compatibility on a physical arm64-v8a device.
4. Qualify mobile browsers plus private-mode and cache-pressure behavior before
   broadening the desktop browser claims.
5. Qualify older Apple-device memory and compaction headroom before generalizing
   the iPhone 17 Pro Max budgets.
6. Expand retrieval-quality evidence with BEIR/TREC-compatible runs, pooled
   blind judgments, and anonymized application queries.
7. Reopen parallel exact scanning or ANN/HNSW only after a measured miss inside
   the supported fewer-than-50K product envelope.

Every slice must preserve the capability-separated architecture and optional
embedding boundaries, update current docs and tests, and pass the affected
wrapper, release, and claim validators before landing.

## Explicitly Deferred

- HNSW/ANN implementation before the scaling gate.
- Server mode, networking, synchronization, dashboards, and distributed data.
- Automatic compaction policies before device memory and latency data exists.
- New retrieval abstractions without two concrete implementations.
- Embedding-model execution inside the Rust retrieval core.
- Reintroducing Swift ONNX or making Q8 the production Swift embedding default;
  direct Core ML FP32 is the qualified production path.
- NIST TREC participation until the post-release evaluation expansion is
  explicitly scheduled.
