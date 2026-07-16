# Retrieval Quality Evaluation Standard

Status: active evaluation guidance

Date: 2026-07-11

## Purpose

This document defines how VectorKit should decide whether vector-only, keyword,
hybrid, encoding, or ranking changes improve retrieval quality. It preserves the
V3 ideas discovered while building the V1 and V2 MiniLM fixtures and aligns the
next benchmark with established information-retrieval practice.

There is no universal NDCG or recall score that makes a search engine “good.” A
gold standard is a trustworthy test collection and evaluation procedure:

```text
fixed corpus + representative queries + independent relevance judgments
            + standard metrics + locked evaluation split
```

Absolute scores depend on the corpus, query difficulty, judgment density, and
retrieval task. Compare systems only on the same collection and judgments.

## Three Separate Questions

Do not combine these into one “quality” number.

1. **Human retrieval quality:** Does the ranking satisfy the user’s information
   need? Measure against graded relevance judgments.
2. **Encoding or algorithm fidelity:** Does I8 or a future approximate engine
   preserve F32 exact neighbors? Measure result overlap against exact F32.
3. **Systems performance and correctness:** Is retrieval fast, memory-safe,
   persistent, deterministic, filtered correctly, and free of deleted or stale
   results? Measure latency, memory, persistence, and lifecycle invariants.

F32 agreement answers question 2. It is not a substitute for human judgments in
question 1.

## What Moss Does

Reviewed `usemoss/moss` at commit
`dbfbf3638b0dfb2d45f29aa7c4ae18a7c0c7ef4a`.

### Public performance benchmark

Moss’s public `benchmarks/` suite uses 100,000 FAQ-style documents, 15 queries,
three excluded warmup rounds, and 50 measured rounds for 750 top-five searches.
It reports end-to-end latency, including embedding. This is a performance test,
not a relevance test.

Source:
https://github.com/usemoss/moss/blob/dbfbf3638b0dfb2d45f29aa7c4ae18a7c0c7ef4a/benchmarks/README.md

### SDK search-quality benchmark

Moss also has a less-visible SDK quality runner. Its dataset format is:

```text
corpus.jsonl
queries.jsonl
qrels.tsv
```

The runner builds an index, searches every judged query, and reports:

- Hit Rate
- MRR
- NDCG
- average latency
- P95 latency

The checked-in configuration uses top 10 with full SciFact, full NFCorpus, and
a mini MS MARCO dataset. Its recorded MiniLM baseline includes:

| Dataset | Hit Rate | NDCG | MRR |
|:---|---:|---:|---:|
| NFCorpus | 0.690 | 0.313 | 0.504 |
| SciFact | 0.806 | 0.658 | 0.621 |

These scores cannot be compared numerically with VectorKit V2 because the
corpora and qrels differ.

Sources:

- https://github.com/usemoss/moss/blob/dbfbf3638b0dfb2d45f29aa7c4ae18a7c0c7ef4a/sdks/javascript/sdk/test/README.md
- https://github.com/usemoss/moss/blob/dbfbf3638b0dfb2d45f29aa7c4ae18a7c0c7ef4a/sdks/javascript/sdk/test/search.test.ts
- https://github.com/usemoss/moss/blob/dbfbf3638b0dfb2d45f29aa7c4ae18a7c0c7ef4a/sdks/javascript/sdk/test/BASELINE.md

### Moss assessment

Moss follows the core industry pattern by using external datasets, qrels, MRR,
and NDCG. VectorKit V2 already goes further in several areas: direct relevance
recall, vector-only I8/F32 fidelity, filters, deletion, replacement,
persistence reload, multiple candidate pools, and executable quality gates.

The public Moss repository does not currently show, in this suite:

- Precision or recall metrics alongside NDCG
- pooled judgments across all retrieval configurations
- blind multi-assessor labeling
- per-category gates
- confidence intervals or paired significance tests
- explicit pass/fail relevance regression thresholds
- separate encoding-fidelity evaluation

Moss is useful corroboration, but it should not be VectorKit’s sole gold
standard.

## Established Industry and Research Practice

### TREC: the evaluation methodology standard

The TREC/Cranfield model uses a document corpus, topics or queries, and qrels.
Because judging every document is impractical, TREC pools the top results from
multiple diverse retrieval runs and judges the pooled documents. This reduces
the risk that a novel system is penalized because only one baseline’s results
were labeled.

Use TREC-compatible qrels and run files so VectorKit’s metric implementation can
be checked against `trec_eval` or `ir_measures`.

Sources:

- https://trec.nist.gov/howto.html
- https://trec.nist.gov/data/reljudge_eng.html
- https://ir-measur.es/en/latest/getting-started.html

