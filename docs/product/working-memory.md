# RetrievalKit Working Memory

This file captures active implementation context that should survive chat
changes. Keep it short. Delete or move notes once they become irrelevant,
implemented, or superseded by the product spec.

## Current Workflow

- 2026-07-23: the project was renamed from VectorKit to RetrievalKit across
  crates (`retrievalkit-*`), Swift packages (`RetrievalKit`,
  `RetrievalKitGraph`, `RetrievalKitPipeline`, `RetrievalKitShared`,
  `RetrievalKitBench`, `RetrievalKitIOSBench`), Python distributions
  (`retrievalkit`, `retrievalkit-graph`), FFI symbols/headers, scripts, CI,
  and docs. Exception: the frozen Phase 6 evidence chain keeps the historical
  VectorKit naming because `manifest.json` hash-pins it —
  `benchmarks/publication/artifacts/**`, `contract-v1.json`,
  `validate_publication.py`, `generate_publication.py`, and
  `tests/test_publication.py` must not be renamed while phase6-publication-v1
  remains the authorized claim source. `validate_readme.py` uses the new
  names (it validates the current README). The GitHub repository is now
  `gungorbasa/RetrievalKit`. The v0.1.0 xcframework checksums in
  `release/release-v0.1.0.json` refer to archives built under the old artifact
  names, so Apple artifacts must be rebuilt before release. The rename also
  changed the frozen Phase 7 contract namespace and repository-local evidence
  paths; its manifest, validator constants, and report identities were
  reconciled on 2026-07-23 without changing gate inventory, thresholds,
  baselines, fixtures, or result schema.
- The `v0.1.0` combined Swift/Python release-candidate implementation is the
  active distribution slice. The root README is an evidence-led product page
  whose numeric observations are mapped to permitted Phase 6 claim IDs and
  mutation-tested in CI. The root Swift package exposes five products with
  separate base/graph remote aggregates; macOS arm64 Python targets CPython
  3.10–3.14. Release tooling produces canonical XCFramework archives, a closed
  wheel matrix, checksums, SPDX SBOM, and provenance, then validates two-root
  determinism and fresh consumers. On 2026-07-23 the owner selected
  Apache-2.0 for RetrievalKit, with copyright held by EGGYOLK YAZILIM TİCARET
  LİMİTED ŞİRKETİ; the root license, notice, Cargo metadata, and Python
  metadata are reconciled. Publication remains fail-closed until the owner
  provisions passing Phase 7 scheduled/release results for the release
  revision, authorizes claim handling, and approves a signed tag. No
  publication or device command is part of this slice. See
  `docs/product/release-process.md`.
- Benchmark Phase 7 is complete. Contract V1 and the 26-gate registry freeze
  deterministic PR correctness/integrity, provisioned full-quality, and
  controlled release-performance gates. The checked-in synthetic fixture has
  no HotpotQA or device evidence. Generic CI never blocks on absolute timing;
  missing scheduled/release inputs are `not_provisioned`, not passed. Manual
  release qualification validates pre-collected 10K/25K/50K F32/I8 evidence
  only, requires explicit owner authorization, contains no device command, and
  exposes no 100K option. See
  `docs/product/reports/phase-7-regression-gates-report.md`.
- The approved capability-separated Rust/FFI/Swift architecture is implemented
  and locally qualified through all seven clean commit gates.
- Keep customer-specific fixtures deferred; the graph package remains generic
  and schema-driven.
- Prefer mature fast crates for performance-sensitive work when they clearly
  help. Avoid dependencies for simple local logic.
- The planned commercial qualification must evaluate RetrievalKit as a complete
  semantic, hybrid, and graph-scoped retrieval package while preserving the
  capability-separated API architecture. The benchmark and claim
  roadmap lives in
  `docs/product/complete-retrieval-benchmark-and-marketing-roadmap.md`.
