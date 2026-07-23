# Phase 7 Regression Gates Report

Status: complete

Date: 2026-07-21

Physical-device execution: none

External publication: none

## Outcome

Phase 7 adds 26 fail-closed gates across pull-request, scheduled/full, and
release tiers. The pull-request tier runs an original checked-in synthetic
graph-quality fixture through production Rust retrieval and graph code. The
scheduled tier accepts only explicitly provisioned, hash-bound full-evaluation
observations. The manual release tier validates pre-collected controlled-device
evidence only after a separate owner authorization binds the observation.

No production API or production dependency changed. No network dataset,
private corpus, secret, physical device, or unstable absolute timing gate is
used in ordinary PR checks. No device installation, launch, collection, or
command was run. The canceled 100K physical-device lane remains permanently
excluded and has no workflow option.

## Frozen Phase 7 identities

- contract JSON SHA-256:
  `85bc320da7d740e101bb6f04b688e825d3accea36a0ce6cb505026e7e921b730`
- gate registry SHA-256:
  `74370da71e1da54d27c4173edc11b14d05839d22f8e522b5270f3fa3623e8f41`
- baseline SHA-256:
  `338bf2231a9b9c841515fb7018b2be7014993a71964e9e4876244f90173bd34d`
- fixture SHA-256:
  `79c37a1f1abf026d28e784454f1eb3410ad24e3abbfc0e33347b3e6a0da164a4`
- expected smoke observation SHA-256:
  `ce55ed64bfbe12f5aa903686cbad8839f048d806520c319223e8b4f8be976fcd`
- result schema SHA-256:
  `47a7cc56b290d0249b7bee76b511d90af8ba58422f074b20ef7678209c5ac4de`
- canonical Phase 7 static artifact-set SHA-256:
  `07208b27160fa2c27c75c91961020a96615a83e4f49ab47071e340e474b28a01`

The 2026-07-23 RetrievalKit rename mechanically changed the contract namespace
and repository-local evidence paths. The manifest and validator pin the
resulting identities above; gate inventory, thresholds, baselines, fixtures,
and result schema are unchanged.

JSON uses sorted keys, two-space indentation, finite numbers, and one trailing
LF. Result roots omit time, absolute paths, randomness, and unstable host data.

## Pull-request gates

Nine blocking PR gates cover:

- exact, internal BM25, and weighted-hybrid result identities and ordering;
- zero deleted, superseded, or dimension-mismatched results;
- metadata-filter correctness;
- save, read-only validation, load, and replay equivalence;
- graph selection, ordered traversal, candidate projection, scoped ranking,
  supported empty scope, and stale/invalid scope rejection;
- perfect fixture NDCG@3, Recall@3, complete-evidence recall, candidate recall,
  and candidate complete evidence;
- zero graph queries, visited nodes, traversed edges, or projected candidates
  in graph-free instrumentation; and
- closed schema, inventory, SHA-256, canonical serialization, and two-root
  result determinism.

All smoke quality thresholds are 1.0 and all correctness violation counts are
zero. These are exact synthetic invariants, not noisy quality estimates.

## Scheduled/full gates

Nine controlled full-evaluation gates require the frozen 296-query HotpotQA
locked-test population, MiniLM revision, pinned evaluators, Phase 5 dependency
environment, and Phase 3–5 evidence needed by the Phase 6 validator.

The exact floors are:

| Metric | Threshold |
| --- | ---: |
| NDCG@10 | 0.9279094336065143 |
| Recall@10 | 0.9577702702702703 |
| Complete Evidence Recall@10 | 0.9222972972972973 |
| Candidate Recall | 0.9679054054054054 |
| Candidate Complete Evidence | 0.9425675675675675 |
| Empty scopes | 0 |
| Exact external-reference Recall@10 | 1.0 |

The other gates require byte-identical full artifact roots and a passing Phase
6 claim/publication validation with all frozen input identities unchanged.
Unavailable licensed inputs produce `not_provisioned` for every affected gate
and a nonzero workflow result. USearch timings remain ineligible because the
frozen ANN Recall@10 gate failed.

## Release gates

Eight manual release gates require the complete supported 10K/25K/50K F32/I8
matrix and reject any missing configuration, hidden skip, invalid environment,
or unauthorized input. Required qualifiers are iPhone 17 Pro Max
(`iPhone18,2`, `V54AP`), arm64 release configuration, iOS build, exact
toolchain, source revision, session/sample count, foreground state, nominal or
fair thermal boundaries, Low Power Mode off, network isolation, and fresh
processes.

