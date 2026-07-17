# HotpotQA Phase 3 Locked Reporting Report

Status: Phase 3b locked reporting complete; all protocol gates passed

Date: 2026-07-17

This is a sealed-test engineering report, not a device-performance, product, or
marketing claim. It reports the pre-registered A–G quality ablation using the
development-selected configuration without retuning. Phase 4 did not begin.

## Authorization, attempts, and execution identity

The successful authorization is
`benchmarks/retrieval-quality/hotpotqa/phase-3b-execution-authorization.json`,
SHA-256
`5831cdc6e145225e37ff546185ced0e9b9217533820635160a826767651d8263`.
It binds attempt sequence 3, evaluator commit
`2de01754baa5f7452f1162900c2f50cae7d17c04`, the test and derived
populations, the selected lock, A–G, the frozen slices and errors, and the
external evaluator identities.

Three disclosed full attempts exist. Failed attempts published no canonical
result root and remain recorded outside the result root.

| Attempt | Authorization SHA-256 | Execution commit | Outcome |
|---:|---|---|---|
| 1 | `31fd8b92a6930c50f21c533e5123681cd6b2eb8295f56981ef625b30f5ace71a` | `33ef312f107abcb7179f8e10cd003381c6e0c358` | Failed before ranking or label access because the prompt's repeated collection identity was initially used as the population identity. No result root was published. |
| 2 | `15cd90a683694bb8f9ba69ec1fed0ced54261e97fb4bf1bf94ce67617d369ce6` | `fac4d44f3cde6ed899c72e17a378477d225ecea6` | Stage A completed and labels opened after the seal, but Stage B rejected canonical Rust decimal notation using Python's different exponent notation. No result root was published. |
| 3 | `5831cdc6e145225e37ff546185ced0e9b9217533820635160a826767651d8263` | `4e52dbd6946e1cbf965e7f9736d9eb47d12fff3e` | Passed and atomically published the sole canonical result root. |

Both failures were stopped and disclosed. Each correction was committed before
a new authorization; attempts were never compared to choose a preferred
quality result. The successful execution used:

- executable: `target/release/vectorkit`;
- executable SHA-256:
  `5f4f4fd6ce0d9083814233212ef5a5d1284a519b0d3c4408be7240721cb4fb15`;
