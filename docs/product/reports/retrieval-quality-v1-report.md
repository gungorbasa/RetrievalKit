# VectorKit Retrieval Quality V1: Vector Only and Hybrid

Status: historical baseline. V2 expands this fixture to 42 harder judged
queries and is the active quality report.

Date: 2026-07-11

## Executive Summary

| Mode | Production configuration | Human relevance | I8 agreement with F32 | Decision |
|:---|:---|---:|---:|:---|
| Vector only | I8 exact search | 1.0000 | 0.9833 at top 5; 1.0000 at top 10 | Pass |
| Hybrid | I8 with `50/50` candidates | 1.0000 | 0.9500 at top 5 | Pass; keep default |

Both modes returned every judged-relevant document, kept the correct top result,
respected filters, excluded deleted data, returned replacement data, and
preserved rankings after persistence reload. I8 meets the configured 0.95 F32
agreement gate for both modes.

The two agreement numbers answer different questions:

- Vector-only agreement compares I8 exact vector results directly with F32
  exact vector results. BM25 is not involved.
- Hybrid agreement compares the final fused I8 result list with the most
  thorough F32 hybrid configuration, using 100 vector and 100 keyword
  candidates.

## Setup

- Fixture: `personal-workspace-v1`.
- 282 documents: 25 authored documents, one replacement document, and 256
  deterministic realistic distractors.
- 12 human-judged queries covering exact names, semantic paraphrases, metadata
  filters, deletion, and replacement.
- Embeddings: `sentence-transformers/all-MiniLM-L6-v2`, 384 dimensions.
- Encodings: F32 and I8 scalar quantization.
- Fusion: reciprocal-rank fusion with `rrf_k=60`.
- Final results: top 5.
- Candidate pairs: `10/25`, `25/25`, `50/50`, and `100/100`.
- Reference: F32 `100/100` plus a same-encoding `100/100` comparison.
- Vector-only reference: exact F32 search over the same MiniLM embeddings, with
  no keyword query, BM25 candidates, or rank fusion.
- Latency: 50 iterations over all 12 queries, 600 samples per row, Release
  build on Apple Silicon macOS.

## Hybrid Search Quality

Every candidate pair achieved relevance recall@5, MRR, and NDCG@5 of `1.0`
against the current human judgments. Candidate overlap distinguishes how much
of the full reference ranking each smaller pair preserves:

| Encoding | Candidates | Recall vs same-encoding 100/100 | Recall vs F32 100/100 | P95 ms |
|:---|:---|---:|---:|---:|
| F32 | 10/25 | 0.7500 | 0.7500 | 0.142 |
| F32 | 25/25 | 0.9167 | 0.9167 | 0.115 |
| F32 | 50/50 | 0.9667 | 0.9667 | 0.131 |
| F32 | 100/100 | 1.0000 | 1.0000 | 0.158 |
| I8 | 10/25 | 0.7500 | 0.7333 | 0.086 |
| I8 | 25/25 | 0.9167 | 0.9000 | 0.091 |
| I8 | 50/50 | 0.9667 | 0.9500 | 0.109 |
| I8 | 100/100 | 1.0000 | 0.9833 | 0.135 |

The configured default gate requires I8 recall of at least `0.95` against the
F32 high-candidate reference. `50/50` meets it exactly. `25/25` and `10/25`
do not. Keep `50/50` as the V1 default and retain smaller values as explicit
per-query latency overrides.

Hybrid conclusion: `50/50` is the smallest tested candidate pair that passes
the I8-versus-F32 quality gate. The smaller pairs still found all currently
judged answers, but changed too much of the full result list to become defaults.

## Vector-Only F32 Versus I8

The vector-only path calls exact vector search directly. Candidate pairs such
as `50/50` do not apply because there is no BM25 list to fuse.

| Encoding | Depth | Recall vs F32 | Top-1 agreement | Exact ordered-list agreement | Relevance recall | P95 ms |
|:---|---:|---:|---:|---:|---:|---:|
| F32 | 5 | 1.0000 | 1.0000 | 1.0000 | 1.0000 | 0.027 |
| I8 | 5 | 0.9833 | 1.0000 | 0.9167 | 1.0000 | 0.007 |
| F32 | 10 | 1.0000 | 1.0000 | 1.0000 | 1.0000 | 0.032 |
| I8 | 10 | 1.0000 | 1.0000 | 0.9167 | 1.0000 | 0.009 |

At top 5, I8 retained 59 of the 60 F32 results across 12 queries. The
only set difference was the fifth result for `exact-incident`; both encodings
still returned the judged incident document first. At top 10, I8 returned the
same documents as F32 for every query. The exact-order score is `0.9167` because
that same query swapped the fifth and sixth documents.

This confirms the earlier synthetic result with realistic MiniLM embeddings:
I8 preserves approximately 98-100% of F32 vector-only results on this fixture,
without changing the best result or losing a judged-relevant document.

Vector-only conclusion: I8 is suitable as the default compact encoding. The
observed difference was below the correct first result and did not affect human
relevance. An additional F16 or F32 reranking store is not justified by this
fixture.

## Persistence And Payload

| Encoding | Estimated in-memory payload | Persisted size | Load | Rankings after load |
|:---|---:|---:|---:|:---|
| F32 | 625,595 bytes | 454,444 bytes | 5.315 ms | identical |
| I8 | 300,711 bytes | 129,574 bytes | 3.136 ms | identical |

Hybrid and vector-only rankings were identical before and after reload for both
encodings. Deletion and replacement checks produced zero lifecycle violations
for every row. The deleted legacy OAuth guide never returned, and the
remote-work-policy query resolved the replacement text rather than the
tombstoned draft.

## Overall Decision

- Ship I8 as the compact production encoding for both vector-only and hybrid
  retrieval.
- Keep hybrid search at `50/50` candidates by default.
- Keep smaller hybrid candidate pairs as explicit latency-oriented overrides.
- Do not add a second reranking vector store based on the current evidence.
- Keep F32 as the correctness reference in regression benchmarks.

## Limitations

- Current human judgments are intentionally conservative and small. Their
  perfect relevance scores show that the fixture's positive cases are easy for
  MiniLM and hybrid retrieval; they do not prove broad production quality.
- Reference overlap is the binding default-candidate signal. It includes the
  full top-5 ranking, not only judged-positive documents, so it detects ranking
  drift that sparse relevance judgments would otherwise miss.
- The corpus is large enough for `100/100` to be meaningfully different from
  smaller candidate pools. Earlier 26-document experiments were discarded.
- I8 `100/100` retains `0.9833` of the F32 reference, above the product's 0.95
  real-data gate.
- The latency measurements come from a small 282-document corpus on Apple
  Silicon macOS. The physical-device reports remain the source of truth for
  mobile latency and memory budgets.

## Next Quality Work

Add queries and graded judgments from real application usage before changing
fusion weights or reducing the default candidates. The next fixture version
should emphasize ambiguous queries with multiple partially relevant results,
because those will make NDCG and MRR more discriminating than this V1 baseline.
