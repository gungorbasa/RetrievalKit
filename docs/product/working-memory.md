# RetrievalKit Working Memory

This file captures active implementation context that should survive chat
changes. Keep it short. Delete or move notes once they become irrelevant,
implemented, or superseded by the product spec.

## Current Workflow

- 2026-08-01 browser retrieval release decision: include
  `@gungorbasa/retrievalkit-browser` in the v0.1.0 npm inventory alongside the
  independent browser embedding package. Its closed tarball must contain the
  Worker wrapper plus qualified portable and SIMD128 `wasm-bindgen` tiers,
  pass fresh local-install resolution, participate in two-root byte comparison,
  and flow through the same protected authorization, provenance, attestation,
  and npm publication gates. The source package remains private to prevent
  accidental publication. The owner subsequently authorized its one-time npm
  bootstrap: the reviewed non-SDK `0.0.0-bootstrap.0` placeholder now resolves
  anonymously and the package trusts only `gungorbasa/RetrievalKit`,
  `publish-release.yml`, and the protected `npm` environment with publish
  permission. The temporary local npm credential was revoked. This setup does
  not authorize candidate/release workflows, tagging, or v0.1.0 publication.
- 2026-08-01 Android v0.1.0 release decision: no physical Android device is
  currently available, so Android API 24+ arm64-v8a ships as an explicit
  preview. Retain cross-compilation, base/graph/embedding AAR packaging, closed
  inventory, ABI/architecture, JVM/JNI-contract, and fresh consumer dependency
  resolution/compilation release checks. Live-device model acquisition,
  inference, lifecycle, memory, thermal behavior, offline restart, device
  compatibility, and performance remain unqualified and are deferred until a
  device is available. Missing live-device evidence is not a v0.1.0
  publication blocker. Never claim an Android device inference pass or make
  production, performance, or device-compatibility claims beyond existing
  evidence.
- 2026-07-30 website demo direction: the browser/WASM retrieval package and
  independent browser embedding Worker are implemented, desktop-qualified,
  included in the v0.1.0 inventory under the later 2026-08-01 decision, and
  still unpublished. The public website demo uses curated first-party documents,
  accepts arbitrary visitor questions, and runs local browser embedding,
  RetrievalKit WASM search, and a grounded browser SLM. Suggested questions and
  pre-rendered marketing answers may advertise the experience, but interactive
  results must use the same live local pipeline. Python or the RetrievalKit CLI
  prepares a versioned, checksummed corpus pack containing canonical chunks,
  source offsets, precomputed document embeddings, metadata, graph schema and
  records, aliases, and evaluation inputs. A clean browser session validates
  that pack, builds the real in-memory WASM database through public builders,
  and creates query embeddings locally. Cross-platform portable byte snapshots
  are explicitly deferred: future design must distinguish distribution
  snapshots from native transactional persistence and cover versioning,
  migrations, integrity/signing, hostile-input limits, compression, size,
  memory, startup, model compatibility, platform import, browser caching, and
  cross-target qualification. Do not claim portable database snapshots until
  that work is specified, implemented, benchmarked, and qualified. Exact
  source highlighting requires a verbatim citation validated against a
  retrieved chunk and mapped through retained source offsets. The website
  orchestration now exists in the private website repository. The production
  demo at `https://retrievalkit-docs.gungorbasa.chatgpt.site/demo/` ships the
  NASA Apollo 11 corpus pack and builds one combined graph-retrieval database
  through RetrievalKit WASM. Vector mode uses MiniLM retrieval plus a grounded
  Qwen answer. Graph plans are validated and executed by RetrievalKit WASM,
  then Qwen answers from graph-selected source paragraphs. Combined mode uses a
  validated Qwen path ending at `Passage`, graph projection, MiniLM ranking
  within that selection, and Qwen fact selection; the app renders exact NASA
  source sentences rather than generated combined-answer prose. The full
  production pipeline is qualified on the tested Apple-silicon Chromium/WebGPU
  environment. The website enforces 64 MiB client, 1 MiB corpus, and 460 MiB
  combined-model limits; Qwen reports approximately 1.4 GB required GPU
  memory. Safari, Firefox, and physical mobile devices remain unqualified.
- 2026-07-30 website demo integration exposed and fixed two browser-boundary
  defects. Internally tagged WASM graph seed variants now deserialize their
  fields from camelCase (`nodeType`) through `rename_all_fields`, with a
  regression test. The browser embedding package now gives ONNX Runtime
  explicit provider-specific `.mjs` and `.wasm` asset URLs so bundlers such as
  Vite rewrite the hashed runtime assets correctly. Live validation loaded
  MiniLM on WebGPU, ran exact RetrievalKit WASM vector search, and executed a
  `NEXT_EVENT` graph traversal with exact source highlighting.
- 2026-07-28 website repository boundary: the OpenAI Sites-hosted public docs
  source moved out of this public SDK monorepo into the private
  `gungorbasa/RetrievalKit-Website` repository with its subtree history. The
  existing Sites project ID and public URL remain unchanged. This SDK repo
  continues to own deterministic source-preview generation; invoke
  `scripts/release/build_source_preview.py` with an explicit `--site-root`
  pointing at a website checkout. The website repo independently validates the
  checked-in archive and `app/release.ts` checksum. Never recreate `website/`
  here: all site content, design, build, hosting, and deployment changes belong
  in `gungorbasa/RetrievalKit-Website`.
- 2026-07-26 release signing identity: Maven primary artifacts use the dedicated
  two-year `RetrievalKit Release <gungorbasa@users.noreply.github.com>` RSA-4096
  key with fingerprint
  `0E82 F1A5 487A 4EF3 CCF1 ED6C 3932 66CD 4DD1 58ED`, expiring
  2028-07-25. The public key is
  `release/retrievalkit-release-signing-key.asc` and is published to
  keys.openpgp.org. The private key stays in the local GnuPG keyring; its
  passphrase is in macOS Keychain service `RetrievalKit-Maven-GPG` and will be
  copied only to protected GitHub environment secrets. Never commit or print
  the private key or passphrase.
- 2026-07-26 Phase B registry-package decision: npm rejected the selected
  unscoped `retrievalkit` base name as too similar to the existing
  `retrieval-kit` package before publishing any bytes. The owner then selected
  `@gungorbasa/retrievalkit` and `@gungorbasa/retrievalkit-graph`; the Maven
  group remains `io.github.gungorbasa`. Checked-in Node packages remain private
  to prevent an accidental workspace publish, while fail-closed assemblers
  require exactly the selected public identities. The two-root candidate
  workflow now builds, inspects, and byte-compares isolated base, graph, and
  embedding artifacts for the supported Python, Node/browser, JVM, and Android
  targets and integrates them into the same closed Swift/Python/Node/Kotlin
  bundle. Protected publication jobs consume those exact bytes: npm uses OIDC
  trusted publishing with provenance, and Maven signs the 24 authorized primary
  artifacts before Central Portal upload. On
  2026-07-26 the base and graph names received the reviewed non-release
  `0.0.0-bootstrap.0` placeholder and were configured to trust
  `gungorbasa/RetrievalKit`, `publish-release.yml`, and the protected `npm`
  environment. On 2026-08-01, `@gungorbasa/retrievalkit-embedding` and
  `@gungorbasa/retrievalkit-browser-embedding` received the same reviewed
  placeholder and exact production publisher with publish-only permission.
  The fifth approved identity, `@gungorbasa/retrievalkit-browser`, received the
  same reviewed placeholder and exact publisher on 2026-08-01. Its placeholder
  integrity is
  `sha512-0LFxyM0tF99zVA9sVhUpt6F6KzkSdMd2Djj4FtdvxJIG1A2JwUH/pEIoT3/7hYBLkD6O3ZkHj3Y4S5RKAgg/Ig==`.
  All five public records resolve anonymously, and v0.1.0 remains unused. The
  local npm bootstrap credential was removed afterward. These placeholders are
  ownership and publisher setup records, not v0.1.0 SDK releases. Creating a
  new npm trusted publisher now requires npm CLI 11.15.0 or later plus an
  explicit publish permission; npm 11.12.1 returns an empty HTTP 400 because it
  predates that required field.