- The implementation-ready graph evaluation V3 contract is in
  `docs/product/graph-retrieval-evaluation-contract-v3.md`. The owner approved
  it and originally selected iPhone 14 Pro Max as a conservative physical
  device. The 2026-07-18 Phase 4 scope amendment makes iPhone 17 Pro Max the
  sole required current device; older-device qualification is optional future
  work. The third focused revision passed two fresh isolated
  implementation-author reviews on 2026-07-16: both reproduced the exact A-J
  fixture, population hashes, 15-run matrix, artifact/hash schemas, arithmetic,
  and portability rules without a blocker. Benchmark roadmap Phase 0 is
  complete and graph-aware evaluation-artifact Phase 1 is complete. Phase 1.1
  froze the checked-in V3 conformance fixture, schema, populations, 15-run
  matrix, canonical serialization, and byte-rerun validators. Phase 1.2a
  qualifies production-backed whole-corpus A-C (F32 semantic, I8 semantic, and
  I8 weighted hybrid), exhaustive document projection, metrics, save/load
  equivalence, byte-identical partial artifacts, and independent Python
  rankings/TREC/metrics. Phase 1.2b Run D qualifies explicit/topic/team
  graph selection through production `GraphDatabase`, corpus-owned candidate
  filtering and stable identity materialization, Rust/FFI/Swift projection
  parity, save/validate/load equality, stale-selection rejection, byte-stable
  partial artifacts, and exact independent Python agreement for 7 selections
  and 14 paths. Phase 1.2c now qualifies all nine E-G runs through production
  `GraphRetrievalDatabase`, 15 valid executions and six exclusions, D-equivalent
  selections/paths, paired A-E/B-F/C-G metrics, combined persistence and stale
  selection rejection, independent Python reconstruction, pinned
  `ir_measures`, and two byte-identical 56-file artifact sets. Phase 1.2c is
  complete. Its integration-review closure now handles query-local and
  run-wide invalid executions canonically for all six reasons, rebuilds every
  status-derived artifact, reports 1.2a/1.2b/1.2c and publication state
  accurately in the CLI, and rejects any inventory other than the exact
  56-file preimage. Repeated invalid outputs are byte-identical and the valid
  artifact-set SHA-256 remains
  `ee264e919ab5872fd400354f5aa332993fd55fdedcaab400e6f5ba41619f631c`.
  The publication closure pins official NIST `trec_eval`, derives exact clean
  release identities and release-context run IDs, assembles the closed 44-file
  public root, and independently validates two byte-identical emissions plus
  section 4.7 logical-run portability. See
  `docs/product/reports/graph-retrieval-phase-1-publication-report.md`. No public
  graph-retrieval quality, performance, device, or marketing claim is complete.
  Benchmark Phase 2 is complete. Phase 2a selected HotpotQA distractor train
  V1.1 and publicly judged distractor dev V1 over the January 14, 2019
  linked-abstract corpus; the exact source, label-blind corpus, graph, query,
  seed, MiniLM, manifest, and validation rules are frozen in
  `docs/product/public-graph-collection-adapter-contract-v1.md`. Phase 2b built
  both closed V3 roots twice, independently replayed all 3,000 source-only seed
  outcomes, validated production-backed ingestion, proved every generated file
  byte-identical, and atomically published adapter manifest SHA-256
  `8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6`.
  See `docs/product/reports/hotpotqa-graph-adapter-phase-2-report.md`. 2Wiki is
  deferred because its dataset-content license is unstated and its Dropbox
  artifacts are mutable without publisher checksums. Benchmark Phase 3a is
  complete. The pre-registered 36-candidate development search froze weighted
  hybrid alpha `0.2` with vector/keyword limits `100`/`100`; development A-G
  passed independent replay, pinned `ir_measures`, official NIST `trec_eval`,
  persistence checks, and two-root byte determinism. The lock SHA-256 is
  `ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118`.
  See
  `docs/product/reports/hotpotqa-phase-3-development-ablation-report.md`. The
  Phase 3b locked run is complete. The dedicated pipeline produced two
  byte-identical label-free ranking roots, opened labels only after ranking
  seal
  `90a0dd8ab2b9a3b575ad6e80366703fb8eb24dc01dd11d859645da00ccc9128c`,
  reproduced the scored root, and atomically published a 39-file canonical
  result. A–C executed 297 test queries and D–G executed 296 after the one
  frozen ambiguity exclusion. Independent recalculation differed by at most
  `1.7763568394002505e-15`; pinned `ir_measures` and official `trec_eval`
  matched exactly. Three attempts are disclosed, with no result root from the
  two failed evaluator attempts. See
  `docs/product/reports/hotpotqa-phase-3-locked-reporting-report.md`. Benchmark
  Phase 3 is complete. Phase 4a and Phase 4b are complete:
  deterministic 10K/25K/50K supported-product workloads and
  `100k-384d-v3-stress` share one frozen
  generator/policy; all four fixture/manifest pairs reproduce byte-identically,
  and independent validation, Apple M1 Max F32/I8 correctness and
  persistence/replay, staged instrumentation, memory estimation, isolated iOS
  harness builds, and linkage checks pass. Phase 4b physical-device execution
  is closed on iPhone 17 Pro Max with a supported-product PASS. The supported
  query matrix is complete with
  30 thermally valid sessions across 10K/25K/50K F32/I8, and all 816 supported
  lifecycle artifacts are complete with unique process IDs, valid thermal and
  foreground boundaries, operation-specific correctness, exact component
  accounting, and load/replay equivalence. All 12 graph-free
  baseline/candidate sessions pass identical-result, zero-graph-counter, and
  maximum `1.03` median-P95 ratio gates. The owner permanently canceled the
  eligible 100K diagnostic stress lane after reporting excessive device heat;
  the accepted stress tree is empty and the five partial F32 artifacts are
  preserved only as timestamped rejected evidence. Contract V1 Amendment 3
  defines the validation-only terminal outcome `not_run_device_safety` while
  retaining complete F32/I8 stress evidence as the normal alternative. The
  validator passes the amended closeout only with the exact cancellation
  authorization; without it, the absent F32 preflight still fails closed. See
  `docs/product/reports/phase-4b-device-qualification-report.md`. The 100K row
  is diagnostic,
  must remain labeled `stress`, and does
  not change the V1
  fewer-than-50K capacity envelope. See
  `docs/product/reports/phase-4a-deterministic-device-workloads-report.md`.

  Owner-directed Phase 4b stress cancellation (2026-07-20): do not resume the
  `100k-384d-v3-stress` physical-device lane. The user stopped it permanently
  because the iPhone became excessively hot. Amendment 3 is SHA-256
  `656c6065c95e7ea85928dacb81eaca423b5ddbfb827b45bf13b31798dd958133`.
  The validation-only cancellation authorization is SHA-256
  `926cfa543889cabbedf591d9de3e98d5bfe57886e0a31baa0595bf88e6785e07`.
  It binds v4 unchanged, the 846 supported artifacts at
  `f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a`,
  the 12 graph-free artifacts at
  `6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a`,
  zero accepted stress artifacts, and the five partial files only as rejected
  evidence. The result is supported `passed`, graph-free `passed`, stress
  `not_run_device_safety`, and amended Phase 4b closeout `passed`. Never
  describe this as a 100K execution or benchmark PASS, and do not change the
  support boundary or marketing classification.

  Owner-approved Phase 4b reporting variance (2026-07-19): retain the current
  iPhone 17 evidence without rerunning solely because the device moved within
  iOS 26 from 26.5.1 (`23F81`) to 26.5.2 (`23F84`). The 30 supported query
  sessions and 10K/F32 lifecycle prepare artifact were captured on `23F81`;
  the three 10K/F32 build warmups and 20 measured build samples were captured
  on `23F84`. The final report must disclose this split explicitly. Do not
  rewrite the immutable v3 authorization hash or relabel artifact provenance.

  Owner-approved Phase 4b lifecycle-reporting decision (2026-07-19): retain
  the frozen harness behavior for `save` and `read_only_validation` operations.
  Their artifacts prove the operation through successful persistence or
  read-only directory validation plus persisted-component accounting, and
  therefore serialize `correctness_checks` as `null`. `prepare`, `build`,
  `cold_load`, `warm_load`, and `replay` continue to record the applicable
  behavioral correctness checks, with load/replay operations also requiring
  replay equivalence. The independent validator already enforces these
  operation-specific rules. Do not rebuild, reauthorize, or rerun existing
  evidence solely to repeat all 11 behavioral checks in every save or
  read-only-validation artifact. Disclose this distinction in the final Phase
  4b report.

  Owner-approved Phase 4b foreground/lineage decision (2026-07-19): after
  three preserved `read_only_validation` attempts completed while UIKit still
  reported background state, add a bounded pre-measure foreground gate to both
  isolated benchmark apps and reauthorize them as v4. Preserve the 77 accepted
  v3 query/prepare/build/save artifacts byte-for-byte under their original
  authorization. Their closed artifact-set SHA-256 is
  `a7d021e0b45fbd2a722482af44428335eac0d8ab188032676c4643e051e7a9dc`.
  Use v4 only for unfinished paths, validate both authorization and executable
  identities, and update the apps in place without erasing persisted state.
  See `docs/product/target-device-graph-benchmark-contract-v1-amendment-2.md`.
  Authorization v4 is
  `benchmarks/device-graph/phase4b-execution-authorization-v4.json`, SHA-256
  `4f7aab9657bb836e4e434cd701e70ed55dc2cd1adfd4b4d4ec46178f1d76702f`.
  It binds source commit `9201410f88648743574801dced76bb5b551eb1f9`,
  base executable `f96b69c5...cae5a9`, graph executable
  `6b6ac8a3...bd97c`, and the unchanged v3 framework binaries.
  The v4 apps were installed in place and the blocked 10K/F32
  `read_only_validation` lane is now complete: three warmups and 20 measured
  launches all report foreground execution, unique process IDs, successful
  directory validation, and the v4 authorization hash. The canonical
  path/SHA-256 set for these 23 artifacts is
  `0567349fd68661cdbd84415c76ef31531f00336a66aaef5937fee6c4187866ad`.
  Measured operation durations ranged from 125,176,208 to 129,478,458 ns
  (median 127,407,646 ns). The remaining supported lifecycle and graph-free
  lanes later completed under v4; the stress lane reached the separate terminal
  safety outcome described above.