Controlled thresholds are:

- maximum query or lifecycle P50/P95/P99 ratio to the bound baseline: 1.10;
- maximum graph-free candidate/baseline median-session P95 ratio: 1.03;
- maximum process peak memory: 1,610,612,736 bytes;
- maximum per-component or total persisted-size ratio: 1.05; and
- zero lifecycle, claim-policy, or physical-device 100K violations.

The 10% latency allowance is the pre-registered initial controlled-run
variance. It does not apply to generic CI and cannot be widened after a
failure. Persisted-size headroom is 5% because storage accounting is much less
hardware-sensitive. The memory ceiling is the frozen Phase 4 device-safety
budget. Baseline changes require explicit old/new evidence, rationale,
platform/toolchain, samples, source revision, claim impact, review, and
regenerated hashes.

## Result and failure artifacts

Every run writes canonical `result.json` and `failure-summary.md`. A failed row
states the regressed metric, expected and actual values, baseline identity,
affected guarantee or claim, evidence paths, reproduction command, and blocking
tier. Missing scheduled or release input writes the same artifacts with
`not_provisioned`; this status is never accepted in the PR tier and never
converted to pass.

## Independent validation and mutation coverage

The independent validator imports no runner code. It validates static and
result schemas, exact gate membership, thresholds, baseline references, frozen
Phase 4–6 identities, fixture provenance/judgments, hashes, workflow security,
failure summaries, platform qualifiers, prohibited claims, and two-root byte
identity.

Eighteen mutation tests cover changed identity, changed ordering, deleted or
superseded results, dimension mismatch acceptance, filter errors, replay
divergence, NDCG/recall/evidence loss, candidate loss, unexpected empty or
invalid scopes, graph-free activity, artifact/schema tampering, hidden skips,
unauthorized baseline identity, graph-free slowdown, memory and persisted-size
overruns, lifecycle/latency regression, USearch winner language, 100K device
claims, and missing platform/version qualifiers. All mutations are rejected.

## CI security and reliability

All workflows use `contents: read`, pinned full action SHAs, bounded job
timeouts, concurrency controls, and no `pull_request_target`. PR jobs receive
no secrets and use only repository fixtures. Controlled scheduled and release
jobs run on explicitly labeled self-hosted Apple Silicon runners. Machine
results and human summaries are retained for 30 days (full) or 90 days
(release). Infrastructure failure cannot produce a pass.

The release workflow is manual, requires an explicit boolean authorization and
a separate authorization JSON binding the observation hash, validates evidence
only, and contains no device-management command. Accepted or rejected summary
artifacts are preserved. The workflow text contains no 100K option.

## Frozen evidence preservation

The independent validator confirms these unchanged identities:

- Phase 6 contract:
  `0fa1ac9f10b502c178b263b68f2e84e4121443e6aa329a8cfca239afa60afd94`
- Phase 6 manifest:
  `819c2310f23c24a623f149b982d7d8048dc970b7a55ddd2401be56670cd068ad`
- Phase 6 artifact set:
  `8c935925dbc48e82fcdf6bcfb83e3c878dce5f7d488ef6c11a332dfa465ead90`
- Phase 6 validator:
  `af0147b3a929c0955939f660d84a783774f4f74bad6be5446aa385ce79528860`
- Phase 5 artifact set:
  `1e7283359f1781dacca1ced3c2fa1794e19a02a2b9669a782465e8f42a8c5602`
- Phase 4b supported / graph-free sets:
  `f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a`
  / `6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a`

No frozen Phase 4–6 file was modified, regenerated, rebaselined, or
reinterpreted. The Phase 6 claim register remains the authority for permitted,
prohibited, and withheld statements.

## Remaining risk

Full collection and release performance gates cannot run on ordinary hosted
CI because licensed inputs and controlled hardware are intentionally absent.
They remain blocking when provisioned and visibly `not_provisioned` otherwise.
The 10% latency envelope may need a new reviewed baseline after a legitimate
toolchain or OS transition; it cannot be adjusted from a failed result. Older
iPhone, energy, thermal-efficiency, 100K support, USearch timing, and equivalent
graph-performance winner claims remain unavailable.