- 2026-08-01 PyPI owner setup: `retrievalkit`, `retrievalkit-graph`, and
  `retrievalkit-embedding` each received a reviewed non-SDK `0.0.0a0`
  placeholder. All three projects now trust
  only `gungorbasa/RetrievalKit`, `publish-release.yml`, and the protected
  `pypi` environment. Separate temporary bootstrap workflows were required
  because PyPI rejects one identical pending-publisher identity for multiple
  uncreated project names; those temporary publishers and workflows were
  removed after the projects existed. All three public records resolve
  anonymously, and v0.1.0 remains unused. The embedding placeholder was
  published by successful GitHub Actions run `30690365488`; temporary `main`
  access to the `pypi` environment was removed afterward, leaving only the
  `v*` tag policy.
- 2026-07-26 Maven Central owner setup: the automatically provisioned
  `io.github.gungorbasa` namespace is verified. A six-month Portal user token
  named `RetrievalKit GitHub Actions` is installed as
  `MAVEN_CENTRAL_USERNAME` and `MAVEN_CENTRAL_PASSWORD` in the protected
  `maven` environment; rotate it before 2027-01-26. All three Maven GPG secrets
  are also present, the public key is distributed, and the environment accepts
  only `v*` tags. The signed tag and provisioned Phase 7 evidence remain
  external gates.
- 2026-07-26 Phase B publication-authorization decision: no completed
  authorization file is committed to the release revision. The exact signed-tag
  candidate, candidate/scheduled/release workflow run IDs, passing Phase 7
  result hashes, and bundle inventory/checksum/manifest hashes are closed
  before approval. The protected GitHub `release` environment is the authority;
  after its required-reviewer approval, the publication job records the GitHub
  approval event and exact workflow run/tag/commit in a runtime
  authorization-provenance record, validates it against the unchanged
  candidate, retains and attests it, and attaches it to the GitHub Release.
  Approval events predating the current workflow-run attempt are rejected;
  unprotected environments fail closed because they produce no required-review
  event. On 2026-07-26 the existing GitHub repository became public, and the
  `release`, `pypi`, `npm`, and `maven` environments were restricted to `v*`
  tags. `release` requires the sole owner as reviewer with self-review enabled
  to avoid deadlock; registry jobs depend on that approval. A signed tag,
  provisioned passing release gates, and registry-side trust/credentials are
  still required.
- 2026-07-26 Phase A DX implementation: the clean-source onboarding harness now
  measures Python, Swift, Node.js, and Kotlin with schema-v2 evidence; it runs
  monthly and on demand. Swift quickstarts use a checked entrypoint that reports
  the exact XCFramework build and retry commands. TypeScript supports maintained
  Node.js 22.13+ and 24 LTS ranges through one tested policy shared by preflight
  and package engines. Runnable Node.js 22 quickstarts use explicit
  `try`/`finally` lifecycle cleanup; `await using` remains an optional Node.js 24
  convenience. The refreshed development dependency tree has zero npm audit
  findings. Kotlin builds require JDK 17 while published bytecode targets Java
  11+, and preflight validates the exact `JAVA_HOME` binary with actionable
  recovery. Android preflight reads the NDK `source.properties` revision and
  requires major version 26; linker presence alone is not sufficient evidence
  of a supported NDK. Apple symbol verification captures the readable `nm`
  output before checking required core/graph exports because Rust 1.97 LLVM 22
  archives can make Apple LLVM 21 `nm` return nonzero after it has emitted the
  RetrievalKit FFI symbols. The public Sites source now includes Swift,
  complete language snippets, responsive mobile actions, and a custom 404;
  this Phase A work is deployed to
  `https://retrievalkit-docs.gungorbasa.chatgpt.site`.
- 2026-07-26 Swift distribution decision: the root `Package.swift` is the only
  public Swift manifest. It exposes `RetrievalKit`, `RetrievalKitGraph`,
  `EmbeddingKit`, and `RetrievalKitPipeline` over the single
  `RetrievalKitGraphFFI` aggregate, so applications may select base, graph, or
  both products from one repository and version. This intentionally makes a
  base-only Swift consumer download the graph-capable binary. The internal
  graph-free `RetrievalKitFFI` artifact and repository-local component package
  remain for isolation and symbol-neutrality qualification, not publication.
  `TextChunker` is part of `RetrievalKit`; `RetrievalKitIngest` is no longer a
  separate Swift product. Public release is no longer blocked on a standalone
  graph repository; owner authorization, signed-tag, claims, and Phase 7 gates
  remain.
- 2026-07-25 wrapper onboarding qualification baseline: CI now exercises Node/macOS
  arm64, Kotlin/JVM/macOS arm64 with JDK 17, Android arm64-v8a, and explicitly
  non-release Python source portability on Windows. Wrapper build entrypoints
  run actionable Python, Node, or Kotlin/JDK/NDK preflights. The manual
  onboarding workflow originally recorded clean-export time-to-first-result
  evidence and environment/cache caveats for Python, Node, and Kotlin; the
  2026-07-26 Phase A update above supersedes that wrapper set and cadence.
  Searchable public
  source-preview docs are deployed at
  `https://retrievalkit-docs.gungorbasa.chatgpt.site`; their versioned Python
  bundle is built from commit `d73eaf6`, carries SHA-256
  `cde4d966c4bf39ea372b6a871ae9638ca1173383e98658df52c4028413f026c0`
  on the page, and
  passed the documented graph quickstart from a fresh extraction. This is a
  narrow source-preview path, not registry publication or expanded platform
  support. Its earlier standalone graph Swift publication blocker was
  superseded by the 2026-07-26 unified-package decision above.
- 2026-07-25 Swift/Rust boundary decision: Swift is the first logic-free wrapper
  implementation and establishes the contract for later language wrappers.
  Rust owns progressive dimension inference, pending graph-only records,
  canonical hidden identity derivation, embedding validation, and public
  `alpha` semantics. Swift owns only idiomatic API shape, marshaling,
  handle/async/cancellation lifetime, and typed error presentation. The common
  retrieval upsert and every query path use typed C ABI values with contiguous
  float buffers; JSON remains limited to cold schema-rich graph ingestion. Do
  not add wrapper-side fallbacks or duplicate these rules in Python/Node.
  Result hydration uses fixed hit arrays, one packed UTF-8 arena, one flat
  metadata-entry array, and one flat matched-term range array for BM25/hybrid;
  Swift validates and decodes ranges without owning retrieval semantics. Exact,
  BM25, and hybrid hits expose effective metadata; hybrid traces expose one
  buffer-level alpha and no constant filter-match field. Alpha endpoints skip
  candidate generation for the zero-weight source. Public wrapper FFI no longer
  exposes generic fusion/RRF. Native ABI types/constants use the
  `RetrievalKit`/`RETRIEVALKIT_` product prefixes; the stale pre-rename
  `Vk`/`VK_` names are removed. The graph aggregate ABI version for this
  contract is 12. Remaining boundary performance debt is explicit: the Rust
  query object still owns one vector copy, and advanced multi-document graph
  upsert serializes embeddings on the cold JSON path. Optimize those only with
  measured, compatibility-tested ABI changes; they are not wrapper semantics.