## Active Product Constraints

- RetrievalKit is local-first retrieval for mobile/desktop, with iOS/macOS as the
  first wrapper target.
- V1 public retrieval remains semantic exact-vector search and hybrid ranking;
  BM25 remains hybrid's internal lexical component,
  filtering, persistence, and Swift integration. An optional fully local graph
  package is now authorized behind gated M0-M5 milestones; graph-free core hot
  paths and artifacts remain graph-neutral.
- Do not add HNSW/ANN, server mode, sync, dashboards, or distributed database
  behavior unless the product spec changes.
- Retrieval must stay fast on local devices. Avoid hot-path JSON, SQLite,
  network calls, avoidable allocation, and broad string lookups.

## Optional Graph Roadmap Status

- Retrieval configuration no longer uses a `.hybrid` extra. Every
  retrieval-capable database builds semantic and BM25 state, and high-level
  hybrid calls blend them directly with query-time `alpha` (`1` vector-only,
  `0` BM25-only). Compact snapshots may omit persisted BM25 and rebuild it from
  canonical chunk text on load. The graph aggregate ABI is version 7 after
  typed graph candidate projection.

- The capability-separated architecture is approved. `CorpusIndex` becomes the
  canonical owner; `RetrievalIndex` and `GraphEngine` are derived components.
  Swift will expose `RetrievalDatabase`, `GraphDatabase`, and
  `GraphRetrievalDatabase`, with embeddings accepted only by
  retrieval-capable builders. See
  `docs/product/capability-separated-architecture.md`.

- Capability separation is complete. Rust now owns canonical `CorpusIndex`,
  derived `RetrievalIndex`, and graph-only `GraphEngine` components. Swift
  exposes `RetrievalDatabase`, `GraphDatabase`, and
  `GraphRetrievalDatabase`; graph-only APIs accept no vector settings or
  embeddings, and graph selections release automatically. Five interleaved
  pre-refactor/current benchmark pairs measured median p95 deltas of exact
  -0.57%, internal BM25 -0.05%, and hybrid +0.41%, passing the +3% gate. See
  `docs/product/reports/capability-separated-qualification-report.md`.

- Retrieval configuration now always builds exact-vector and BM25 state.
  Hybrid blending is selected directly per query with `alpha`; compact
  persistence may omit BM25 bytes and rebuild them from canonical chunk text
  on load. The graph aggregate ABI is version 7.
  Three interleaved before/after benchmark pairs measured exact -1.21%, BM25
  -2.61%, and hybrid +0.35%, passing the +3% p95 gate.

- M0 product authorization and the customer fixture contract/template are in
  place. The template deliberately contains no invented customer data.
