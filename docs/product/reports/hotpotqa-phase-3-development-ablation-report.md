# HotpotQA Phase 3 Development Ablation Report

Status: Phase 3a development selection complete; Phase 3b locked reporting pending

Date: 2026-07-17

This is a development-only engineering report. It is not a final benchmark,
device-performance, product, or marketing claim. The locked test split was not
opened, searched, scored, sliced, or inspected.

## Frozen protocol and selection

The pre-registered search space is
`benchmarks/retrieval-quality/hotpotqa/phase-3-development-search-space.json`,
SHA-256
`30a93141c0b36d446617342ae846ff4174ff1f8b0f0f9cf008882ed6f3cbdeca`.
Only development Run C selected the configuration. The ordered objective was:

1. Complete Evidence Recall@10, descending;
2. NDCG@10, descending;
3. MAP, descending;
4. Recall@10, descending;
5. MRR@10, descending;
6. total candidate count, ascending;
7. maximum component candidate count, ascending; and
8. canonical configuration bytes, ascending.

The selected candidate is weighted hybrid alpha `0.2`, vector candidate limit
`100`, and keyword candidate limit `100`. Its primary development value was
Complete Evidence Recall@10 `0.746268656716418`; the runner-up used alpha
`0.2`, vector `50`, keyword `100`, and scored `0.7429519071310116`. The primary
criterion was therefore decisive; no later tie break selected the winner.

The checked-in lock is
`benchmarks/retrieval-quality/hotpotqa/phase-3-selected-configuration.json`,
SHA-256
`ec4757562140b92f298c85341ab64442dfcb07634da500e8abfe291401b95118`.
Its replay is byte-identical, and C and G use the same alpha and candidate
limits.

### All registered candidates

The table is ordered by alpha, vector limit, then keyword limit. CER is Complete
Evidence Recall@10. Values are shown to six decimals; selection used the full
stored values.

| # | Alpha | Vector | Keyword | CER@10 | NDCG@10 | MAP |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 0.2 | 25 | 25 | 0.734660 | 0.861886 | 0.807189 |
| 2 | 0.2 | 25 | 50 | 0.734660 | 0.862441 | 0.808658 |
| 3 | 0.2 | 25 | 100 | 0.737977 | 0.863560 | 0.809461 |
| 4 | 0.2 | 50 | 25 | 0.736318 | 0.861792 | 0.807150 |
| 5 | 0.2 | 50 | 50 | 0.742952 | 0.863588 | 0.808489 |
| 6 | 0.2 | 50 | 100 | 0.742952 | 0.863843 | 0.808819 |
| 7 | 0.2 | 100 | 25 | 0.731343 | 0.860277 | 0.806295 |
| 8 | 0.2 | 100 | 50 | 0.742952 | 0.862866 | 0.807569 |
| 9 | 0.2 | 100 | 100 | 0.746269 | 0.863770 | 0.807943 |
| 10 | 0.4 | 25 | 25 | 0.721393 | 0.858520 | 0.803533 |
| 11 | 0.4 | 25 | 50 | 0.733002 | 0.861582 | 0.806236 |
| 12 | 0.4 | 25 | 100 | 0.739635 | 0.862783 | 0.806797 |
| 13 | 0.4 | 50 | 25 | 0.711443 | 0.855994 | 0.801102 |
| 14 | 0.4 | 50 | 50 | 0.721393 | 0.858259 | 0.803654 |
| 15 | 0.4 | 50 | 100 | 0.731343 | 0.860495 | 0.805232 |
| 16 | 0.4 | 100 | 25 | 0.703151 | 0.852689 | 0.797525 |
| 17 | 0.4 | 100 | 50 | 0.714760 | 0.856235 | 0.801443 |
| 18 | 0.4 | 100 | 100 | 0.723051 | 0.858547 | 0.804041 |
| 19 | 0.6 | 25 | 25 | 0.698176 | 0.836569 | 0.772239 |
| 20 | 0.6 | 25 | 50 | 0.699834 | 0.837392 | 0.774171 |
| 21 | 0.6 | 25 | 100 | 0.708126 | 0.838516 | 0.774384 |
| 22 | 0.6 | 50 | 25 | 0.679934 | 0.833215 | 0.769596 |
| 23 | 0.6 | 50 | 50 | 0.681592 | 0.833639 | 0.770726 |
| 24 | 0.6 | 50 | 100 | 0.689884 | 0.834827 | 0.770935 |
| 25 | 0.6 | 100 | 25 | 0.661692 | 0.830336 | 0.768406 |
| 26 | 0.6 | 100 | 50 | 0.668325 | 0.831529 | 0.769455 |
| 27 | 0.6 | 100 | 100 | 0.673300 | 0.832665 | 0.770665 |
| 28 | 0.8 | 25 | 25 | 0.623549 | 0.799946 | 0.732334 |
| 29 | 0.8 | 25 | 50 | 0.626866 | 0.800364 | 0.732740 |
| 30 | 0.8 | 25 | 100 | 0.631841 | 0.800612 | 0.732356 |
| 31 | 0.8 | 50 | 25 | 0.603648 | 0.797209 | 0.729603 |
| 32 | 0.8 | 50 | 50 | 0.606965 | 0.797803 | 0.730250 |
| 33 | 0.8 | 50 | 100 | 0.611940 | 0.798186 | 0.730040 |
| 34 | 0.8 | 100 | 25 | 0.603648 | 0.798936 | 0.729889 |
| 35 | 0.8 | 100 | 50 | 0.603648 | 0.799023 | 0.729902 |
| 36 | 0.8 | 100 | 100 | 0.610282 | 0.800407 | 0.730747 |