- 2026-07-23 owner sequencing decision: pause release qualification and all
  new benchmark work until the SDK implementation is finalized. Do not rebuild
  the release candidate, provision Phase 7 scheduled/release evidence, resume
  device work, or add another performance/quality benchmark without a new
  explicit owner task. The active slice is SDK API, behavior, wrapper parity,
  and developer-experience completion. The already-collected evidence and
  fail-closed publication gates remain intact for later resumption.
- 2026-07-24 SDK-finalization approach: combine parity-first closure with
  developer-experience work. Contract dependencies set the order, while native
  API naming, errors, types, examples, and docs close inside every slice rather
  than waiting for a final polish pass. The planned order is canonical
  result/trace parity, Python stable candidate projection, retrieval-only
  cross-wrapper conformance, typed Python graph query transport, then a bounded
  public API/DX closure audit. Design record:
  `~/.gstack/projects/gungorbasa-VectorKit/gungorbasa-main-design-20260724-103816.md`.
- 2026-07-24 public ingestion implementation: Swift now uses
  capability-specific progressive APIs and keeps record/document ownership
  internal. Retrieval-only callers
  upsert a `Document` with its caller-produced embedding; graph-only callers
  upsert a `Record`; the common combined path upserts a `Record` with its
  embedding and lets RetrievalKit create and link the searchable document
  internally. An advanced combined overload accepts multiple
  `EmbeddedDocument` values, each with a stable public document ID, text, and
  caller-produced embedding. Do not expose `ChunkKey`, keyed embedding maps, or
  explicit record/document linking in the common public path. Internally retain
  separate stable record and searchable-document identities so text splitting
  does not inflate or destabilize the graph. The first embedding now infers the
  database dimension; direct `search` overloads cover vector-only, BM25-only,
  and `alpha`-weighted hybrid queries. Explicit-dimension progressive builders,
  keyed embedding maps, and public chunk construction were removed from the
  Swift database APIs; the lower-level mutable `VectorIndex` remains a separate
  compatibility surface.
- 2026-07-23: the root README now presents RetrievalKit as one hybrid-search
  product. Canonical, runnable Project Apollo walkthroughs live in
  `docs/guides/swift.md` and `docs/guides/python.md`; wrapper READMEs remain
  lower-level API/build references. Graph scope is documented as candidate
  selection before the shared semantic + BM25 ranker, never as a third scoring
  signal. The checked-in Swift and Python quickstarts use the same scenario and
  exact outputs. The release Swift consumer smoke script now prefixes temporary
  package directories with `Consumer-` so a consumer directory cannot collide
  with the checked-out `RetrievalKit` package identity. Its negative
  base-plus-graph check forces both static aggregates to load, ensuring their
  duplicate native symbols remain an explicit mutual-exclusion guard even when
  the linker would otherwise dead-strip unreferenced archive members.
- 2026-07-23: the Xcode 26.3 two-root release build produced byte-identical
  canonical Apple archives. Current SwiftPM checksums are
  `fcc3c94144ce26104c92abb9227a1e95a45395e1db44265e70e585ead915266f`
  for `RetrievalKitFFI.xcframework.zip` and
  `5cac89628b3296aaedda0006049283d87261d157c09d7f537b05a93e8b1f4468`
  for `RetrievalKitGraphFFI.xcframework.zip`. The closed-bundle validator now
  includes the assembler-required `THIRD_PARTY_NOTICES.md` in both root and
  hashed inventory checks; a local replay of the 12 CI-built root-A artifacts
  passes full bundle validation.
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
  paused distribution slice. The root README is an evidence-led product page
  whose numeric observations are mapped to permitted Phase 6 claim IDs and
  mutation-tested in CI. One Swift package exposes four products over the
  graph-capable aggregate; macOS arm64 Python targets CPython 3.10–3.14.
  Release tooling produces a canonical Swift XCFramework archive, a closed
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
  canonical chunk text on load. The graph aggregate ABI was version 7 after
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
  on load. That implementation used graph aggregate ABI version 7; the
  progressive Swift API moved the aggregate to ABI version 8 by adding stable
  document and owner-record identities to result buffers. The Rust-owned
  progressive-builder boundary moves the aggregate to ABI version 9.
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
- Production Swift embedding uses direct Core ML FP32 through
  `CoreMLEmbedder.load(...)`; model acquisition, verification, extraction, and
  local compilation happen only while loading or explicitly prefetching the
  embedder. Retrieval database construction, indexing, persistence, and search
  remain network-free.
- Core ML model conversion and deterministic archive construction stay outside
  the Swift package. The
  generic conversion script is `scripts/embedding/convert-embedding-coreml.py`
  with a BGE compatibility wrapper at
  `scripts/embedding/convert-bge-small-coreml.py`; the production FP32 archive
  builder is `scripts/embedding/build-coreml-fp32-archive.py`. The conversion
  process is documented in `docs/product/embedding-model-conversion.md`.
- Generated model artifacts should stay under
  `target/embedding-models/` and should not be committed by default.
- Python and Node local embeddings use separate optional packages in this same
  repository: `wrappers/python-embedding` (`retrievalkit-embedding`) and
  `wrappers/typescript/embedding`
  (`@gungorbasa/retrievalkit-embedding`). They bind the optional Rust ONNX
  provider directly and do not depend on the base or graph retrieval packages.
  Browser embedding is not supplied by either native package.

## Ingestion Context

- Generic text chunking lives in the separate Rust `retrievalkit-ingest` crate so
  retrieval remains isolated in `retrievalkit-core`.
- Fixed and sentence-aware strategies use Unicode-character limits and overlap;
  returned ranges are UTF-8 byte offsets into the original text.
- Swift exposes chunking through `TextChunker` in `RetrievalKit` and Python
  through `retrievalkit.ingest`. Both call the same Rust implementation.
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
  `alpha=0.6`. Internal Rust benchmarks may still exercise RRF. Public result
  traces expose alpha, vector/keyword ranks, raw scores, normalized scores, and
  matched terms without exposing the internal fusion enum.
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

## 2026-07-25 Python, TypeScript, and Kotlin Wrapper Completion

- Python stable candidate projection now calls the corpus-owned Rust operation
  and preserves generation/corpus checks, filtering, lexical identities,
  source-node count, and before/after counts. Python graph query input and
  result materialization use typed PyO3 conversion; JSON remains only on
  justified cold schema/advanced ingestion and hydration paths.
- Python's common path now accepts `Document` plus one direct embedding,
  graph-only `Record`, or combined `Record` plus an optional direct embedding.
  Rust infers dimension and derives hidden document/chunk identities. The
  legacy keyed-record surface remains available as an advanced compatibility
  path.
