# Phase 6 Benchmark Publication Report

Status: complete

Report date: 2026-07-21

External publication: none

## Outcome

Phase 6 produced a frozen, repository-local benchmark publication package with
a normative contract, public methodology, separate retrieval-quality, Mac, and
physical-device reports, a machine-readable claim register, an evidence index,
a primary-source licensing audit, reproduction instructions, checksums, a
closed manifest, and an independent validator.

The validator result is PASS. It imports no generator code. It independently
recomputes Phase 3 per-query quality aggregates, candidate statistics, Phase 5
raw-sample percentiles and exact ratios, USearch Recall@10, every published
Phase 4b query percentile from raw samples, and all graph-free ratios. It also
enforces the exact claim membership and wording, evidence eligibility,
hardware/OS/version qualifiers, expiry, licensing decisions, inventory, and
hashes.

No production code, public API, production dependency, frozen Phase 4/5 file,
or supported capacity changed. No physical-device command, retuning, upload,
release, push, message, website, or external publication occurred. Phase 7 was
not started.

## Frozen identities

- contract JSON SHA-256:
  `0fa1ac9f10b502c178b263b68f2e84e4121443e6aa329a8cfca239afa60afd94`
- publication manifest SHA-256:
  `819c2310f23c24a623f149b982d7d8048dc970b7a55ddd2401be56670cd068ad`
- canonical publication artifact-set SHA-256:
  `8c935925dbc48e82fcdf6bcfb83e3c878dce5f7d488ef6c11a332dfa465ead90`
- independent validator SHA-256:
  `af0147b3a929c0955939f660d84a783774f4f74bad6be5446aa385ce79528860`
- validation record SHA-256:
  `6ed7a06851f12ac1ba4d15cc9f73420769e5333587de6b5a600e7f7597e423df`
- Phase 3 input artifact-set SHA-256:
  `e5d5824365d40745156701ba36744c1b7f764ce8fffb13245112b2c9ecb771c6`
- Phase 4b supported/graph-free set SHA-256:
  `f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a` /
  `6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a`
- Phase 5 input artifact-set SHA-256:
  `1e7283359f1781dacca1ced3c2fa1794e19a02a2b9669a782465e8f42a8c5602`
- measured RetrievalKit revision:
  `9c784d2f11b91bb907150aa1b6046880ff89fde6`

The manifest excludes itself from the canonical preimage and binds all nine
other publication-root files, the contract, validator, source revisions, and
frozen input identities.

## Independently recomputed results

The frozen HotpotQA comparison covers 296 common valid queries. Whole-corpus
weighted-I8 to graph-scoped weighted-I8 results are:

| Metric | Baseline | Scoped | Delta | Relative | W/T/L |
| --- | ---: | ---: | ---: | ---: | ---: |
| NDCG@10 | 0.858036 | 0.927909 | 0.069873 | 8.14% | 121/157/18 |
| Recall@10 | 0.871622 | 0.957770 | 0.086149 | 9.88% | 69/211/16 |
| Complete evidence@10 | 0.743243 | 0.922297 | 0.179054 | 24.09% | 69/211/16 |

The mean per-query candidate reduction is 972.65x, candidate recall is 96.79%,
candidate complete evidence is 94.26%, and empty scopes are zero. The pooled
totals are 3,750,320 eligible and 6,326 projected chunks, or 592.84x; that
pooled ratio is explicitly not substituted for the mean per-query result.

On the frozen Apple M1 Max exact F32 benchmark, sqlite-vec 0.1.9 P50 divided by
RetrievalKit P50 is:

| Size | Unfiltered | Filtered |
| --- | ---: | ---: |
| 10K | 7.17x | 10.38x |
| 25K | 7.60x | 9.08x |
| 50K | 7.29x | 8.43x |

The exact P50/P95 nanosecond values and millisecond rounding are in
`mac-systems-performance.md` and `evidence-index.json`. RetrievalKit and
sqlite-vec passed the frozen identity, filtering, deletion, determinism, and
reload gates. USearch 2.26.0 independently recomputes to mean Recall@10
0.965/0.850/0.775 at 10K/25K/50K; the gate failed and all USearch timing
comparisons remain disqualified.

All 48 physical-device query rows independently reproduce as the median of
five per-session nearest-rank P50/P95/P99 values over 1,000 samples. All six
graph-free candidate/baseline median-session P95 ratios pass: F32
1.00/0.99/1.00 and I8 1.01/0.98/1.02 after two-decimal publication rounding
for exact-vector/BM25/hybrid. The full unrounded values are in
`evidence-index.json`. The OS evidence split is retained exactly: query
sessions and 10K F32 prepare on iOS 26.5.1 (23F81), and the remaining 815
lifecycle artifacts on iOS 26.5.2 (23F84). The 100K result remains
`not_run_device_safety`, with zero accepted and five rejected partial
artifacts.

