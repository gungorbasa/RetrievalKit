# VectorKit Retrieval Quality V2: Harder Vector-Only and Hybrid Fixture

Date: 2026-07-11

## Executive Summary

V2 expands the original fixture from 12 to 42 judged queries and from 282 to
306 documents. It adds competing documents, near-duplicates, exact details,
ambiguous requests, filtered ambiguity, and multi-grade relevance judgments.

| Mode | Configuration | Relevance recall | MRR | NDCG | I8 agreement with F32 | Status |
|:---|:---|---:|---:|---:|---:|:---|
| Vector only | I8, top 5 | 0.9405 | 0.9881 | 0.9488 | 1.0000 | Pass |
| Vector only | I8, top 10 | 0.9683 | 0.9881 | 0.9535 | 0.9976 | Pass |
| Hybrid | I8, `50/50`, top 5 | 0.9028 | 1.0000 | 0.9272 | 0.9762 | Pass |

All configured gates pass. Both modes preserve the best F32 result for every
query. Filters, deletion, replacement, and persistence checks produce zero
lifecycle violations.

## Dataset

- 49 authored source documents (one deleted lifecycle case), one replacement
  document, and 256 deterministic distractors; 305 documents remain active.
- 42 human-authored queries across ten categories.
- Graded judgments from 1 (partially relevant) to 3 (best answer).
- MiniLM `all-MiniLM-L6-v2` normalized 384-dimensional embeddings.
- Exact F32 search is the encoding reference.
- Release build on Apple Silicon macOS, 50 iterations per query.

V2 inherits V1 and explicitly overrides judgments when new documents make an
old relevance set incomplete. Judgments are authored from document meaning and
are not generated from VectorKit rankings.

## Vector-Only Quality

Vector-only runs call exact vector search directly. BM25 and fusion are absent.

| Depth | Relevance recall | MRR | NDCG | Recall vs F32 | Top-1 agreement | Exact-order agreement | P95 ms |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 5 | 0.9405 | 0.9881 | 0.9488 | 1.0000 | 1.0000 | 1.0000 | 0.009 |
| 10 | 0.9683 | 0.9881 | 0.9535 | 0.9976 | 1.0000 | 0.8810 | 0.009 |

I8 and F32 return identical top-five lists for all 42 queries. At top 10, I8
retains 419 of 420 F32 results. Five queries contain ordering changes, but no
best result or judged-relevant result is lost because of quantization.

Decision: keep I8 as the production vector encoding and F32 as the benchmark
reference. V2 provides no evidence that a second F16/F32 reranking store is
needed.

## Hybrid Quality

Hybrid search uses RRF with `rrf_k=60`. The first candidate number is vector
depth and the second is BM25 depth.

| Candidates | Relevance recall | MRR | NDCG | Recall vs F32 `100/100` | P95 ms |
|:---|---:|---:|---:|---:|---:|
| `10/25` | 0.9187 | 1.0000 | 0.9323 | 0.8476 | 0.102 |
| `25/25` | 0.9187 | 1.0000 | 0.9318 | 0.9333 | 0.108 |
| `50/50` | 0.9028 | 1.0000 | 0.9272 | 0.9762 | 0.124 |
| `100/100` | 0.8968 | 1.0000 | 0.9223 | 1.0000 | 0.154 |

`50/50` remains the smallest tested pair that clears the configured 0.95
reference-overlap gate. It also clears the new 0.90 human relevance-recall gate.

However, the harder judgments show that larger candidate pools are not
automatically more relevant. On this fixture, `10/25` and `25/25` have slightly
better human relevance scores than `50/50`, while matching less of the full
F32 `100/100` result list. The F32 reference is an encoding and ranking-stability
baseline, not a claim that every reference result is human-optimal.

Decision: retain `50/50` as the provisional production default requested for
V1, but do not treat the decision as closed. Production-derived queries should
decide whether `25/25` offers a better real-world quality and latency tradeoff.

## Persistence and Payload

| Encoding | Estimated payload | Persisted size | Load | Hybrid reload | Vector reload |
|:---|---:|---:|---:|:---|:---|
| F32 | 684,300 bytes | 497,178 bytes | 5.709 ms | identical | identical |
| I8 | 331,864 bytes | 144,756 bytes | 2.646 ms | identical | identical |

## What V2 Fixes

- Query count increases from 12 to 42.
- Perfect relevance scores disappear, so regressions are now measurable.
- Ambiguous and multi-answer queries exercise graded NDCG.
- Near-duplicate documents compete for rank.
- A direct 0.90 relevance-recall gate complements F32-overlap gates.
- V1 remains immutable as a historical baseline.

## Remaining Limitation

V2 is realistic and substantially harder, but its queries are still curated.
The repository does not contain anonymized application query logs, failed
searches, or user relevance feedback. Those inputs are required before calling
the benchmark production-derived.