- TypeScript/Node is implemented through napi-rs as separate provisional
  `retrievalkit-node-local` and `retrievalkit-node-graph-local` packages.
  Blocking native work uses worker tasks, embeddings use `Float32Array`, i64
  values use exact `bigint`, graph paths/provenance and projection are typed,
  and an aggregate guard rejects loading both packages in one process. The
  narrow qualified target is Node.js LTS on macOS arm64; browser/WASM and public
  npm availability are not claimed.
- Kotlin/JVM and Android are implemented through a thin typed JNI crate with
  separate base and graph aggregates. Public APIs use blocking idiomatic
  overloads, `FloatArray`, sealed values, typed exceptions, and
  `AutoCloseable`. Opaque registry handles resolve under a short global lock
  and then use per-resource locking, so independent databases do not serialize.
  Android AARs currently contain only arm64-v8a. Kotlin Multiplatform and public
  Maven availability are not claimed.
- Base and graph artifacts remain mutually exclusive in every language. The
  graph aggregate includes retrieval; base artifacts are checked to exclude
  graph code and dependencies.

Verification completed without benchmark workloads:

- Scoped Rust format checks for the Python, Node, and JNI crates pass, and
  workspace clippy with all targets/features passes. The repository-wide format
  check still reports pre-existing formatting drift in unrelated CLI, core,
  FFI, graph, and benchmark files under the installed rustfmt; those files were
  deliberately left untouched.
- Base and graph Cargo checks pass for Python, Node, and JNI.
- Python Ruff, strict mypy, base `27` tests, graph `8` tests, all three
  examples, CPython 3.14 wheel builds, and isolated installed-wheel smoke tests
  pass.
- TypeScript build/typecheck/lint pass; base `6` and graph `7` tests pass.
  Package-content, graph-exclusion, isolated local-install, all three examples,
  Node 24 LTS, and production-dependency audit checks pass.
- Kotlin/JVM base and graph unit/conformance tests pass from a forced rerun.
  Retrieval-only, graph-only, and combined examples pass. Both Android release
  AARs assemble and pass base/graph aggregate inspection; the JNI payloads are
  arm64-v8a.
- README claim validation/tests and release validation pass. Publication still
  requires owner authorization, a signed tag, authorized claims, and
  provisioned passing Phase 7 gates.
- The broad `cargo test --workspace --all-features --no-fail-fast` run passes
  every non-CLI target but fails 34 `retrievalkit-cli` V3 qualification tests
  at their common fixture integrity precondition:
  `manifests/chunking.json` is recorded as 715 bytes but is 718 bytes. This
  tracked frozen benchmark/release evidence predates and is unrelated to the
  wrapper changes. It was not regenerated because benchmark and release
  qualification remain explicitly paused.

## 2026-07-26 Release Truth Lock

- `release/release-v0.1.0.json` is the machine-readable source for the qualified
  Python range (`>=3.10,<3.15`) and the base persistence contract (new writes
  use V4; V1–V4 remain readable).
- Release validation now fails closed when Python source metadata or built-wheel
  `Requires-Python` metadata exceeds the qualified CPython 3.10–3.14 range,
  when active base persistence documentation drifts from V4, or when the active
  product spec reintroduces obsolete Node/Maven identities.
- Active migration, compatibility, changelog, Python, and Swift documentation
  now distinguish base V4 snapshots from independently versioned graph
  capability formats. The 2026-07-25 cross-language parity audit is explicitly
  historical evidence for revision `fccb3a9`, not current packaging guidance.
- Focused release tests, static release validation, Python base/graph wrapper
  checks, CPython 3.14 wheel builds, isolated installation smoke tests, and
  built-wheel metadata inspection pass. The next DX blocker is rebuilding the
  public website and source preview from the same current release truth.

## 2026-07-26 Public Query-Path Positioning

- The website and GitHub README must present three first-class query paths:
  retrieval-only exact vector/BM25/hybrid search with `RetrievalDatabase`,
  graph-only traversal and candidate projection with `GraphDatabase`, and
  graph-scoped exact vector/BM25/hybrid retrieval with
  `GraphRetrievalDatabase`.
- Graph-only search is a complete standalone path, not merely a setup step for
  retrieval. Public explanations and examples must make clear that it accepts
  no retrieval configuration, vector index, or embeddings.
- Graph-scoped retrieval uses graph relationships to choose the candidate
  neighborhood; the graph is not a separate scoring signal. The same retrieval
  engine ranks within the selected scope.

## Likely Next Tasks

The owner explicitly resumed Phase B release setup on 2026-07-26. The scoped
npm names, PyPI projects, protected GitHub environments, Maven signing
identity, Central namespace, and Portal token are configured. Registry-owner
setup for all three PyPI identities and all five npm identities is complete:

1. re-verify all registry records and exact protected publisher settings
   immediately before publication dispatch;
2. re-verify the Central namespace, published signing key, and five protected
   Maven secrets immediately before publication dispatch;
3. resume only when the owner explicitly authorizes the signed-tag and
   provisioned Phase 7 evidence gates.

Do not publish v0.1.0, create its tag, rebuild frozen qualification fixtures,
or resume physical-device work until the corresponding documented gate is
explicitly reached.

The canonical result/trace contracts and shared retrieval/graph conformance
expectations now cover Rust, Swift, Python, TypeScript, and Kotlin. Python,
Node, and Kotlin base and graph runners remain separate because their native
aggregates are intentionally mutually exclusive; Swift uses the documented
unified aggregate exception. Current packaging and compatibility status comes
from `release/release-v0.1.0.json`, `docs/product/release-process.md`,
`docs/product/compatibility-policy.md`, and the active product spec. The dated
cross-language parity audit is preserved as historical evidence for its
recorded source revision.

When the owner explicitly resumes release work, continue the parked release
gates from `docs/product/release-process.md` and
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

## 2026-07-26 Browser/WebAssembly Additive Target

- The owner authorized an additive browser/WASM implementation while requiring
  the existing native Swift, Python, Node, Kotlin/Android, Rust, C, and CLI
  implementations and their performance paths to remain unchanged.
- The browser target has three explicit products:
  `RetrievalDatabase`, `GraphDatabase`, and `GraphRetrievalDatabase`. It uses a
  separate `wasm-bindgen` boundary and a dedicated Worker-owned TypeScript
  package; the Node N-API wrapper remains separate.
- The initial artifact is in-memory. Filesystem persistence and `fs2` are
  excluded only on WASM. Native SimSIMD and zstd behavior remains unchanged.
  SimSIMD 6.5.16 checks for WASM but its final release link omits the requested
  C archive, so WASM uses a target-specific portable Rust scorer and half
  conversion.
- Direct generated-WASM smoke coverage exercises all three products and F32,
  F16, BF16, and I8 retrieval. The first 384d F32 portable baseline meets the
  browser p95 ≤10 ms retrieval-only gate at 10K and 25K chunks, but fails it at
  50K. A compiler-only WASM SIMD128 flag did not improve scoring, so explicit
  browser-only SIMD is required before qualifying 50K×384d. The browser must
  match the native compact profile: 384d caller F32 input, per-vector symmetric
  signed-I8 storage/scoring, zero-point 0, and one F32 scale. A portable
  50K×384d I8 diagnostic measures 18.09 ms vector p95 and 18.58 ms hybrid p95.
  The faster 192d diagnostic is scaling evidence only, not a replacement
  product profile. Bulk ingestion is also superlinear and remains a separate
  performance task. Details are in
  `docs/product/reports/browser-wasm-portable-baseline-2026-07-26.md`.