## Claim register

The register contains nine permitted, six prohibited, and four withheld
claims. Exact proposed text and all required fields are authoritative in
`claim-register.json`.

Permitted:

- `P6-QUALITY-001`: scoped HotpotQA NDCG@10 result, with losses and no universal
  graph interpretation.
- `P6-QUALITY-002`: scoped Recall@10 and complete-evidence result, preserving
  the 16 losses.
- `P6-QUALITY-003`: candidate reduction and retained-evidence result, explicitly
  identified as a mean per-query ratio.
- `P6-MAC-EXACT-001`: Apple M1 Max unfiltered exact-search P50 ratios.
- `P6-MAC-EXACT-002`: Apple M1 Max filtered exact-search P50 ratios.
- `P6-MAC-CORRECTNESS-001`: frozen exact correctness gates.
- `P6-ANN-NEGATIVE-001`: USearch recall-gate failure and timing
  disqualification.
- `P6-DEVICE-001`: supported-product and graph-free device qualification.
- `P6-DEVICE-SAFETY-001`: 100K `not_run_device_safety` disclosure and
  ineligibility.

Prohibited:

- `P6-PROHIBITED-001`: universal RetrievalKit superiority.
- `P6-PROHIBITED-002`: a RetrievalKit-versus-USearch performance advantage.
- `P6-PROHIBITED-003`: a graph performance-winner claim.
- `P6-PROHIBITED-004`: 100K physical-device support or pass.
- `P6-PROHIBITED-005`: a combined winner table for non-equivalent graph apps.
- `P6-PROHIBITED-006`: treating embedding latency as part of reported
  retrieval latency.

Withheld:

- `P6-WITHHELD-001`: older-iPhone qualification.
- `P6-WITHHELD-002`: energy or sustained thermal superiority.
- `P6-WITHHELD-003`: redistribution of raw HotpotQA-derived data or raw device
  captures.
- `P6-WITHHELD-004`: automatic transfer to later RetrievalKit or dependency
  revisions.

Claims expire on 2027-07-21 and rerun sooner after any relevant source,
benchmark, workload, dependency, model, dataset, qrels, hardware, OS, timing,
or licensing change.

## Licensing and exclusions

The audit records HotpotQA as CC BY-SA 4.0, MiniLM and USearch as Apache 2.0,
NumPy as BSD-3-Clause, sqlite-vec as MIT or Apache 2.0, and SQLite as public
domain, with pinned versions and primary-source links. Raw HotpotQA and
transformed corpus payloads, MiniLM weights, raw device evidence and
identifiers, binaries, rejected partials, and disqualified timing evidence are
not copied into the publication root.

The repository has no root project license. The explicit task authorizes these
owner-authored repository artifacts, but no general downstream redistribution
grant is inferred. External distribution is withheld until the owner adds an
applicable project license and required notices.

## Package inventory

The closed ten-file publication root contains:

- `methodology.md`
- `retrieval-quality.md`
- `mac-systems-performance.md`
- `physical-device-systems-performance.md`
- `claim-register.json`
- `licensing.json`
- `evidence-index.json`
- `reproduction.md`
- `checksums.json`
- `manifest.json`

The adjacent `phase6-publication-v1-validation.json` records the independent
PASS without creating a self-referential publication root.

## Verification

Phase 6-specific verification includes Python compilation, Ruff, strict mypy,
12 mutation tests, full evidence integration, direct independent validation,
local Markdown-link and inventory checks, and three byte-identical generated
roots (checked-in plus two fresh temporary roots). Both fresh roots independently
validated to canonical artifact-set SHA-256
`8c935925dbc48e82fcdf6bcfb83e3c878dce5f7d488ef6c11a332dfa465ead90`.

Repository-wide Rust formatting, warnings-denied Clippy, workspace/all-feature
tests, applicable Swift builds/tests, native symbol isolation, and quickstarts
were run before the scoped commit. The completion handoff records the exact
commands and final commit identity.

## Remaining risk

The results remain bounded to the frozen datasets, workloads, revisions,
systems, hardware, and OS builds. No older iPhone, energy, thermal-efficiency,
100K support, USearch timing, or equivalent graph-performance comparison is
available. External publication additionally requires a repository license,
notices/attribution review, and an owner decision. Those limitations are
represented as prohibited or withheld claims rather than implied away.