## Development A–G matrix

A–C executed all 603 declared development queries. D–G declared 603 and
executed 599, with exactly four pre-freeze ambiguous-seed exclusions. There
were no explicit-seed runs, fallback seeds, expected paths, invented path
judgments, filters, truncations, empty scopes, or invalid executions.

| Run | Development run ID |
|---|---|
| A | `v3-a-whole-semantic-f32-na-cfg-4286d9495e78` |
| B | `v3-b-whole-semantic-i8-na-cfg-7ff37736e033` |
| C | `v3-c-whole-weighted-i8-na-cfg-706ba354a7b7` |
| D | `v3-d-selection-none-none-hotpotqa-exact-title-v1-cfg-39836d11bb48` |
| E | `v3-e-graph-semantic-f32-hotpotqa-exact-title-v1-cfg-0cc25296858e` |
| F | `v3-f-graph-semantic-i8-hotpotqa-exact-title-v1-cfg-81cd8b4967c9` |
| G | `v3-g-graph-weighted-i8-hotpotqa-exact-title-v1-cfg-877b62e67e72` |

### Aggregate retrieval and evidence metrics

| Run | NDCG@10 | Recall@10 | MRR@10 | MAP | Success@1 | CER@10 | Candidate recall |
|---|---:|---:|---:|---:|---:|---:|---:|
| A | 0.767247 | 0.762852 | 0.961305 | 0.690127 | 0.943615 | — | — |
| B | 0.767581 | 0.764511 | 0.960436 | 0.690025 | 0.941957 | — | — |
| C | 0.863770 | 0.873134 | 0.987424 | 0.807943 | 0.976783 | 0.746269 | — |
| E | 0.903621 | 0.958264 | 0.981636 | 0.839246 | 0.968280 | 0.919866 | 0.974958 |
| F | 0.903291 | 0.958264 | 0.980801 | 0.838941 | 0.966611 | 0.919866 | 0.974958 |
| G | 0.933229 | 0.970785 | 0.985671 | 0.888192 | 0.976628 | 0.944908 | 0.974958 |

D/E/F/G produced 599 logically identical selections and 13,400 identical path
rows per run. The macro candidate reduction ratio was `928.592856`. Across the
599 executed queries, graph projection reduced 7,589,330 eligible-query chunks
to 13,400 projected chunks. Candidate recall was `0.974958`; 28 queries lacked
at least one supporting document in scope, while candidate complete-evidence
rate was `0.953255`.

### Paired graph effects

Paired values use the common 599-query graph population, so a baseline value
may differ slightly from its 603-query aggregate above.

| Pair / metric | Baseline | Scoped | Absolute delta | Relative delta | W/T/L |
|---|---:|---:|---:|---:|---:|
| A/E NDCG@10 | 0.766278 | 0.903621 | +0.137343 | +17.92% | 362/214/23 |
| A/E CER@10 | 0.535893 | 0.919866 | +0.383973 | +71.65% | 254/321/24 |
| B/F NDCG@10 | 0.766615 | 0.903291 | +0.136676 | +17.83% | 362/214/23 |
| B/F CER@10 | 0.539232 | 0.919866 | +0.380634 | +70.59% | 251/325/23 |
| C/G NDCG@10 | 0.862861 | 0.933229 | +0.070369 | +8.16% | 241/325/33 |
| C/G CER@10 | 0.744574 | 0.944908 | +0.200334 | +26.91% | 148/423/28 |
| C/G MRR@10 | 0.987340 | 0.985671 | -0.001669 | -0.17% | 3/592/4 |

Graph scoping lost 26, 25, and 30 baseline top-10 evidence documents in the
A/E, B/F, and C/G comparisons respectively. The canonical paired artifacts
retain every affected query ID. The result is positive on development for
NDCG, recall, and complete evidence overall, but it is not uniformly positive:
C/G has 33 NDCG losses, and MRR declines slightly.