- The explicit browser-only signed-I8 SIMD128 artifact now passes the latency
  gate on the reference M1 Max Node/WASM diagnostic: 50K×384d vector p95 is
  1.80 ms and hybrid p95 is 2.20 ms. Worker-side validation selects SIMD128 or
  the portable fallback before database construction. Complete portable/SIMD
  result conformance passes for 384d and a 396d tail case. Native scoring,
  public database/search methods, and every existing language wrapper remain
  unchanged; cross-browser qualification remains pending.
- Browser package publication, site deployment, a release tag, and public
  performance claims remain unauthorized. Cross-browser and device
  qualification is still required.

## 2026-07-26 Shared ONNX Embedding Experiment

- The completed experiment produced the optional `retrievalkit-embedding` Rust
  crate and evaluated a separate Swift `EmbeddingKitONNX` package. Production
  Swift selected direct Core ML FP32, so the experimental Swift package and its
  Apple ONNX Runtime XCFramework build material were retired on 2026-07-27.
  The Rust provider remains unchanged for cross-platform/non-Apple use.
  `retrievalkit-core` and every retrieval database remain embedding-neutral.
- The frozen model is
  `sentence-transformers/all-MiniLM-L6-v2@c9745ed1d9f207416be6d2e6f8de32d1f16199bf`:
  256-token maximum, masked mean pooling, L2 normalization, and 384-dimensional
  F32 output. FP32, FP16, and Q8 are explicit profiles; Q8 is dynamic signed
  INT8 for ONNX with seven quality-sensitive MatMul nodes retained in full
  precision, and broadly compatible weight-only INT8 for Core ML.