- M1 implements canonical `RecordStore` values/identities, persisted corpus and
  generation identity, adaptive `CandidateScope`, scoped exact/BM25/hybrid
  retrieval, and ordered bulk hydration in `retrievalkit-core`.
- The 10K x 384d local development comparison measured graph-free p95 changes
  of exact +1.32%, BM25 +2.46%, and hybrid +2.66% versus pre-M1 using the median
  of three final p95 runs. Repeat the <=3% release gate on pinned hardware.
- M2 is authorized as a generic schema-driven package using domain-neutral
  synthetic conformance fixtures. Customer data is deferred acceptance evidence
  and must never become hard-coded schema behavior. Real-workload capacity and
  device claims remain provisional until private customer validation occurs.
- `retrievalkit-graph` M2 now provides record/chunk node schemas, explicit typed
  references and collections, validation policies, deterministic CSR adjacency,
  exact property seeds, bounded multi-step traversal, cycles, canonical paths,
  limits, cancellation, edge provenance, and graph-result projection into exact,
  BM25, and hybrid candidate scopes. It consumes one core index and does not
  expose a peer mutable handle.
- M3.1 now defines deterministic canonical schema JSON with a BLAKE3 identity
  and a bounded versioned graph snapshot payload tied to the core corpus and
  generation. Round-trip, deterministic encoding, corruption, truncation,
  trailing-byte, schema-hash, and stale-generation tests pass. M3.2 still needs
  to compose this payload with core persistence through atomic staging,
  validation, and activation.
- M3.2 now persists one composite immutable generation containing a complete
  core database, `schema.json`, and `graph.bin`. It checks sizes and BLAKE3
  digests, reopens and validates the staged core/graph pair, syncs it, and only
  then atomically activates it through the graph manifest. Read-only validation,
  repeat saves, abandoned staging, unsafe manifest paths, truncation, appended
  data, and same-size corruption have conformance coverage.
- M3.3 now serializes writers with an OS-released exclusive database lock.
  Loaders use a short shared open lock and retain a shared per-generation lease,
  preventing cleanup while a loaded index uses that generation. Locked saves
  remove abandoned staging and unleased superseded generations; an invalid
  existing manifest stops recovery without deleting snapshots.
- M3.4 injects failure at every pre-activation composite-save checkpoint and
  proves the previous manifest remains byte-identical and loadable. The M1 Max
  local persistence fixture measured save p95 98 ms and open/validate p95 10 ms
  for 2K records/8K edges. Five interleaved current/pre-M1 graph-free runs
  measured exact +0.44%, BM25 +1.01%, and hybrid +0.73%, passing the <=3% local
  gate. See `docs/product/reports/graph-m3-benchmark-report.md`; pinned-device
  release qualification is still required.
- M4.1 selected and proved the Swift aggregate packaging topology. The existing
  `retrievalkit-ffi` crate has an off-by-default `graph` feature; base
  `RetrievalKitFFI` remains graph-free, while `RetrievalKitGraphFFI` is built with the
  feature and contains the base retrieval symbols plus graph ABI symbols in one
  static library. Graph-enabled apps select the aggregate instead of linking
  both artifacts.
- M4.2a adds the aggregate native lifecycle boundary: a corpus-bound builder
  ingests generic canonical record/chunk batches, Rust decodes and validates the
  canonical schema, finalization consumes the builder into one graph handle,
  and that handle supports composite save/load/validation. JSON is limited to
  cold schema and ingestion paths; M4.3 query hot paths remain typed C ABI.
- M4.2b adds the `RetrievalKitGraph` Swift product with generic `Encodable` schema,
  canonical record, metadata, and chunk types. Actor-owned builder/index handles
  preserve native ownership; finalization consumes the builder. Swift integration
  tests cover schema marshaling, record ingestion, consumed-builder rejection,
  and composite save/validate/load.
- Full Swift linkage testing proved the base and aggregate static artifacts
  cannot coexist in one SwiftPM test bundle without duplicate core symbols.
  `RetrievalKitGraph` therefore lives in its own Swift package and is selected
  instead of the base package, enforcing the intended single-core topology.
- M4.3a adds typed C/Swift graph queries for node-ID seeds and bounded traversal,
  opaque native result ownership, materialized matches, limit/truncation traces,
  and atomic cancellation. Separate base and graph Swift package test suites
  pass without co-linking native artifacts. Equality seeds, path provenance,
  candidate projection, and composed rankers remain in M4.3b.
- M4.3b now retains each native graph result with its generation-bound projected
  candidate scope and exposes scoped exact, BM25, and hybrid ranking in Swift.
  Typed C ABI tests and Swift integration tests prove all three rankers return
  only the graph-selected record.
- M4.3c exposes typed Swift equality seeds for queryable string, integer, and
  boolean fields. Graph matches now materialize every canonical path edge with
  typed source/target IDs and relationship occurrence, schema-rule,
  source-record, source-field, inverse, and built-in provenance. Synthetic Swift
  integration coverage exercises forward and inverse traversal, equality seed
  scalar mappings, cancellation, and composed scoped ranking. The graph
  aggregate ABI version was 2 for this path-materialization contract.
- M4.4 transports the settled cross-wrapper graph error taxonomy through stable
  native status codes and maps it to `RetrievalKitGraphError` without wrapper-side
  validation. Swift integration tests cover invalid schema/identity, query-limit
  rejection, cancellation, corrupt/missing persistence, internal core failures,
  and builder consumption after both successful and failed finalization. The
  graph aggregate ABI version is now 3.
