# Vector, BM25, And Hybrid Retrieval Plan

Status: implemented. The active V1 defaults are I8 vector storage, `50/50`
candidates, and weighted normalized fusion with query-time `alpha=0.6`.
Refer to `retrievalkit-product-spec.md` for current decisions.

This note captures the current direction for improving RetrievalKit retrieval
quality and speed. It is library-level guidance, not specific to any one
example dataset.

## Goals

- Keep RetrievalKit local-first and exact for V1.
- Improve retrieval quality across semantic, keyword, and mixed queries.
- Keep behavior deterministic and explainable.
- Measure embedding latency separately from retrieval latency.
- Return chunk-level results by default, with enough trace data to debug rank.

## Vector Search

Vector search should remain the semantic baseline.

- Normalize vectors at insert time when using cosine similarity.
- Normalize the query vector once before scoring.
- Store vectors in contiguous layouts for fast exact scans.
- Prefer direct numeric chunk IDs internally.
- Apply metadata filters before vector scoring when possible.
- Support multiple encodings, but treat `f32` as the quality baseline.
- Benchmark recall loss for compressed encodings like `f16`, `bf16`, and `i8`.
- Expose vector score and vector rank in result traces.

Important quality rule:

```text
Encoded vector search must be tested against exact f32 search.
```

## BM25 Search

BM25 should be a first-class internal lexical component, not a fallback text scan.

- Maintain a real inverted index.
- Tokenize deterministically.
- Store term frequency per chunk.
- Store document/chunk length.
- Store average chunk length.
- Store document frequency per term.
- Use stable defaults:

```text
k1 = 1.2
b = 0.75
```

BM25 result traces should include:

- raw BM25 score
- BM25 rank
- matched terms
- term contribution details where practical

Tokenizer behavior should be explicit:

- lowercase
- split punctuation
- deterministic Unicode handling
- stopword policy documented
- stemming optional later, not required for V1

## Hybrid Retrieval

Hybrid search combines vector and BM25 candidate sets, then fuses ranks or
scores.

Current V1 pipeline:

```text
query
-> metadata-prefiltered vector top N
-> metadata-prefiltered BM25 top N
-> union candidates
-> fuse scores or ranks
-> deterministic tie-break
-> return top K with trace
```

Suggested defaults:

```text
vector_top_k = 50
bm25_top_k = 50
final_top_k = 10
```

These are defaults, not fixed behavior. The Rust public API exposes
`HybridQuery::with_candidate_limits(vector_top_k, keyword_top_k)` so callers can
tune latency and candidate breadth per query.

## Fusion Strategy

Weighted normalized score fusion is the current default. It min-max normalizes
vector and BM25 scores across the fused candidate set, then applies query-time
`alpha`:

```text
hybrid_score =
  alpha * normalized_vector_score
  +
  (1 - alpha) * normalized_bm25_score
```

Default:

```text
alpha = 0.6
```

Reciprocal Rank Fusion remains available internally for benchmark comparisons:

```text
hybrid_score =
  1 / (rrf_k + vector_rank)
  +
  1 / (rrf_k + bm25_rank)
```

Common benchmark baseline:

```text
rrf_k = 60
```

Both strategies must remain deterministic and explainable in traces.

## Current Performance Takeaways

The local synthetic benchmark after BM25 optimizations shows:

- BM25 candidate generation is no longer the primary hybrid bottleneck.
- Filtered hybrid is fast when metadata prefiltering narrows the candidate set.
- Unfiltered hybrid latency is dominated by vector candidate count.
- Keyword candidate count is comparatively cheap.

Do not change default candidate limits only from latency numbers. First add a
same-encoding comparison against a high-candidate reference so smaller candidate
limits can be evaluated for final-result quality.

## Result Trace

Hybrid results should expose enough data to explain ranking.

Example trace shape:

```json
{
  "vector_score": 0.82,
  "vector_rank": 3,
  "normalized_vector_score": 0.61,
  "bm25_score": 7.4,
  "bm25_rank": 1,
  "normalized_bm25_score": 1.0,
  "hybrid_score": 0.766,
  "matched_terms": ["erica", "bar"],
  "fusion": "weighted_normalized_score",
  "vector_weight": 0.6,
  "keyword_weight": 0.4,
  "filter_matched": true
}
```

RRF traces should include `fusion = "rrf"` and `rrf_k`.

Missing ranks should be represented explicitly. For example, a candidate found
only by BM25 should have no vector rank rather than a fake vector score.

## Filtering

Filters should apply consistently across retrieval modes.

- Vector search must not return filtered-out chunks.
- BM25 search must not return filtered-out chunks.
- Hybrid search should apply the same filter to both candidate generators.
- Filtering should happen before expensive scoring when the index layout makes
that practical.

## Deterministic Tie-Breaks

When scores tie, ranking should remain stable.

Suggested tie-break order:

```text
hybrid_score desc
vector_rank asc if present
bm25_rank asc if present
chunk_id asc
```

## Optional Quality Features

These are useful, but not required before the core vector/BM25/hybrid path is
solid.

- Field boosts for metadata like `content_type`.
- Exact phrase boost in BM25.
- Optional grouping by document.
- Optional deduplication by document or caller-provided group key.
- Query-time candidate expansion.
- Lightweight reranking over hybrid candidates.

Chunk results should remain the default. Grouping and deduplication should be
explicit options.

## Benchmark Plan

Compare these modes:

```text
vector only
BM25 only
hybrid RRF
hybrid weighted normalized score fusion
```

Measure:

- Recall@K
- MRR
- NDCG
- retrieval-only latency
- filter time
- vector scoring time
- BM25 lookup time
- fusion time
- memory and persistence size

Benchmark rules:

- Separate query embedding latency from retrieval latency.
- Use exact f32 vector search as the quality baseline.
- Test compressed vector encodings against f32 recall.
- Use deterministic fixtures and stable queries.

## Recommended Next Steps

1. Add same-encoding hybrid candidate-quality benchmarks comparing small
   candidate limits against a high-candidate reference.
2. Add fixture-backed benchmarks with realistic chunk text, metadata, and BM25
   term distributions.
3. Carry the public candidate-limit override into language wrappers.
4. Revisit default candidate limits only after real-data quality and latency
   benchmarks support a change.