- The approved public artifact destination is
  `gungorbasa/retrievalkit-minilm`. It was published at immutable commit
  `617ce926c1f9e0289365d3e999474cc28b1645d4`; `manifest-v1.json` has SHA-256
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`.
  The Rust ONNX provider continues using those pins. The production Swift Core
  ML archive is separately pinned at immutable commit
  `405818d6afef1aaf2fc8da67da6caf20b55f0a28`; its exact archive and manifest
  hashes are recorded in the production Swift section below.
- The Rust provider uses `ort` 2.0.0-rc.12's API-24 dynamic-loading boundary
  and requires an application-bundled official ONNX Runtime 1.24.3 path. This
  avoids silently using `ort-sys`'s 1.24.2 prebuilt artifact and keeps runtime
  acquisition out of provider initialization. Final qualification used the
  official 1.24.3 arm64 dynamic library. The completed Apple comparison used a
  local official 1.24.3 XCFramework with CPU, XNNPACK, and Core ML execution
  providers; that Swift-only build path and bridge are no longer active.
- The artifact-level Python/Core ML Tools conformance run passes all frozen
  gates, but a later unified SDK-boundary run found provider-specific Q8
  ranking drift. Direct Core ML FP32/FP16/Q8 mean Top-10 overlap is
  100%/99.05%/96.19% and all pass. Swift ONNX CPU is
  100%/99.76%/94.76%, XNNPACK is 100%/99.52%/95.00%, and ONNX Core ML EP is
  100%/99.76%/93.57%. The actual Rust ONNX boundary matches the CPU Q8 result
  at 94.76%. Therefore ONNX CPU Q8 and ONNX Core ML EP Q8 fail the frozen 95%
  gate; XNNPACK Q8 only meets the boundary exactly. The Q8 artifact must retain
  seven quality-sensitive MatMul nodes in full precision, but that exclusion
  alone does not qualify every packaged provider.
- On the M1 Max reference host, the 50-warm-up/750-sample 32-token single-query
  Rust slice measured FP32/FP16/Q8 embedding p95 of
  3.689/5.797/2.216 ms, exact retrieval p95 of 0.218/0.218/0.214 ms, and
  warm end-to-end p95 of 3.967/6.019/2.561 ms over 10K 384d signed-I8 stored
  vectors. All pass the latency targets, but Q8 fails ranking quality and must
  not be described as generally qualified.
- The apples-to-apples Swift release harness measured direct Core ML
  FP32/FP16/Q8 at 3.225/3.029/3.032 ms p95. ONNX CPU measured
  3.697/4.144/2.241 ms, XNNPACK 12.268/7.607/4.113 ms, and ONNX Core ML EP
  15.759/4.491/14.720 ms. Direct Core ML FP16 is the lowest-latency Apple row,
  but the later canonical policy selects FP32 for cross-runtime parity. Q8 does
  not speed up direct Core ML, and the fastest raw
  ONNX Q8 row misses its quality gate. ONNX Runtime reported only partial Core
  ML graph partitioning, so those provider rows are not full-model Core ML
  execution.
- A 2026-07-27 direct Core ML compute-unit qualification kept production code
  unchanged and measured fixed-256 FP16 at 32 tokens with three independent
  50-warm-up/750-sample runs. CPU-only median p95 was 5.968 ms, CPU+GPU was
  3.303 ms but ranged 3.205–4.835, CPU+Neural Engine was a stable 3.200 ms,
  and `.all` was 3.118 ms. All FP16 modes passed cosine and Top-10 gates.
  Keep `.all` as the Apple default; CPU+Neural Engine is the most predictable
  explicit M1 Max policy. The historical Swift ONNX CPU row beat its partially
  partitioned Core ML execution-provider row, but neither is a production
  Swift path. Retrieval remains CPU-resident. Direct Core ML Q8 with CPU+GPU
  aborted twice in Apple's MPSGraph MLIR compiler (status 134), reinforcing
  that Q8 is not an Apple production choice.
- The Swift ONNX A/B experiment is complete. Its dynamic sequence inputs used
  the actual input length up to 256; production direct Core ML remains fixed at
  256. The flexible
  Core ML candidate passed conformance but regressed from 2.915 to 6.518 ms p95
  and from 3.038 to 7.612 ms p99, so it fails both adoption gates and must not
  replace the fixed package.
- The default benchmark commands implement the full token-length
  16/32/64/128/256 and batch 1/8/32 matrix. The report's final acceptance
  evidence uses 50/750 at 32 tokens and batch one; every other matrix shape was
  exercised as a 5/10 scaling smoke. Do not describe the entire matrix as
  50/750-qualified until the long-running commands complete at that sample
  count. Exact results and regression limitations are recorded in
  `docs/product/reports/shared-onnx-embedding-experiment-2026-07-26.md`.
- No RetrievalKit release, package publication, or version tag is authorized
  by this experiment. Publishing the explicitly approved model artifacts is
  separate from publishing SDK packages.

## 2026-07-27 FP32 Embeddings And I8 Database Storage

- Model precision and database encoding are separate. The optional Rust
  embedding crate defaults to FP32, and production Swift uses direct Core ML
  FP32. RetrievalKit's caller-produced F32 boundary and default
  `I8ScalarQuantized` database encoding are unchanged.
- FP32 ONNX and Core ML produced 100% mean and exact Top-10 agreement on the
  frozen fixture. After Rust-equivalent per-vector signed-I8 encoding, both
  provider directions measured 99.76% mean Top-10 overlap, 97.62% exact sets,
  and a 90% minimum; CPU-only reference and production Core ML `.all` both
  passed the locked gates.
- `I8ScalarQuantized` remains the database default and stores exactly 384
  signed bytes plus one four-byte scale for each 384d vector. Actual persisted
  vector files at 10K/25K/50K rows were exactly
  3,880,000/9,700,000/19,400,000 bytes, with no F32 duplicate. Corresponding
  complete synthetic database directories were
  4,129,778/10,337,778/20,658,824 bytes versus
  15,609,765/39,037,765/78,058,810 bytes for F32.
- On the same release benchmark, I8 retrieval p95 at 10K/25K/50K was
  0.181/0.427/0.791 ms versus 0.945/2.203/4.811 ms for F32. Model Q8 remains an
  explicit experiment: it neither reduces an already-I8 database nor matches
  FP32 cross-runtime ranking parity.
- The same provider vectors passed through the actual
  `GraphRetrievalDatabase` implementation in both cross-provider directions.
  Mean Top-10 overlap was 99.76% for vector, 100% for hybrid, 100% for
  graph-scoped vector, and 99.29% for graph-scoped hybrid; every minimum was
  at least 90%. Full and graph-scoped BM25 hits were exactly identical, as was
  the graph-only selection. I8 retrieval timings include query quantization.
- The target-local Core ML packages were restored from the existing canonical
  export copy and pass the complete root manifest validator. The immutable
  public repository's earlier loose Core ML directories had Core ML
  Tools-rewritten `Manifest.json` representations that did not match the root
  canonical-tree digest. The production correction is the deterministic
  archive at commit `405818d6afef1aaf2fc8da67da6caf20b55f0a28`; consumers do
  not use those earlier loose directories.

## 2026-07-27 Production Swift Core ML

- `EmbeddingKit` is the only production Swift embedding package. Its
  `CoreMLEmbedder.load(...)` API defaults to the fixed-256 FP32
  `all-MiniLM-L6-v2` model and Core ML compute units `.all`. Existing
  local/bundled initializers remain available. Every production result is
  checked to contain exactly 384 finite values and normalized to unit L2 norm.
- The immutable archive is
  `all-MiniLM-L6-v2-coreml-fp32-v1.tar` at
  `gungorbasa/retrievalkit-minilm@405818d6afef1aaf2fc8da67da6caf20b55f0a28`.
  It is `90,664,960` bytes with SHA-256
  `e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f`.
  `archive-manifest-v1.json` is `2,029` bytes with SHA-256
  `085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60`;
  the canonical payload-tree SHA-256 is
  `29f56defb74316d8491e7fba4eeba98cf24dc10b0e2b5b1df4a2d4e352f5fe5c`.
  Two builds were byte-identical, and a public immutable HTTPS re-download,
  clean safe extraction, and full canonical-tree comparison passed.
- Download is allowed only inside `load(...)` or explicit `prefetch(...)`.
  HTTPS, exact archive size/SHA-256, a closed regular-file ustar inventory,
  manifest identity, and every payload size/SHA-256 are verified before atomic
  publication in the OS cache. `localOnly` performs no network request;
  concurrent in-process loads share acquisition. Partial/corrupt archives and
  extracted caches are removed. The locally compiled `.mlmodelc` cache key
  includes immutable artifact identity and OS/Core ML compatibility, and a
  failed compiled-model load triggers one clean recompile.
- On the Apple M1 Max reference host with a release build and a 32-token query,
  a genuine cold public download plus verification/extraction/compilation took
  19.525 s. Cached initialization took 436.596 ms, first inference 81.456 ms,
  and 50-warm-up/750-measurement warm embedding p95 was 4.527 ms. The frozen
  I8 retrieval p95 is 0.218 ms at the 10K acceptance shape, so the combined
  measured boundaries remain below the 10 ms product gate; retrieval-only
  remains below 8 ms.
- The frozen ONNX CPU FP32 versus Core ML `.all` FP32 rerun retained median
  cosine 1.0 and 100% mean/exact Top-10 agreement. Both actual RetrievalKit I8
  directions passed vector, hybrid, graph-scoped vector, and graph-scoped
  hybrid gates; BM25, graph-scoped BM25, and graph-only selection were exactly
  identical. The generated reports retain SHA-256
  `71e864a8445faae9933e196119a5343af2ebec446eb6bc20b30c564c264b8f42`
  and `7eb3cf309cd6b2e3fd08d8a28da4cae74f4478f68422146d4c4ec3ae32de3bfc`.
- The retired Swift ONNX package, C bridge, Apple XCFramework builder/settings,
  and active guidance are removed. Historical comparison metrics remain in the
  dated report. The optional Rust ONNX provider and all Browser/WASM work are
  unchanged. No RetrievalKit package release, registry publication, tag, or SDK
  upload was performed.
- Exact artifact, command, environment, latency, conformance, regression, and
  remaining-risk details are in
  `docs/product/reports/swift-coreml-production-implementation-qualification-2026-07-27.md`.

## 2026-07-27 Production Python And Node Embeddings

- Production Python and Node embedding wrappers now live in the RetrievalKit
  monorepo as independently distributable optional packages:
  `wrappers/python-embedding` and `wrappers/typescript/embedding`. Their native
  aggregates are `retrievalkit-python-embedding` and
  `retrievalkit-node-embedding`; both bind the existing optional
  `retrievalkit-embedding` Rust provider and neither depends on
  `retrievalkit-core`, `retrievalkit-graph`, or a retrieval wrapper.
- Both public packages expose only the canonical FP32
  `sentence-transformers/all-MiniLM-L6-v2` revision
  `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`. Input is fixed at 256
  WordPiece tokens. Output validation requires exactly 384 finite,
  L2-normalized F32 values. RetrievalKit continues accepting F32 publicly and
  independently defaults database storage to `I8ScalarQuantized`; no database
  migration or duplicate F32 payload is introduced.
- Python provides `OnnxEmbedder.load`, `prefetch`, `embed`, and `embed_batch`
  and releases the GIL around model/cache and inference work. Node provides
  asynchronous `load`, `prefetch`, `embed`, and `embedBatch`, schedules native
  work away from the event loop, and supports `close`,
  `Symbol.dispose`, and `Symbol.asyncDispose`.
- Model download is limited to embedder loading or explicit prefetch.
  `local_only`/`localOnly` is network-free. Both wrappers share the Rust
  provider's verified OS cache, immutable artifact pin, cross-process
  acquisition lock, temporary downloads, exact size/SHA-256 checks, atomic
  publication, and corrupt/partial cleanup. A clean Python prefetch completed
  in 9.016 s; Node then loaded the same cache locally in 378.212 ms.
- The official macOS arm64 ONNX Runtime 1.24.3 package boundary is
  `27,724,968` bytes with SHA-256
  `b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729`.
  Package builds verify this identity and include the runtime license and
  third-party notices. No runtime binary is checked into the repository.
- Frozen 94-vector qualification against the Rust FP32 reference passed for
  both wrappers: median cosine `1.0`, minimum cosine
  `0.9999999999998386`, mean Top-10 overlap `100%`, exact Top-10 sets
  `100%`, and minimum per-query overlap `100%`. On the Apple M1 Max reference
  host, release 50-warm-up/750-measurement 32-token embedding p95 was
  `6.222 ms` for Python and `6.207 ms` for Node.
- Base Python, graph Python, Node base/graph, Rust core/graph/embedding, and
  Browser/WASM regressions pass. Package-content and dependency-tree checks
  prove the new packages remain retrieval-free. Neither registry package,
  RetrievalKit release, tag, nor SDK artifact was published.
- Exact commands, hashes, package checks, and remaining risks are recorded in
  `docs/product/reports/python-node-embedding-production-implementation-qualification-2026-07-27.md`.

## 2026-07-27 Production Browser Embedding

- Browser FP32 MiniLM embedding lives in the independent
  `wrappers/browser-embedding` package. It imports neither browser retrieval,
  Node N-API, nor Rust retrieval code. A dedicated module Worker owns verified
  acquisition, tokenization, ONNX Runtime Web session creation/warmup, FIFO
  inference, cancellation, and contiguous result transfer.
- The package pins `@huggingface/tokenizers` 0.1.3,
  `onnxruntime-web` 1.27.0, and the six-file FP32 artifact inventory at
  `gungorbasa/retrievalkit-minilm@617ce926c1f9e0289365d3e999474cc28b1645d4`.
  `manifest-v1.json` retains SHA-256
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`.
  An actual public immutable HTTPS acquisition verified all six files in
  10.547 s.