- M4.5 adds Swift graph-scoped metadata filter parity (`equals`, `notEquals`,
  `exists`, inclusive ranges, in-values, all, and any) for exact, BM25, and
  hybrid retrieval. Hybrid calls expose candidate limits plus query-time alpha
  over weighted normalized-score fusion; exact and hybrid hits materialize native
  filter decisions, and hybrid hits include complete ranking traces. Synthetic integration coverage
  proves graph scope and metadata filters intersect before ranking and that
  changing fusion weights changes the winning record without Swift-side
  ranking logic.
- M4.6 adds idempotent explicit closure for Swift graph builders, indexes,
  query-result/scope owners, and cancellation tokens, with `deinit` retained as
  fallback cleanup. Use-after-close maps to `graphUnavailable`. Result closure
  serializes against active scoped ranking calls; token closure waits for active
  native queries while still permitting concurrent `cancel()` to reach Rust.
  Swift stress coverage races 32 scoped searches with result closure and accepts
  only completed searches or typed closed-resource rejection.
- M4.7 moves Swift graph queries and scoped exact/BM25/hybrid ranking into
  detached native tasks admitted by a writer-preferring read/write gate.
  Immutable reads may overlap, including multiple rankers using one retained
  candidate scope. Composite save and index close remain exclusive, and a
  waiting writer blocks later readers. Deterministic gate tests prove concurrent
  reader admission, writer exclusion, and writer preference.
- M4.8 exposes typed query truncation reasons and candidate projection counts
  (`sourceNodes`, `resolvedChunks`) in Swift. The wrapper rejects only negative
  integer values that cannot safely cross C `size_t`; Rust remains the sole
  semantic validator. Tests cover normal and max-results-truncated queries,
  projection cardinality, and negative dimension/hop/top-k/candidate inputs.
- M4.9 adds the `RetrievalKitGraphQuickstart` executable to the separate graph
  Swift package. Its deterministic generic two-record fixture demonstrates
  schema creation, canonical ingestion, property-seeded traversal,
  metadata-filtered scoped hybrid ranking, composite save, and schema-owning
  reopen with no model, network, or customer data. CI runs it and verifies the
  expected `graph-retrieval` result and `1/1` projection.
- M4.10 adds `benchmarks/graph-conformance/v1/fixture.json`, a synthetic
  canonical-schema/record fixture consumed unchanged by Rust and Swift tests.
  Both implementations assert the same equality seed, bounded path sequence,
  projection counts, filtered exact ordering, and keyword result. Swift graph
  transport types are now `Codable`, enabling the future Python wrapper to use
  this exact contract. `scripts/verify-swift-graph-wrapper.sh` proves base symbol
  neutrality, aggregate core+graph symbols, separate package linkage, tests,
  and exact quickstart output; manual CI runs the script after full Apple builds.
- The earlier M4 Swift qualification is complete. Full macOS, iOS device, and
  iOS simulator
  XCFramework builds pass for both base and aggregate artifacts, including
  symbol-neutrality and separate-linkage verification. The clean build exposed
  and fixed a Bash 3 empty-feature-array failure in the base build script. See
  `docs/product/reports/graph-m4-swift-qualification-report.md`.
- M5 adds the separate optional `retrievalkit-graph` Python distribution over the
  same Rust `GraphDatabase` and `GraphRetrievalDatabase`. The base `retrievalkit`
  distribution remains graph-free. The optional aggregate exposes typed graph
  models, graph-only and combined builders, and separate `database.graph` and
  `database.retrieval` query namespaces. Synthetic Python tests and a runnable
  graph-retrieval quickstart cover the public surface. As with the Swift native
  artifacts, applications select one compatible distribution per process.
  Customer validation remains deferred.

## Current Size, RAM, and Speed Goal

- Hard goal: roughly `24K` vectors plus required local data must stay under
  `20 MB` total persisted size. This means vectors plus chunk metadata, required
  display/retrieval data, BM25 data if enabled, headers, tombstones, and version
  data. Do not treat the target as vector-only.
- RAM matters too. Prefer compact in-memory layouts and avoid loading full text
  or broad string-heavy structures on the hot retrieval path unless benchmarks
  prove the cost is acceptable.
- Speed remains a first-class requirement. Size reductions are not useful if
  they push target-device retrieval outside the low-latency budget.
- `24K x 384d` with `I8ScalarQuantized` is the first practical compact target.
  With current binary persistence, it saves at `12.772 MiB` including vectors,
  chunks, BM25, tombstones, and manifest.
- `24K x 384d` with `F16` is close but over budget at `21.470 MiB`.
- `24K x 768d` with `I8ScalarQuantized` is also close but over budget at
  `21.561 MiB`; vector payload alone is about `17.7 MiB`.
- With compact vector-only persistence (`persist_bm25=false`) and compact chunk
  metadata encoding, `24K x 768d I8ScalarQuantized` filtered measured locally at
  `19.581 MiB`, with `chunks.bin` down to about `1.888 MiB`.
- `24K x 1536d` cannot fit under `20 MB` with current I8 storage.
- Full chunk text should likely stay outside the hot index if the `20 MB` target
  includes user-visible data.
- Current size/speed report: `docs/product/size-speed-report.md`.

Approximate vector-only sizes for `24K` vectors:

| dim | F32 | F16 | I8 | Binary |
|---:|---:|---:|---:|---:|
| 384 | ~35.2 MiB | ~17.6 MiB | ~8.9 MiB | ~1.1 MiB |
| 768 | ~70.3 MiB | ~35.2 MiB | ~17.7 MiB | ~2.2 MiB |
| 1536 | ~140.6 MiB | ~70.3 MiB | ~35.3 MiB | ~4.4 MiB |

## Encoding Decisions