### Encoding and hybrid effects

A/B NDCG@10 changed by `+0.000334618` (`+0.0436%`) with 5 wins, 595 ties,
and 3 losses; eight query IDs were affected. E/F NDCG@10 changed by
`-0.000330240` (`-0.0365%`) with 1 win, 596 ties, and 2 losses; three query IDs
were affected. Across both comparisons, 10 distinct queries had an I8 ranking
divergence. Recall@10 changed by `+0.001658375` for A/B and exactly `0` for E/F.

On the complete 603-query whole population, B/C hybridization changed NDCG@10
by `+0.096188912`, Recall@10 by `+0.108623549`, and MAP by `+0.117917640`.
On the 599-query graph population, F/G changed the same metrics by
`+0.029938338`, `+0.012520868`, and `+0.049251366`. The error analysis still
finds 93 queries where one of the pre-registered B/C or F/G NDCG comparisons
regressed.

## Slices and error analysis

The declared population contains 577 bridge and 26 comparison questions; 87
easy, 413 medium, and 103 hard questions. Exact categories are 80 bridge/easy,
98 bridge/hard, 399 bridge/medium, 7 comparison/easy, 5 comparison/hard, and
14 comparison/medium. All 603 questions have two supporting documents.

For the 599 resolved graph queries, scope sizes were 113 in `1-10`, 485 in
`11-100`, and one in `101-1000`. Candidate-recall buckets were 2 at `0`, 26 at
`[0.5,1)`, and 571 at `1`. Filter selectivity and path accuracy are
`not_applicable`; all 599 graph traversals have no truncation. Per-category
NDCG@10 and CER@10 for every run are retained in the independent report. One
notable negative slice is comparison questions: graph CER@10 is `0` for easy
and medium comparison categories and `0.2` for hard comparison questions.

Closed error-category counts are:

- seed ambiguity exclusion: 4;
- empty graph scope: 0;
- truncated graph selection: 0;
- supporting evidence absent from scope: 28;
- evidence in scope but missing from top 10: 5;
- whole-corpus ranking failure: 8;
- graph-scoped ranking failure: 48;
- I8 ranking divergence: 10;
- hybrid ranking regression: 93;
- duplicate-collapse effect: 0; and
- unexpected execution failure: 0.

The independent artifact retains the complete affected-query ID lists.

## Independent and external validation

`scripts/quality/validate_hotpotqa_phase_3.py` independently validates the
development input hashes, 36-candidate closure, mechanical winner, lock,
inventories, run configurations and populations, exclusions, TREC rankings,
all supported per-query and aggregate metrics, graph selections/paths,
persistence, slices, errors, deterministic encoding, and absence of test
access. Its maximum absolute difference was `3.219646771412954e-15` for tuning
and `3.0531133177191805e-15` for the A–G matrix.

Two fresh complete 39-file A–G roots from executable SHA-256
`7b5f13a453b1fbc85ad3cf753b6feb54d99f527b98306fa2ab51f2050e332065`
and Git commit `60eb5cacd2b24cccb74d6595953fdd30e377937b` were recursively
byte-identical. Their inventory identity is
`26cbd1d0b7b22eb98faadd59d4554aacd10cdd8c767cc1c92adc7553e1026d67`.

Pinned `ir_measures==0.4.3` matched every supported per-query and aggregate
metric exactly. Official NIST `trec_eval` 10.0-rc3 at upstream commit
`f4253652c8efd0d86ddffd0d163cc0a0f813111a` also matched every supported value
exactly. Official `trec_eval` has no exact mapping for Judged@5/10 or the
graph/evidence metrics; those are explicitly unsupported, not silently
approximated. Path accuracy is `not_applicable` because no expected paths
exist.

## Corrections encountered during qualification

Full qualification exposed three evaluation-tooling defects, each corrected
in a separate commit: exact JSON float parsing for the frozen lock, precedence
of the adapter's upstream-frozen ambiguous-seed exclusions over retained-corpus
alias replay, and fixture-specific seed-lane inference in D/E/F/G equality
checking. The selected candidate and lock bytes never changed. Failed attempts
published no result root.

## Test-access evidence and next task

The matrix audit records only `opened_splits: ["development"]`, with
`test_collection_opened`, `test_metrics_inspected`, and
`test_artifacts_generated` all `false`. Every Phase 3a entry point rejects a
path containing a `test` component before collection access. No test ranking,
metric, slice, error, or retrieval artifact exists in the Phase 3a roots.

Phase 3 is active. Phase 3a is complete. Phase 3b locked reporting is pending,
and Phase 4 is inactive. The exact next task is a separate, one-shot Phase 3b
execution against the sealed test split using the immutable selected-
configuration lock, with no retuning or development-driven change.