- Download is limited to `load` or explicit `prefetch`. `localOnly` performs no
  artifact fetch. Every response and cache hit is checked by exact size and
  SHA-256; concurrent work is deduplicated, the completion marker is last, and
  interrupted, partially published, or corrupt state is cleaned. Application
  Worker/JavaScript/ORT assets must be served or precached separately for
  completely offline startup.
- Output is fixed to a maximum of 256 tokens and exactly 384 finite,
  L2-normalized F32 values. Frozen actual-WASM conformance passed 94/94 with
  median cosine `0.9999999999996866`, minimum cosine
  `0.9999999999991718`, and 100% mean/exact/minimum Top-10 agreement versus
  Rust FP32.
- Both browser/Rust I8 database-query directions passed RetrievalKit's actual
  vector, hybrid, graph-scoped vector, and graph-scoped hybrid gates. BM25,
  graph-scoped BM25, and graph-only selection were exactly identical.
  Persisted I8 regression proves one signed byte per dimension plus one F32
  scale and no duplicate F32 payload.
- A real Chrome 150 dedicated-Worker WebGPU run, with external network blocked,
  measured cached initialization 756.400 ms, first inference 25.300 ms, and
  50-warm-up/750-measurement warm p95 7.500 ms. The matching 50K×384d I8
  SIMD128 retrieval rerun measured vector p95 1.887 ms and hybrid p95
  2.250 ms, producing separate-boundary sums of 9.387 ms and 9.750 ms. The
  WASM fallback is correct but its warm p95 is 19.804 ms and does not pass the
  combined performance gate.
- Package checks pass with 12 offline tests, 64 files, 9,385,943 compressed
  bytes, 38,262,039 unpacked bytes, verified runtime/legal assets, and zero
  production audit findings. Browser retrieval, Rust core/graph/embedding/WASM,
  release metadata, and generated portable/SIMD smokes pass. No package,
  website, tag, model artifact, or RetrievalKit release was published.
- The initial implementation report left Firefox, Safari, and real
  CacheStorage qualification open. Firefox and the core CacheStorage matrix
  are superseded by the desktop production matrix below. Safari, mobile,
  private-mode/cache-pressure behavior, the material model/runtime footprint,
  the slower WASM tier, actual combined Chrome latency, and 50K ingestion cost
  remain open.

## 2026-07-27 Browser Desktop Production Matrix

- The real production matrix now composes `wrappers/browser-embedding` and
  `wrappers/browser` in separate dedicated module Workers against the generated
  `retrievalkit-wasm` SIMD128 artifact. It uses real browser CacheStorage,
  50K×384d I8 retrieval, a tokenizer-verified 32-token query, 50 warm-ups, and
  750 measurements.
- Chrome 150 selected WebGPU and passed all correctness/cache gates. Cached
  initialization was `696.160 ms`, first inference `22.770 ms`, 50K ingestion
  `57,485.055 ms`, embedding p95 `10.560 ms`, retrieval p95 `1.905 ms`, and
  same-page end-to-end p95 `12.405 ms`. The earlier separately summed
  sub-10-ms estimate is not an end-to-end pass and must not be used as one.
- Firefox 150 exposes `navigator.gpu` but has no usable adapter on this host.
  Production selection now requires `requestAdapter()` to succeed, so Firefox
  correctly selected WASM. It passed all correctness/cache/retrieval gates with
  embedding p95 `20.120 ms`, retrieval p95 `1.580 ms`, and end-to-end p95
  `21.660 ms`.
- Concurrent cold browser prefetch made exactly six artifact requests, then
  published seven CacheStorage entries atomically. Interrupted acquisition
  left no partial cache residue. Missing `localOnly`, cached-only load,
  corruption cleanup/recovery, Unicode, 256-token truncation, empty input, and
  post-close behavior passed in Chrome and Firefox. A real Chrome CacheStorage
  fixture additionally passed eviction cleanup; deterministic quota injection
  preserves typed cache errors.
- The initial Safari 26.5.2 attempt reached the matching system driver but was
  blocked by the disabled Allow Remote Automation setting; the later
  2026-07-28 run below supersedes that blocker. Mobile browsers, private
  browsing, natural cache pressure, and 50K ingestion cost remain open. Safari
  performance was later accepted under the platform-specific budget recorded
  below. No package, site, tag, or release was published. The original
  Chrome/Firefox evidence SHA-256 is
  `ffb80633fe00239b42c45428ef2829f4f379e3ecd164a8ea2448d854be47a38e`;
  full details are in
  `docs/product/reports/browser-desktop-matrix-qualification-2026-07-27.md`.

## 2026-07-28 Browser WebGPU Hot-Path Investigation

- Temporary runtime instrumentation separated ONNX execution from public
  Worker/client overhead. At 256 chunks, runtime/public embedding p95 was
  `7.005/7.090 ms`; after the production 50K build it was
  `10.515/10.685 ms`. The boundary accounts for only about `0.1-0.2 ms`; the
  missing combined budget is inside WebGPU execution under the sustained 50K
  browser workload.
- A 10-second settle, bounded transfer batches, releasing source inputs,
  WebGPU-only provider configuration, cached session recreation, load-order
  changes, bounded corpus generation, and foreground-target control did not
  produce a sub-10-ms result. These were diagnostics only and were removed
  from the production harness.
- Production now avoids two redundant 384-value F32 copies on the
  single-embedding path while retaining finite, normalized, exact-dimension
  validation at runtime, service, and client boundaries. Browser embedding
  tests (19), browser retrieval tests (12), package validation (64 files), and
  harness tests (19) pass.
- The final original, uninstrumented 50K Chrome 150 run measured cached
  initialization `909.880 ms`, first inference `22.980 ms`, ingestion
  `63,630.295 ms`, embedding p95 `10.610 ms`, retrieval p95 `1.995 ms`, and
  end-to-end p95 `12.460 ms`. Correctness and CacheStorage gates passed; the
  combined gate did not. Evidence SHA-256:
  `29ffa34a970e629170b2008f654a9d89c4ac5c94c9de4c78e372b6b6817aa1be`.