- `F32` remains correctness ground truth.
- `F16` is a good quality-preserving size reduction and currently has full
  synthetic recall against F32 in the benchmark runs.
- `BF16` is supported but was much slower than F32/F16 on the current machine.
  Do not choose it for speed without device-specific benchmarks.
- `I8ScalarQuantized` is implemented as symmetric per-vector quantization:
  `i8` values plus one `f32` scale per vector.
- I8 synthetic recall passed the current gate. Exact full-scan latency is now
  faster than F32/F16 on the current Apple M1 Max development machine because
  RetrievalKit uses an AArch64 `dotprod` C shim for I8 dot products when runtime
  feature detection reports support. SimSIMD still reports
  `neon,neon_f16,dynamic`, not `neon_i8`, because this machine has
  `FEAT_DotProd=1` but `FEAT_I8MM=0`; keep RetrievalKit's guarded fallback path.
- `BinaryQuantized` is a future size-constrained candidate retrieval option.
  It may be necessary for `768d + data <20 MB`, but needs recall benchmarking
  before use.

## Recent Benchmark Takeaways

Social Network real-data MiniLM benchmark:

- Built `target/examples/social-network-index-minilm` from the real Social
  Network fixture using Core ML `sentence-transformers/all-MiniLM-L6-v2`,
  `seq=256`, `384d`, cosine, and `I8ScalarQuantized`.
- Persisted index: `28,650` chunks, `31.346 MiB`.
- Query fixture: `target/examples/social-network-minilm-queries.json`.
- Swift exact vector search over the MiniLM index measured `0.470 ms` average,
  `0.466 ms` p50, `0.497 ms` p95, and `0.535 ms` p99 over `750` measured
  queries with `top_k=5`.
- Core ML MiniLM query embedding measured separately at `3.057 ms` average,
  `2.973 ms` p50, `3.545 ms` p95, and `5.493 ms` p99.
- Approximate embedding + Swift search: `3.527 ms` average, `3.439 ms` p50,
  `4.042 ms` p95, and `6.028 ms` p99. This is close to Moss's published
  `4.3 ms` p95, but should be replaced by a single Swift end-to-end benchmark
  once Swift-side tokenization/model execution is wired in.
- Source reports:
  `docs/product/reports/social-network-end-to-end-benchmark-report.md` and
  `docs/product/reports/social-network-minilm-swift-search-report.md`.

Local Rust CLI hybrid work after BM25/runtime optimizations:

- BM25 keyword search is no longer the main hybrid bottleneck for the current
  synthetic V1 benchmark. Runtime BM25 uses hash-backed postings, bounded
  top-k selection, metadata allowlists for filtered search, and cached active
  document frequency per term.
- Filtered hybrid search is fast when metadata filters narrow candidate sets.
  On `10K x 384d I8ScalarQuantized`, `top_k=10`, `100` queries:
  `filter_every=100` measured around `0.03-0.05 ms`, `filter_every=10`
  around `0.10-0.34 ms`, and `filter_every=2` around `0.49-1.58 ms`
  depending on candidate limits and fusion mode.
- Unfiltered hybrid latency is dominated by vector candidate count, not BM25.
  In the same benchmark, `10` vector candidates produced roughly
  `0.44-0.57 ms`; `25` vector candidates roughly `1.17-1.30 ms`; `50`
  vector candidates roughly `2.5 ms+`.
- Keyword candidate count is comparatively cheap after the BM25 changes.
- Hybrid candidate limits are intentionally public and per-query:
  `HybridQuery::with_candidate_limits(vector_top_k, keyword_top_k)`.
  The V1 fixture compares each pair with both a same-encoding `100/100`
  reference and F32 `100/100`. Keep `50/50` as the default because it is the
  smallest tested pair that meets the `0.95` I8-versus-F32 recall gate; callers
  can still choose smaller limits when latency matters more than overlap.

Physical-device validation using the iOS `Device` mode has now run on an
iPhone with iOS 26.5. Source report:
`docs/product/reports/iphone-device-validation-i8-report.md`.

| dim | filter | avg | p95 | load | persisted | observed RSS after load |
|---:|:---|---:|---:|---:|---:|---:|
| 384 | none | 0.471 ms | 0.506 ms | 22.54 ms | 12.772 MiB | 180.33 MiB |
| 384 | 1/10 | 0.111 ms | 0.173 ms | 28.68 ms | 12.910 MiB | 214.89 MiB |
| 768 | none | 0.640 ms | 0.658 ms | 25.62 ms | 21.561 MiB | 255.34 MiB |
| 768 | 1/10 | 0.167 ms | 0.256 ms | 27.83 ms | 21.699 MiB | 249.27 MiB |

Treat the RSS values as process-level sequential-run observations, not isolated
per-index memory footprints. Add one-scenario-per-launch device presets before
making memory-budget decisions from RSS.

For `24K` vectors, `top_k=10`, `200` synthetic queries after the AArch64 I8
dotprod backend, late result materialization, active-offset scans, and the
unfiltered I8 fast path:

| dim | F32 avg | F16 avg | I8 avg | I8 recall@10 vs F32 |
|---:|---:|---:|---:|---:|
| 384 | ~2.5 ms | ~2.6 ms | ~0.8 ms | 0.9895 |
| 768 | ~5.4 ms | ~5.6 ms | ~1.0 ms | 0.9920 |

For isolated scoring kernels, `50K x 768d I8` measured about `1.25 ms` average
versus about `9.1 ms` for F32 on the same machine.

With an indexed equality filter at roughly `1/10` selectivity, `24K` vectors,
`top_k=10`, and `200` synthetic queries, filtered `I8ScalarQuantized` measured
about `0.51 ms` average for `384d` and `0.81 ms` average for `768d`.

