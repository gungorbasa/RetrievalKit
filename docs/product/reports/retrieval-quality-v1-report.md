# Retrieval Quality V1 Report

Date: 2026-07-11

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
- Latency: 50 iterations over all 12 queries, 600 samples per row, Release
  build on Apple Silicon macOS.

## Quality Results

Every candidate pair achieved relevance recall@5, MRR, and NDCG@5 of `1.0`
against the current human judgments. Candidate overlap distinguishes how much
of the full reference ranking each smaller pair preserves:

| Encoding | Candidates | Recall vs same-encoding 100/100 | Recall vs F32 100/100 | P95 ms |
|:---|:---|---:|---:|---:|
| F32 | 10/25 | 0.7500 | 0.7500 | 0.153 |
| F32 | 25/25 | 0.9167 | 0.9167 | 0.115 |
| F32 | 50/50 | 0.9667 | 0.9667 | 0.130 |
| F32 | 100/100 | 1.0000 | 1.0000 | 0.159 |
| I8 | 10/25 | 0.7500 | 0.7333 | 0.087 |
| I8 | 25/25 | 0.9167 | 0.9000 | 0.092 |
| I8 | 50/50 | 0.9667 | 0.9500 | 0.108 |
| I8 | 100/100 | 1.0000 | 0.9833 | 0.135 |

The configured default gate requires I8 recall of at least `0.95` against the
F32 high-candidate reference. `50/50` meets it exactly. `25/25` and `10/25`
do not. Keep `50/50` as the V1 default and retain smaller values as explicit
per-query latency overrides.

## Persistence And Payload

| Encoding | Estimated in-memory payload | Persisted size | Load | Rankings after load |
|:---|---:|---:|---:|:---|
| F32 | 625,595 bytes | 454,445 bytes | 3.233 ms | identical |
| I8 | 300,711 bytes | 129,575 bytes | 3.433 ms | identical |

Deletion and replacement checks produced zero lifecycle violations for every
row. The deleted legacy OAuth guide never returned, and the remote-work-policy
query resolved the replacement text rather than the tombstoned draft.

## Interpretation

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

## Next Quality Work

Add queries and graded judgments from real application usage before changing
fusion weights or reducing the default candidates. The next fixture version
should emphasize ambiguous queries with multiple partially relevant results,
because those will make NDCG and MRR more discriminating than this V1 baseline.