### BEIR: heterogeneous retrieval coverage

BEIR evaluates lexical, sparse, dense, late-interaction, and reranking systems
across diverse datasets. Its official evaluator reports NDCG, MAP, Recall, and
Precision at several cutoffs and can emit TREC run files.

Source: https://github.com/beir-cellar/beir

### MTEB: embedding and retrieval comparison

MTEB uses NDCG@10 as the main score for retrieval tasks and covers many domains,
languages, and annotation styles. It is appropriate for comparing embedding
models, but a product-specific collection is still required for VectorKit’s
actual user behavior.

Sources:

- https://docs.mteb.org/overview/available_tasks/retrieval/
- https://docs.mteb.org/get_started/advanced_usage/two_stage_reranking/

### MS MARCO: first-answer ranking

MS MARCO’s official passage and document ranking leaderboards report MRR@10.
This is valuable when the product mainly needs one good answer, but MRR alone
does not evaluate the rest of a multi-result ranking.

Source: https://microsoft.github.io/msmarco/Submission.html

### ANN-Benchmarks: numerical fidelity and speed

ANN-Benchmarks compares the fraction of exact nearest neighbors recovered with
queries per second, plus build time and index size. This is the right model for
I8/F32 fidelity and any future ANN engine, but not for human relevance.

Source: https://ann-benchmarks.com/

## Active Evaluation Design

The active evaluation path uses checked-in deterministic V1/V2 fixtures and
public collections with existing qrels. SciFact and NFCorpus are the first
real-world collections because both fit the under-50K V1 target and do not
require VectorKit to operate an assessor workflow.

Every collection freezes its corpus, queries, qrels, embedding model,
preprocessing, chunking policy, and evaluation split. Official test splits are
used for final reporting. Train or development splits may be used for
configuration work when the upstream collection provides them; never tune on
the reported test split.

Human-authored VectorKit-specific collection building is deferred until
representative product data and actual assessors are available. There is no
active pooling, blind-review, adjudication, or human-judgment workflow.

### Metrics

Primary product metrics:

- NDCG@5: quality and order of the app-visible ranking
- Recall@5: coverage of judged relevant results
- Success@1: whether the first result is useful

Secondary diagnostics:

- NDCG@10 and Recall@10 for industry comparison
- MRR@10 for first-relevant rank
- Precision@5 for result-list cleanliness
- MAP for broader binary-relevance comparison
- per-category and worst-decile query scores

Fidelity metrics:

- I8 recall@5 and recall@10 versus exact F32
- top-one agreement
- exact-order agreement
- score error or rank displacement when useful

Operational gates:

- zero deleted, superseded, stale, filter, and persistence violations
- deterministic rankings before and after reload
- device P50/P95/P99 latency, RSS, persisted size, and build/load time reported
  separately from relevance

### Regression decisions

Do not select a configuration only because it matches a deeper F32 result list.
Use qrels-based NDCG and recall for ranking decisions. Use F32 overlap only to
approve compression or approximation.

For every candidate change:

1. Compare per-query scores with the current release baseline.
2. Report the mean change and a bootstrap confidence interval.
3. Inspect the largest wins and losses.
4. Reject lifecycle violations regardless of average quality.
5. Require explicit review when one category regresses despite a higher global
   average.

Keep collection-specific gates explicit. Do not promote provisional thresholds
to release policy until repeatable SciFact and NFCorpus baselines exist.

## V3 Execution Plan

Implemented:

1. Emit deterministic TREC-compatible qrels and run files from the Rust quality
   runner.
2. Cross-check per-query and aggregate metrics with `ir_measures`; periodically
   check selected release metrics with official `trec_eval`.
3. Evaluate vector F32/I8, BM25, RRF, and weighted fusion while retaining raw
   scores separately from rank-derived TREC scores.
4. Add BEIR SciFact and NFCorpus adapters with pinned downloads, checksums,
   official test splits, and deterministic MiniLM preprocessing.

Next:

1. Establish repeatable external baselines and collection-specific regression
   gates.
2. Add paired bootstrap confidence intervals and per-category regression gates.
3. Evaluate downstream answer and citation quality separately when VectorKit is
   used inside RAG; do not fold generation quality into retrieval metrics.

Deferred: a VectorKit-specific judged collection until assessors and
representative product data exist.

## Current Position

VectorKit V2 remains the fast deterministic regression fixture. The active next
step is reproducible TREC evaluation on SciFact and NFCorpus using their
upstream qrels. A VectorKit-specific human-reviewed release collection remains
explicitly deferred.