## Deferred Exploration

- `docs/research/turbovec-notes.md` captures ideas from `RyanCodrai/turbovec`.
  Useful ideas include filter-aware scoring loops, explicit cache warmup,
  strict binary format validation, and benchmark/debug counters. Do not adopt
  its approximate TurboQuant index as the V1 primary retrieval engine.
- Optional rerank vector store for compressed encodings is deferred.
- Revisit reranking only after real-data benchmarks show I8 needs higher final
  quality.
- If explored later, measure disk size, memory, recall, and latency for:
  `I8`, `I8 + F16 rerank`, and `I8 + F32 rerank`.
- Reranking adds small CPU cost for `top_k * overfetch` candidates but
  significant memory/disk cost from the second vector store.

## Python Wrapper Context

- `docs/agents/python.md` defines Python wrapper guidance. Python is currently
  an internal developer wrapper before the public Swift wrapper is finalized.
- `crates/retrievalkit-python` exposes a thin PyO3 module. Its default feature set
  calls only `retrievalkit-core`; the `graph` feature adds the aggregate graph
  bindings.
- `wrappers/python` contains the graph-free `retrievalkit` maturin package. The
  public API is Pythonic:
  `Index.add(documents=[...])`, `Index.search(embedding, limit=10, where=...)`,
  `Index.keyword_search(...)`, `Index.save(...)`, `Index.load(...)`, and
  `delete_document(...)`.
- The canonical Python capability API now also exposes
  `RetrievalDatabaseBuilder`, `RetrievalDatabase`, and `database.retrieval`,
  matching Rust and Swift ownership while using Pythonic mappings, keyword
  arguments, snake_case methods, and context managers. Capability-neutral
  records and chunks are passed separately from embeddings keyed by chunk key.
- Embeddings are caller-provided. `search_text(index, text, embed=...)` is only
  a convenience helper that calls the supplied provider, validates one returned
  query vector, then calls vector search.
- `wrappers/python-graph` contains the optional `retrievalkit-graph` aggregate.
  `GraphDatabaseBuilder` accepts graph-only records without embeddings.
  `GraphRetrievalDatabaseBuilder` takes separate `graph=` and `retrieval=`
  configurations, while built databases expose `database.graph` and
  `database.retrieval`. The Python layer only marshals typed dictionaries;
  schema validation, traversal, ranking, persistence, and hydration remain in
  Rust.
- The Python graph surface is idiomatic and lifecycle-safe: builders accept
  iterables, scoped retrieval prefers `within=`, databases/selections support
  `close()` and context managers, `TimestampMillis` preserves timestamp typing,
  and graph queries expose cooperative cancellation plus second-based timeouts.
- The formal Rust/Swift/Python matrix and remaining result/performance follow-up
  work are recorded in
  `docs/product/reports/cross-language-wrapper-parity-audit.md`.

## EmbeddingKit Context

- EmbeddingKit lives separately from RetrievalKit under `wrappers/swift/EmbeddingKit`.
  RetrievalKit still accepts caller-provided embeddings and does not depend on an
  embedding runtime.
- Core ML model conversion is intentionally outside the Swift package. The
  generic conversion script is `scripts/embedding/convert-embedding-coreml.py`
  with a BGE compatibility wrapper at
  `scripts/embedding/convert-bge-small-coreml.py`. The process is documented in
  `docs/product/embedding-model-conversion.md`.
- Generated model artifacts should stay under
  `target/embedding-models/bge-small-en-v1.5/` and should not be committed by
  default.

## Ingestion Context

- Generic text chunking lives in the separate Rust `retrievalkit-ingest` crate so
  retrieval remains isolated in `retrievalkit-core`.
- Fixed and sentence-aware strategies use Unicode-character limits and overlap;
  returned ranges are UTF-8 byte offsets into the original text.
- Swift exposes chunking through the separate `RetrievalKitIngest` product and
  Python through `retrievalkit.ingest`. Both call the same Rust implementation.
  Tokenizers differ by model, so exact token counting remains provider-owned.
- The optional Swift `RetrievalKitPipeline` package and Python
  `retrievalkit.pipeline` module compose chunking, embedding, document upsert, and
  hybrid text search. They validate all embeddings before upsert, so provider
  failures leave the previous document version unchanged.
- The pipeline layer owns `DocumentChunker` in Swift and Python. Its default is
  sentence-aware Rust chunking at 500 characters with 50 characters of overlap;
  applications can override it for document-aware splitting. Pipeline validates
  custom text, ordering, and UTF-8 source ranges before embedding, so overrides
  do not weaken downstream index metadata.
- Token-aware pipeline chunking is implemented. Swift automatically uses an
  embedder's `TextTokenCounter` and `maxInputTokens`; Python accepts
  `count_tokens` and `max_tokens`. Oversized chunks are recursively subdivided
  while preserving original UTF-8 offsets. Providers without tokenizer access
  keep the character-based default.

## Completed Optimizations

- Crash-safe transactional persistence is implemented in the Rust core. Format
  V2 stores immutable generations under `.snapshots`, syncs and validates a new
  generation before atomically publishing `manifest.json`, and cleans stale
  generations after success. V1 root-file indexes remain readable and migrate
  on their next save. An OS-released file lock serializes cross-process saves.
  Failure-injection tests cover every pre-publication stage and lock contention.
- Explicit compaction is implemented in Rust and exposed through Swift and
  Python. It atomically rebuilds vectors, chunks, BM25, metadata filters, and ID
  lookups using only active chunks, preserves active IDs and monotonic ID
  allocation, reports estimated memory reclaimed, and is a cheap no-op without
  tombstones. Saving afterward publishes the compacted snapshot.