- Do not claim a browser sub-10-ms combined pass or weaken precision, token
  count, corpus size, provider semantics, or independent Worker ownership.
  Browser/Metal tracing is the next performance investigation. Details:
  `docs/product/reports/browser-webgpu-hot-path-investigation-2026-07-28.md`.
- The owner accepted these measured desktop numbers on 2026-07-28 and replaced
  the former combined sub-10-ms gate with provider-tiered budgets on the fixed
  Apple M1 Max, 50K×384d, 32-token, 50-warm-up/750-measurement contract:
  WebGPU embedding plus SIMD128 retrieval p95 `<=15 ms`, WASM compatibility
  embedding plus SIMD128 retrieval p95 `<=25 ms`, and retrieval-only p95
  `<=8 ms`. Chrome passes at `12.460 ms`; Firefox passes at `21.660 ms`.
  These are reference-host qualification budgets, not universal device SLAs.
  Safari execution and mobile-device measurements remain open; browser/Metal
  tracing is optional optimization work rather than a release gate.
- A 2026-07-28 Safari-only 50K matrix attempt used `--require-all` and failed
  before WebDriver session creation with Safari's explicit instruction to
  enable **Allow remote automation** in Developer settings. The repository did
  not change that security-sensitive owner setting. That attempt produced no
  Safari sample and was superseded by the successful run below.
- The owner subsequently ran `/usr/bin/safaridriver --enable`, and the exact
  Safari-only matrix passed all correctness, CacheStorage, lifecycle, and
  actual-I8 retrieval checks. Safari 26.5.2 selected WebGPU embedding and
  SIMD128 retrieval. Cached initialization was `2,026.660 ms`, first inference
  `184.520 ms`, ingestion `52,133.680 ms`, embedding p95 `16.520 ms`,
  retrieval p95 `1.940 ms`, and end-to-end p95 `18.380 ms`. Retrieval passes;
  the result exceeded the general `15 ms` WebGPU tier. The owner subsequently
  accepted a Safari-specific `20 ms` reference budget, so Safari passes and
  performance optimization is deferred. The general Chrome WebGPU `15 ms`,
  Firefox WASM compatibility `25 ms`, and retrieval-only `8 ms` budgets remain
  unchanged. Evidence SHA-256:
  `80adf52555758ff168e2a39411cedff16c0b4bba15339417cc8279c72f68bec3`.
  Details:
  `docs/product/reports/browser-safari-desktop-qualification-2026-07-28.md`.

## 2026-07-27 Production Kotlin/JVM And Android Embedding

- Optional Kotlin embedding now lives in the independent `:embedding` and
  `:android-embedding` modules. Their native aggregate is
  `retrievalkit-jni-embedding`, which depends on the existing optional
  `retrievalkit-embedding` provider and not on `retrievalkit-core`,
  `retrievalkit-graph`, or any retrieval JNI aggregate.
- The blocking `OnnxEmbedder` API exposes FP32-only `load`, `prefetch`,
  `embed`, `embedBatch`, immutable model information, and deterministic
  `AutoCloseable`. It enforces the canonical 256-token input behavior and
  exactly 384 finite, L2-normalized F32 output values. Blank inputs and empty
  batches use typed errors. Android callers use `AndroidOnnxEmbedder` to root
  the verified model cache under the application cache.
- Model download is limited to embedder loading or explicit prefetch.
  `localOnly` is network-free. Kotlin reuses the Rust provider's immutable
  artifact commit
  `617ce926c1f9e0289365d3e999474cc28b1645d4`, manifest SHA-256
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`,
  verified cache, file lock, temporary downloads, corrupt/partial cleanup, and
  atomic publication. A real empty-cache public prefetch through the packaged
  JVM JAR took `31,957.196 ms`; a missing-cache local-only load failed with
  `ModelAcquisitionException` and made no fallback. Two independent JVM
  processes then contended on another empty cache: verified prefetch returned
  in `14,848.206/15,129.522 ms`, both emitted byte-identical vectors, and the
  cache retained no temporary or partial file.
- Kotlin/JVM packages official ONNX Runtime 1.24.3 for macOS arm64 at
  `27,724,968` bytes and SHA-256
  `b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729`.
  Android packages only arm64-v8a from the official `40,948,335`-byte AAR
  (SHA-256
  `67397e4a970e75617f765d2015ceaf911917e1d822276cfb5792744e8085cbce`);
  selected `libonnxruntime.so` is `25,831,632` bytes with SHA-256
  `4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad`.
  Both artifacts include exact ONNX Runtime legal files. Generated runtime and
  JNI binaries remain outside source control.
- Frozen Kotlin/JVM qualification passed 94/94 vectors versus Rust FP32 with
  median cosine `1.0`, minimum cosine `0.9999999999998386`, and 100%
  mean/exact/minimum Top-10 agreement. The packaged-JAR release run measured
  cached initialization `1,667.211 ms`, first inference `8.906 ms`, and
  50-warm-up/750-measurement warm p50/p95/p99
  `6.627/8.175/9.337 ms`. Adding the frozen native I8 retrieval p95
  `0.218 ms` yields `8.393 ms`, below the combined 10 ms gate.
- Both Kotlin/Rust I8 database/query directions pass RetrievalKit's actual
  vector, hybrid, graph-scoped vector, and graph-scoped hybrid paths. BM25,
  graph-scoped BM25, and graph-only selection are exactly identical. The
  persisted I8 regression still proves one signed byte per dimension plus one
  F32 scale with no duplicate F32 vector payload.
- The JVM JAR and Android AAR closed-inventory verifiers pass. The Android AAR
  is `12,061,602` bytes with SHA-256
  `ecc7a93ce6917f3887cf560355c11d1a97a87b15f9cea8449a036c39e79ea996`;
  its only native entries are the arm64-v8a JNI aggregate and official ONNX
  Runtime. Live Android device inference was not available; the 2026-08-01
  owner decision now classifies it as unqualified deferred evidence rather
  than a v0.1.0 release gate. No Maven artifact, RetrievalKit release, tag, or
  SDK package was published.
- Exact commands, hashes, conformance metrics, package inventories, and
  remaining risks are in
  `docs/product/reports/kotlin-embedding-production-implementation-qualification-2026-07-27.md`.

## External Naming Decision

- 2026-07-31: after considering the unrelated `retrieval-kit` crate published
  on crates.io on 2026-04-27, the owner decided to keep the product name
  `RetrievalKit` and proceed. This is an owner decision, not evidence that
  outside legal counsel performed trademark clearance, and the known naming
  overlap remains a risk accepted by the owner rather than an unresolved
  release blocker. The approved registry identities remain PyPI `retrievalkit`
  and `retrievalkit-graph`, npm `@gungorbasa/retrievalkit` and
  `@gungorbasa/retrievalkit-graph`, and Maven `io.github.gungorbasa`.

- 2026-07-31 embedding release decision: v0.1.0 also publishes the existing
  optional embedding integrations for every supported wrapper target. The
  approved identities are PyPI `retrievalkit-embedding`, npm
  `@gungorbasa/retrievalkit-embedding` and
  `@gungorbasa/retrievalkit-browser-embedding`, and Maven
  `io.github.gungorbasa:retrievalkit-embedding` and
  `io.github.gungorbasa:retrievalkit-embedding-android`. Swift continues to
  expose `EmbeddingKit` inside the unified `RetrievalKit` package. Rust
  embedding crates remain source-only and are not published to crates.io.
