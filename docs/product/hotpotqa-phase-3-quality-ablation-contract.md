# HotpotQA Phase 3 Quality Ablation Contract

Status: frozen Phase 3a selection protocol

This contract pre-registers the HotpotQA development search and reporting
configuration before any Phase 3 retrieval result is generated. It extends the
Graph Retrieval Evaluation Contract V3 without changing the frozen Phase 2
adapter, production APIs, metric semantics, or publication rules. Phase 3a ends
after development-only selection and the complete development A–G matrix.
Locked reporting is a separate, one-shot Phase 3b task.

## 1. Sealed inputs and phase boundary

The only tuning collection is
`hotpotqa-linked-abstracts-graph-v1-development`, version `1`, with 12,670
records/chunks, 603 retrieval queries, 1,206 qrels, and 603 evidence rows. Its
global development population SHA-256 is
`1d972dd63fdef4e29f46f54e1a643f3663189379d1d679b8e265539d8c112a0f`.
The derived graph execution population contains 599 queries and has SHA-256
`da343545fa764b44c5382f4a16c933dded7bd613ae6e12768b5c2772c6739582`.
The four frozen derived-seed ambiguity exclusions remain
`excluded_pre_freeze`; there is no explicit-seed lane and no fallback.

The generated sibling root named `test` is sealed throughout Phase 3a. Phase
3a tooling must reject a collection whose exact `collection.json.split` is not
`development`, reject any path whose final component is `test`, and avoid
opening test queries, qrels, evidence, expected paths, exclusions, embeddings,
rankings, or metrics. It may compare only the already-frozen adapter/root
identity or checksum recorded outside the test collection. No test ranking,
selection, metric, slice, or error artifact may be created.

## 2. Configuration invariants

The following inputs and behavior are frozen and are not tunable:

- corpus, collection, query, qrel, evidence, exclusion, and embedding bytes;
- MiniLM model identity and preprocessing;
- cosine similarity with unit-L2 normalization;
- symmetric per-vector I8 quantization from the frozen F32 embeddings;
- the frozen BM25 tokenizer, scoring, and matched-term policy;
- graph schema and exact-alias-derived seed policy;
- outgoing traversal with a two-hop bound and the frozen graph resource limits;
- top-K 10, evaluation depth 100, and relevance threshold 1;
- null metadata filters and their unchanged evaluation semantics;
- stable chunk/document projection, document deduplication, exhaustive native
  result handling, rank-derived TREC scores, and every V3 metric definition;
- collection ownership boundaries and production ranking/traversal behavior.

Only three globally shared weighted-hybrid values may be selected:
`fusion_alpha`, vector candidate limit, and keyword candidate limit. Runs C and
G must use the exact same selected values. No per-query, per-category, or
graph-scope-specific value is permitted. RRF may be emitted only as a clearly
separate diagnostic and cannot become C or G.

## 3. Closed development search space

The canonical machine-readable search space is
`benchmarks/retrieval-quality/hotpotqa/phase-3-development-search-space.json`.
Its exact bytes are hashed with SHA-256 before execution. Candidate preimages
are the listed canonical objects; the cross product is closed before results.

- `fusion_alpha`: 0.2, 0.4, 0.6, 0.8.
- vector candidate limit: 25, 50, 100.
- keyword candidate limit: 25, 50, 100.

This 36-candidate grid brackets lexical-heavy through semantic-heavy weighted
fusion without using the component-only endpoints. It includes the product
default alpha `0.6` and candidate pair `50/50`. Limits 25, 50, and 100 include
the smaller, default, and debug/high-candidate configurations already used by
the product and its quality work. Every limit is positive and far below the
12,670 searchable chunks. The grid will not be expanded after results.

## 4. Selection objective

Only the complete 603-query development Run C result selects the one shared
C/G configuration. Run G, C-versus-G deltas, any graph-scoped metric, query or
category slice, individual query, and all test data/results are excluded from
selection.

Candidates are ordered lexicographically by:

1. higher Complete Evidence Recall@10;
2. higher NDCG@10;
3. higher MAP;
4. higher Recall@10;
5. higher MRR@10;
6. smaller vector-plus-keyword candidate count;
7. smaller maximum component candidate count; and
8. ascending canonical configuration bytes.

This order gives the pre-declared multi-document evidence objective priority,
then standard ranking quality and recall, and finally chooses the smaller
equally effective candidate budget deterministically. It is fair to the graph
comparison because it observes only the whole-corpus product baseline and
cannot optimize graph gain, graph loss, or graph scope.

The selected candidate, the complete comparison trace against every tied
predecessor, and the search-space hash are emitted mechanically. A provisional
selection stays outside the canonical A–G result root until the checked-in lock
is frozen. Replaying selection from the candidate artifacts must reproduce
byte-identical lock bytes.

## 5. Pre-registered slices

All final development A–G results report these slices without using them for
configuration selection:

- upstream question type;
- upstream difficulty level;
- exact `type:level` category;
- derived seed status;
- candidate-scope size: `empty`, `1-10`, `11-100`, `101-1000`,
  `1001-10000`, or `above-10000`;
- graph truncation reason, including the no-truncation value;
- evidence-document count;
- graph candidate recall bucket;
- filter selectivity as `not_applicable`, because V1 has no filters; and
- path accuracy as `not_applicable`, because no expected paths exist.

Candidate-recall buckets are exact: `0`, `(0,0.5)`, `[0.5,1)`, and `1`.
Slices preserve the run's frozen declared/execution population and metric
statuses; they never drop invalid or excluded rows.

## 6. Pre-registered error categories

Each declared query is assigned deterministically to all applicable categories
from this closed list:

- seed ambiguity exclusion;
- empty graph scope;
- truncated graph selection;
- supporting evidence absent from scope;
- evidence in scope but missing from top 10;
- whole-corpus ranking failure;
- graph-scoped ranking failure;
- I8 ranking divergence;
- hybrid ranking regression;
- duplicate-collapse effect; and
- unexpected execution failure.

Definitions use only frozen inputs and emitted results. Ranking failure means
Complete Evidence Recall@10 is zero for the named lane. I8 divergence compares
A/B or E/F on the identical population. Hybrid regression compares B/C or F/G
on the identical population. Duplicate-collapse effect applies when at least
one native chunk is collapsed before document evaluation. Unexpected execution
failure is any canonical `invalid_execution`; it invalidates Phase 3a under the
V3 attribution rules rather than becoming an exclusion.

## 7. Canonical development matrix

After the configuration lock is committed, Phase 3a executes exactly seven
derived-lane development runs: A–C over all 603 queries and D–G with 603
declared, 599 executed, and four frozen exclusions. C and G use the same lock.
No explicit-seed run exists. D/E/F/G selections and paths for each valid query
must be logically identical. B versus A and F versus E change only encoding;
G versus C changes only graph scope. Any invalid run fails Phase 3a.

Generated tuning and A–G artifacts remain ignored beneath `target/` and are
kept in distinct roots. Phase 3a creates no quality, device-performance, or
marketing claim. Its exact successor is one clean, one-shot Phase 3b execution
against the sealed test split using the immutable selected-configuration file.