- Late result materialization is implemented for exact vector search. The hot
  loop keeps only `chunk_id`, `offset`, and score, then builds final `SearchHit`
  values after sorting the winning candidates.
- Active-offset scanning is implemented for unfiltered exact vector search. The
  index keeps a derived `active_offsets` list so tombstoned rows are not scanned
  after upserts, deletes, or persistence reload.
- A specialized unfiltered `I8ScalarQuantized` search path is implemented. It
  borrows the contiguous I8 values and scale arrays directly, skips generic
  filter checks, and keeps final result materialization after top-k selection.
- The specialized I8 search path now also handles filtered vector searches. It
  uses metadata candidate offsets when available, still verifies the actual
  filter predicate for correctness, and falls back to active-offset scans for
  filter shapes that cannot be fully narrowed by the metadata index.
- BM25 runtime search is optimized for V1 hybrid retrieval. The in-memory index
  uses hash-backed term lookups and postings, bounded top-k selection, filtered
  metadata allowlists, O(1) active average document length, and cached active
  document frequency. Persistence remains deterministic through the existing
  sorted binary format.
- High-level hybrid search defaults to weighted normalized fusion with
  `alpha=0.6`. Internal Rust benchmarks may still exercise RRF. Result traces
  expose vector/keyword ranks, raw scores, normalized scores, matched terms,
  and fusion configuration.
- Hybrid candidate limits are exposed through the Rust public API with
  `HybridQuery::with_candidate_limits(vector_top_k, keyword_top_k)`.
- The CLI matrix benchmark can now vary filter selectivity and hybrid candidate
  limits with `--filter-every-values`, `--vector-candidates`, and
  `--keyword-candidates`.
- A `retrievalkit-ffi` crate now exposes `retrievalkit_bench_synthetic_json` and
  `retrievalkit_string_free` for Swift/macOS/iOS benchmark harnesses. The default
  benchmark runs `24K` chunks, `384d` and `768d`, `f32`/`f16`/`i8`, and both
  unfiltered and `filter_every=10` filtered searches. FFI benchmark rows now
  also include persistence save time, load time, persisted file sizes, and
  post-load search latency by default. `persist_bm25=false` measures a compact
  vector-only persistence profile.
- A SwiftPM macOS command-line harness exists at
  `wrappers/swift/RetrievalKitBench`. It links `retrievalkit-ffi`, supports
  `--small-smoke`, `--config`, and `--config-file`, and successfully ran the
  full default FFI benchmark locally.
- `scripts/build-xcframework.sh` packages `retrievalkit-ffi` as
  `target/apple/RetrievalKitFFI.xcframework`. The full Apple package is verified
  locally with `ios-arm64`, `ios-arm64-simulator`, and `macos-arm64` slices.
  The iOS simulator slice is arm64-only; `x86_64-apple-ios` is intentionally
  not used.
- A minimal SwiftUI iOS benchmark app exists at
  `wrappers/swift/RetrievalKitIOSBench`. It links the local XCFramework, exposes
  smoke, device-validation, full default, and compact vector-only benchmark
  buttons, and the generic iOS Simulator build succeeds locally. The
  device-validation mode runs `24K` chunks, `384d`/`768d`, `i8`, filtered and
  unfiltered, persistence enabled, and `include_recall=false` so F32
  ground-truth indexes do not inflate RSS.
- `chunks.bin` now writes a v2 payload with a metadata field dictionary and
  compact varint integer/timestamp metadata values. The loader still accepts the
  older v1 chunk payload.
- `chunks.bin` and `bm25.bin` are now zstd-compressed at rest by default.
  Manifest fields record compression type and uncompressed byte counts, and load
  transparently decompresses before rebuilding in-memory structures.
- The FFI benchmark report is now schema version `2`. On Apple platforms it
  includes Mach task RSS snapshots for the whole report, per-run build/search
  phases, and persistence save/load/post-load-search phases.

## Likely Next Tasks

The active slice is `v0.1.0` release qualification and distribution. Benchmark
Phases 0–7, persistence hardening, wrapper concurrency, memory qualification,
and the release-candidate implementation are complete. Do not begin another
benchmark phase or resume physical-device stress work without a new explicit
owner task.

Complete the release gates in this order:

1. Select the final clean release revision and rebuild the complete candidate.
   The recorded candidate predates later README changes and must not be treated
   as evidence for the final revision.
2. Provision passing Phase 7 scheduled/full and controlled release results for
   that same revision. Release qualification consumes pre-collected evidence
   only; it authorizes no device command and exposes no 100K lane.
3. Authorize README claim handling as either historical frozen-revision
   observations or release-revision claims backed by new accepted evidence.
4. Add the owner publication authorization binding the revision, legal
   approvals, Phase 7 results, claim mode, and owner identity.
5. Complete the approval checklist, create a verified signed `v0.1.0` tag,
   obtain protected-environment approval, run the guarded publication workflow,
   and verify fresh remote SwiftPM and PyPI consumers.

See `docs/product/release-process.md` and
`docs/product/release-approval-checklist.md`. Until every gate passes, the
release remains a local candidate and external publication stays blocked.

Optional post-release work, ordered by evidence need:

- qualify F16/F32 and compact/compaction headroom on older supported Apple
  devices before generalizing the existing iPhone 17 Pro Max budgets;
- expand external quality evaluation with BEIR/TREC-compatible runs, pooled
  blind judgments, and anonymized application queries;
- benchmark the I8 dot-product path on devices that may not expose `dotprod`;
- explore parallel exact scanning only after measured CPU pressure, and begin
  ANN research only if exact search still misses the frozen latency/recall
  targets.