- source tree: `ce4d360a819528d4a46c40d9c180c903065862ca`;
- source state: clean;
- OS/architecture: macOS/aarch64;
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`; and
- deterministic environment: `LC_ALL=C`, `LANG=C`, `TZ=UTC`, and
  `RAYON_NUM_THREADS=1`.

After execution, the independent validator exposed a validator-only mismatch
between per-query and aggregate metric wrapper schemas. Commit `a02cdae`
corrected that reader and added regression coverage. Retrieval, scoring, the
authorization, and the sealed result root were not changed or rerun.

## Frozen inputs and selected configuration

The selected lock SHA-256 is
`ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118`.
Run C and Run G both use exact F32 alpha `0.2` (`3e4ccccd`), vector candidate
limit `100`, and keyword candidate limit `100`. The selected preimage, BM25,
normalization, and quantization hashes match the frozen Phase 3a lock. No
candidate grid, alternative alpha, query-specific parameter, filter, seed,
traversal, graph, embedding, or exclusion override was accepted or used.

The executed collection is
`hotpotqa-linked-abstracts-graph-v1-test@1`. Its collection identity is
`496d21d1c686e2ef3bc36d9820d0cda058f4ca6b82bb029889ed62b48b084f72`
and its adapter-manifest SHA-256 is
`8a9822e788eb81f2bb7f43b7c62c1690d45c64c8c698f37193706f8d0e67a3e6`.
The normative test-population SHA-256 is
`9b7532b17be9ca0df3d727fe911da4ff090dcd551535ba742f0a0df73a6f7010`;
the derived execution-population SHA-256 is
`93c252bd743e4084c7c50e9f7dee970af2977967a62c5717ba8edc000101a9d8`.
The prompt's accidental repeated collection identity was not used as a
population hash.

A–C declared and executed all 297 test queries. D–G declared 297 and executed
296 after exactly one upstream-frozen `derived_seed_ambiguous` exclusion.
There were no explicit-seed runs, fallback seeds, filters, expected paths, or
invalid executions.

## Label isolation and deterministic publication

Stage A opened only the collection manifest, records, corpus/query embeddings,
query text, exclusions, graph schema, and construction/seed manifests. Its
audit records no `qrels.tsv`, `evidence-judgments.jsonl`, or
`expected-paths.jsonl`, no forbidden file, and no previous result input.

After all A–G rankings, selections, paths, projections, statuses, and
persistence reports were complete, the primary and verification Stage A roots
were byte-identical. The finalized ranking-seal SHA-256 is
`90a0dd8ab2b9a3b575ad6e80366703fb8eb24dc01dd11d859645da00ccc9128c`.

Stage B verified that seal before opening exactly `qrels.tsv`,
`evidence-judgments.jsonl`, and `expected-paths.jsonl`. Its audit records
`retrieval_invoked: false`. It scored the same sealed rankings twice; the two
complete scored roots were byte-identical. The protocol therefore records both
`mandatory_ranking_rerun_equal` and `mandatory_scoring_rerun_equal` as true and
published one root atomically.

## Locked run identities

| Run | Locked run ID | Declared / executed |
|---|---|---:|
| A | `v3-a-whole-semantic-f32-na-cfg-d7a29d1b3756` | 297 / 297 |
| B | `v3-b-whole-semantic-i8-na-cfg-a319692d9e8d` | 297 / 297 |
| C | `v3-c-whole-weighted-i8-na-cfg-bf11b58fbccc` | 297 / 297 |
| D | `v3-d-selection-none-none-hotpotqa-exact-title-v1-cfg-58a10abece17` | 297 / 296 |
| E | `v3-e-graph-semantic-f32-hotpotqa-exact-title-v1-cfg-9ce20376e814` | 297 / 296 |
| F | `v3-f-graph-semantic-i8-hotpotqa-exact-title-v1-cfg-638879c1185b` | 297 / 296 |
| G | `v3-g-graph-weighted-i8-hotpotqa-exact-title-v1-cfg-6a1b35c03686` | 297 / 296 |

D/E/F/G selections are identical for every executed query. E/F/G each have
the same 6,326 path rows as D, with both `selection_equal` and `path_equal`
true. Path accuracy is `not_applicable` because the adapter intentionally has
no expected paths.

## Aggregate metrics

The following values use each run's frozen execution population. D is a
selection-only run and has no ranked retrieval metrics.

| Run | NDCG@5 | NDCG@10 | Recall@5 | Recall@10 | Success@1 | P@5 | MRR@10 | MAP | Judged@5 | Judged@10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A | 0.757592 | 0.769641 | 0.727273 | 0.757576 | 0.946128 | 0.290909 | 0.966105 | 0.697390 | 0.290909 | 0.151515 |
| B | 0.757010 | 0.768462 | 0.727273 | 0.755892 | 0.942761 | 0.290909 | 0.964254 | 0.696943 | 0.290909 | 0.151178 |
| C | 0.837808 | 0.857863 | 0.821549 | 0.872054 | 0.969697 | 0.328620 | 0.981089 | 0.799768 | 0.328620 | 0.174411 |
| E | 0.880651 | 0.899905 | 0.907095 | 0.954392 | 0.966216 | 0.362838 | 0.978322 | 0.834598 | 0.371509 | 0.216311 |
| F | 0.880031 | 0.899284 | 0.907095 | 0.954392 | 0.962838 | 0.362838 | 0.976633 | 0.834035 | 0.371509 | 0.216311 |
| G | 0.920320 | 0.927909 | 0.939189 | 0.957770 | 0.972973 | 0.375676 | 0.982264 | 0.886766 | 0.384347 | 0.216987 |

| Run | Supporting recall@5 | Supporting recall@10 | Complete evidence@5 | Complete evidence@10 |
|---|---:|---:|---:|---:|
| A | 0.727273 | 0.757576 | 0.464646 | 0.521886 |
| B | 0.727273 | 0.755892 | 0.464646 | 0.518519 |
| C | 0.821549 | 0.872054 | 0.646465 | 0.744108 |
| E | 0.907095 | 0.954392 | 0.820946 | 0.915541 |
| F | 0.907095 | 0.954392 | 0.820946 | 0.915541 |
| G | 0.939189 | 0.957770 | 0.885135 | 0.922297 |

Across D–G, candidate recall is `0.967905`, candidate complete-evidence rate is
`0.942568`, candidate reduction ratio is `972.654315`, and empty-scope rate is
`0`. The 296 resolved queries contain 3,750,320 eligible-query chunks before
scope projection and 6,326 projected chunks after it. Sixty-three scopes
contain 1–10 projected chunks and 233 contain 11–100. Seed-resolution coverage
is 296/297 declared queries and 296/296 executed graph queries. No selection
was truncated.

## Frozen paired effects

Graph comparisons use the common 296-query derived population; their baseline
values therefore differ slightly from the 297-query A–C aggregates.

| Pair | Metric | Baseline | Compared | Absolute | Relative | W/T/L |
|---|---|---:|---:|---:|---:|---:|
| A/E | NDCG@10 | 0.770169 | 0.899905 | +0.129735 | +16.85% | 171/111/14 |
| A/E | Recall@10 | 0.758446 | 0.954392 | +0.195946 | +25.84% | 171/111/14 |
| A/E | Complete evidence@10 | 0.523649 | 0.915541 | +0.391892 | +74.84% | 171/111/14 |
| B/F | NDCG@10 | 0.768987 | 0.899284 | +0.130297 | +16.94% | 170/112/14 |
| B/F | Recall@10 | 0.756757 | 0.954392 | +0.197635 | +26.12% | 170/112/14 |
| B/F | Complete evidence@10 | 0.520270 | 0.915541 | +0.395270 | +75.97% | 170/112/14 |
| C/G | NDCG@10 | 0.858036 | 0.927909 | +0.069873 | +8.14% | 121/157/18 |
| C/G | Recall@10 | 0.871622 | 0.957770 | +0.086149 | +9.88% | 121/157/18 |
| C/G | Complete evidence@10 | 0.743243 | 0.922297 | +0.179054 | +24.09% | 121/157/18 |

A/E recovered complete top-10 evidence on 128 queries and lost it on 12; B/F
recovered 129 and lost 12; C/G recovered 69 and lost 16. The canonical
analysis retains all affected-query IDs: 185 for A/E, 184 for B/F, and 139 for
C/G. The positive aggregate effect is not universal; all three comparisons
have query-level losses.

### I8 fidelity

A/B changes NDCG@10 by `-0.001179` (`-0.1532%`), Recall@10 by `-0.001684`,
and Complete Evidence Recall@10 by `-0.003367`, with 1/293/3 wins/ties/losses
and four document-metric-affected queries. E/F changes NDCG@10 by `-0.000621`
(`-0.0690%`) with 1/294/1, while Recall@10 and Complete Evidence Recall@10 are
unchanged; two queries are affected at the compared metric level.

The closed diagnostic category records native I8 ranking divergence for all
297 declared queries. This is not hidden: quantized native chunk order differs
even though projected document metrics are nearly identical. It must not be
misreported as exact ranking identity.

### Hybrid effects

B/C improves NDCG@10 by `+0.089401` (`+11.63%`), Recall@10 by `+0.116162`,
and Complete Evidence Recall@10 by `+0.225589`, with 120/148/29 and 149
affected queries. F/G improves NDCG@10 by `+0.028626` (`+3.18%`), Recall@10 by
`+0.003378`, and Complete Evidence Recall@10 by `+0.006757`, with 97/168/31
and 128 affected queries. The closed error analysis records 48 queries with a
pre-registered hybrid NDCG regression despite the positive aggregate effects.

## Frozen slices and closed errors

All 297 questions are hard and have two evidence documents: 282 are
`bridge:hard` and 15 are `comparison:hard`. Derived seed status is 296 resolved
and one ambiguous exclusion. Candidate-recall buckets contain two queries at
`0`, 15 at `[0.5,1)`, and 279 at `1`. Truncation reason is `none` for all 296
executed graph queries. Filter selectivity and path accuracy are both
`not_applicable` for all 297 declared queries. No category was added or removed
after results were seen.

Closed error-category counts are:

- seed ambiguity exclusion: 1;
- empty graph scope: 0;
- truncated graph selection: 0;
- supporting evidence absent from scope: 17;
- evidence in scope but missing from top 10: 6;
- whole-corpus ranking failure: 142;
- graph-scoped ranking failure: 25;
- I8 ranking divergence: 297;
- hybrid ranking regression: 48;
- duplicate-collapse effect: 0; and
- unexpected execution failure: 0.

These are diagnostic categories, not mutually exclusive query populations.
All affected IDs are retained in `locked-analysis.json`.

## Development-versus-test consistency

The locked result is directionally consistent with development. Development
versus test NDCG@10 gains are `+0.137343` versus `+0.129735` for A/E,
`+0.136676` versus `+0.130297` for B/F, and `+0.070369` versus `+0.069873` for
C/G. B/C hybrid NDCG@10 gain is `+0.096189` on development and `+0.089401` on
test; F/G is `+0.029938` and `+0.028626`. Candidate recall is `0.974958` on
development and `0.967905` on test. Graph scoping is positive in aggregate on
both splits, but both splits contain graph and hybrid regressions.

I8 fidelity remains close at projected-document metric level. A/B NDCG moved
slightly positive on development (`+0.000335`) and slightly negative on test
(`-0.001179`); E/F is slightly negative on both (`-0.000330` and `-0.000621`).
This split-level sign change is reported rather than rationalized away.

## Independent, external, and persistence validation

The independent validator passed authorization, inventory, canonical
serialization, configuration/no-tuning, graph equality, label isolation,
persistence, ranking seal, and populations. It recalculated expected values
from qrels and TREC rankings. Maximum absolute metric difference was
`1.7763568394002505e-15`, below `1e-9`. Its report SHA-256 is
`4317ab49c2c9671f9f11e91db7687855961038220e3480d2407f26ae2bdad1fb`.

Pinned `ir_measures==0.4.3` matched every supported per-query and aggregate
value exactly: both maxima are `0`. Official NIST `trec_eval` 10.0-rc3 at
upstream commit `f4253652c8efd0d86ddffd0d163cc0a0f813111a` also matched every
supported value exactly. Its executable SHA-256 is
`8f5d10550314dd401bb79fd215064e28a135c9535279d3d0288a07f4c3e51e5f`.
Judged@5/10, evidence/graph/candidate metrics, and path accuracy have no exact
official mapping and are explicitly unsupported; none is approximated. The
external report SHA-256 is
`91e0370b6f3c2635c7e13ee586536dccc372016d52e77de289b797aa766416fa`.

All A–C rankings are deterministic and equal after save/validate/load. D graph
persistence is equivalent. E–G generation, selection, path, projection, and
ranking outputs are all equal after reload. No persistence gate failed.

## Artifact identity and phase boundary

The ignored canonical root is
`target/benchmarks/hotpotqa-phase-3b/locked-reporting`. It contains 39 files:
38 manifest entries plus `manifest.json`. The artifact-root SHA-256 over the
38-entry inventory is
`e5d5824365d40745156701ba36744c1b7f764ce8fffb13245112b2c9ecb771c6`;
the manifest-file SHA-256 is
`2d6d47c5bab8d6de4598089bdbf197d7bec14400089f7f3328513748ea70d37e`.

Raw upstream data, generated collections, embeddings, indexes, per-query
results, sealed roots, labels, and validation reports remain ignored under
`target/`; none is tracked by Git. The frozen Phase 2 adapter contract, Phase
3a search space and selected lock, and checked-in V3 fixture were not modified.

Phase 3 is complete. No tuning occurred on test results, labels opened only
after the ranking seal, and Phase 4 did not begin. The exact next benchmark
task is Phase 4: pre-register and implement deterministic 10K, 25K, and 50K
target-device graph fixtures and the staged/end-to-end latency, cold-open,
save, validation, memory, and size protocol before executing device runs.
