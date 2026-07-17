# Complete Retrieval Benchmark And Marketing Roadmap

Status: active; Phases 0 and 1 complete, Phase 2 not started

Date: 2026-07-15

## Purpose

VectorKit is not only a vector index. It is a local retrieval SDK that composes
separate, native capabilities over one canonical corpus:

```text
semantic exact-vector retrieval
  + lexical hybrid ranking
  + typed graph traversal and graph-scoped retrieval
  + metadata filtering
  + deterministic persistence and lifecycle guarantees
```

The commercial benchmark must evaluate this complete package without obscuring
the capability-separated architecture:

```text
RetrievalDatabase      = CorpusIndex + RetrievalIndex
GraphDatabase          = CorpusIndex + GraphEngine
GraphRetrievalDatabase = CorpusIndex + GraphEngine + RetrievalIndex
```

Graph traversal and semantic ranking remain separate query capabilities. The
combined product uses an immutable, generation-bound graph selection as a
candidate scope for exact-vector or hybrid ranking. Marketing language may
describe the complete workflow, but must not imply that graph configuration,
embeddings, traversal, and ranking are one inseparable engine.

## Product Positioning

Primary positioning:

> Search what local data says, what it means, and how it connects.

More precise technical positioning:

> Native, on-device semantic, hybrid, and graph-scoped retrieval for Apple
> applications, with no server required.

The benchmark should demonstrate why an application would adopt VectorKit
instead of assembling a vector library, lexical index, custom adjacency
structures, application-side joins, and separately coordinated persistence.

VectorKit must not be advertised as a general graph database. The first graph
release provides deterministic explicit references, reference collections,
document/chunk structure, bounded typed traversal, and graph-scoped retrieval.
It does not provide Cypher, automatic entity extraction, PageRank, broad graph
analytics, or a transactional graph server.

## Questions The Benchmark Must Answer

The evaluation is divided into four independent questions. No single score may
be used as a substitute for all four.

1. **Retrieval quality:** Does semantic or hybrid ranking return relevant
   documents?
2. **Graph retrieval quality:** Does structural scoping recover complete
   supporting evidence and exclude unrelated candidates?
3. **Systems performance:** How much time, memory, and persisted space does the
   complete workflow require on supported devices?
4. **Product value:** Does the integrated SDK reduce application complexity
   while preserving correctness, determinism, privacy, and native ergonomics?

## Canonical Comparison Matrix

Every graph-aware quality collection must run the following ablations with the
same corpus, queries, embeddings, filters, graph schema, and relevance labels:

| Run | Configuration | Purpose |
| --- | --- | --- |
| A | Whole-corpus F32 semantic | Canonical semantic baseline |
| B | Whole-corpus I8 semantic | Compact-vector fidelity |
| C | Whole-corpus weighted hybrid | Best graph-free product baseline |
| D | Graph selection only | Traversal and candidate-projection correctness |
| E | Graph-scoped F32 semantic | Value of structural scope before semantic ranking |
| F | Graph-scoped I8 semantic | Compact combined configuration |
| G | Graph-scoped weighted hybrid | Complete VectorKit package |

RRF may remain a diagnostic configuration. Weighted hybrid is the primary
high-level hybrid comparison because it matches the current product contract.

The flagship comparison is:

```text
whole-corpus weighted hybrid
                versus
graph selection -> graph-scoped weighted hybrid
```

This isolates the value added by structural context while keeping the same
retrieval engine and embeddings.

## Metrics

### Retrieval metrics

Use the existing TREC-compatible evaluation path where the collection supplies
document-level qrels:

- NDCG@5 and NDCG@10
- Recall@5 and Recall@10
- Success@1
- Precision@5
- MRR@10
- AP and MAP
- Judged@5 and Judged@10

Cross-check per-query and aggregate values with `ir_measures`, and periodically
verify release artifacts with official `trec_eval`.

### Graph and multi-document metrics

Add metrics that cannot be inferred from ordinary document relevance alone:

- **Supporting Document Recall@K:** fraction of required supporting documents
  present in the final ranking.
- **Complete Evidence Recall@K:** fraction of queries for which every required
  supporting document is present in the final ranking.
- **Path Accuracy:** fraction of evaluated traversals whose canonical nodes,
  relationships, direction, and path ordering match the expected path.
- **Candidate Recall:** fraction of required supporting documents present in
  the graph-projected candidate scope before ranking.
- **Candidate Reduction Ratio:** total searchable chunks divided by projected
  candidate chunks. Also report the raw corpus and scope counts.
- **Scoped NDCG@10:** final ranking quality after applying the graph scope.
- **Empty-Scope Rate:** fraction of queries for which graph selection resolves
  to no searchable chunks.
- **Truncation Rate:** fraction of graph queries affected by configured graph
  limits, reported by truncation reason.

Complete Evidence Recall is the primary graph-quality metric. A multi-document
retrieval can fail even when ordinary Success@K is positive if one required
piece of evidence is missing.

### Latency and resource metrics

Measure these stages separately and as a complete operation:

- embedding latency, when an end-to-end model benchmark is intentionally run
- graph seed resolution
- bounded traversal
- candidate projection
- scoped vector or hybrid ranking
- hydration
- total graph-to-retrieval latency
- build, save, load, and read-only validation
- peak resident memory
- graph, vector, lexical, corpus, and complete persisted bytes

Report warm P50, P95, and P99 latency plus cold-start latency. Never mix
embedding, index construction, persistence, or hydration into retrieval timing
without labeling the result as end-to-end.

## Public Collections

### Canonical graph-quality collection

The first public graph-quality adapter should evaluate either HotpotQA or
2WikiMultiHopQA. Prefer the collection that can be adapted without gold-label
leakage and with the clearest license and deterministic source artifacts.

- HotpotQA provides natural multi-hop questions and human-created supporting
  facts. It is distributed under CC BY-SA 4.0.
  Source: https://hotpotqa.github.io/
- 2WikiMultiHopQA combines structured and unstructured sources and provides
  evidence describing reasoning paths.
  Source: https://arxiv.org/abs/2011.01060

The V1 product supports fewer than 50K chunks, so a public adapter must create a
fixed, globally shared collection within that envelope. It must not construct a
different corpus for each query or use the small per-query distractor context
as though it were a full retrieval collection.

The construction protocol must:

1. Pin the upstream release, split, archive URL, checksum, and license.
2. Select the document universe deterministically without consulting relevance
   grades or supporting-fact labels.
3. Freeze the corpus before determining which upstream questions are
   evaluable against it.
4. Retain only questions whose complete gold evidence exists in the frozen
   corpus, and disclose this derived-query selection.
5. Derive graph nodes and edges from public document structure, hyperlinks, or
   structured upstream relationships independently of qrels.
6. Use relevance and supporting-fact labels only during evaluation.
7. Preserve upstream identifiers and emit a manifest describing every
   transformation.
8. Publish the construction and evaluation scripts without committing or
   redistributing raw data unless the upstream license explicitly permits it.

Gold supporting documents or paths must never be used to create graph edges,
choose traversal hops, generate seeds, tune per-query limits, or filter the
candidate set.

### Seed contract

VectorKit does not currently perform automatic entity extraction. The
benchmark must not silently add that capability.

Each graph-aware query must therefore use one of these disclosed seed sources:

- an explicit structured seed that represents an application-provided
  relationship constraint; or
- a deterministic, frozen exact-alias resolver that extracts an unambiguous
  seed directly from the query text without consulting qrels.

Report explicit-seed and derived-seed results separately. Queries for which the
frozen resolver cannot produce an unambiguous seed must fall back through a
documented policy or be excluded before configuration tuning. Do not use a
general LLM or gold supporting title to create benchmark seeds.

### Traversal stress collection

LDBC Social Network Benchmark data may be used for a secondary traversal and
scale fixture. It is not the primary quality or marketing benchmark because
LDBC targets general graph database workloads outside VectorKit's supported
surface.

Source: https://ldbcouncil.org/benchmarks/snb/

## Commercial Device Workload

The flagship device benchmark should include fixed 10K, 25K, and 50K chunk
presets with 384-dimensional embeddings. Add 768-dimensional presets only where
they fit the documented memory and persistence envelope.

Each preset should contain:

- explicit references and reference collections
- one-hop, two-hop, and three-hop traversals
- relationship-plus-text queries
- graph scope intersected with metadata filters
- semantic and exact-name queries
- irrelevant but semantically similar distractors
- repeated references, cycles, missing optional references, and deleted records
- F32 and I8 retrieval configurations
- save, validate, reload, and ranking-stability checks

An example product query is:

```text
Find documents discussing battery problems,
linked to products owned by the mobile team,
created after January.
```

This represents three separate inputs composed by the database:

```text
graph constraint: mobile team -> owns -> product
metadata constraint: created after January
retrieval query: "battery problems"
```

Run release builds offline on:

- one current supported iPhone used for the headline number
- one older supported iPhone used to establish a conservative envelope
- one pinned Apple Silicon Mac for repeatable development comparisons

Record the exact hardware, OS, toolchain, VectorKit commit or release, thermal
conditions, warmups, sample count, and percentile calculation.

## External Baselines

Use two comparison lanes.

### Engine-isolation lane

Give every engine the same precomputed vectors, corpus, queries, top K, and
filters:

- scalar exact scan
- Accelerate/vDSP exact scan where applicable
- an embedded brute-force vector engine such as sqlite-vec where the target
  integration is supported
- an embedded ANN engine such as USearch or ObjectBox only at a disclosed
  recall target against exact F32

Never compare unconstrained ANN latency with VectorKit exact latency. Match
quality first, such as Recall@10 greater than or equal to 0.99, and disclose
build time, index size, and memory.

### Complete-application lane

Compare the complete VectorKit workflow with a representative application-side
stack:

```text
embedded vector engine
  + custom adjacency storage and traversal
  + lexical search or custom fusion
  + application-side filtering and joins
  + independently coordinated persistence
```

Measure:

- final retrieval quality and complete evidence recall
- total query latency and memory
- persisted size and number of stores
- update, deletion, and reload consistency
- lines of integration code and public API operations required
- failure behavior when component generations become inconsistent

Code size is supporting developer-experience evidence, not a quality or speed
metric. The reference implementation must be competent and published so the
comparison is reproducible.

Cloud services are not primary performance baselines because network latency,
server resources, privacy, and deployment topology make the comparison
materially different. They may appear only in a feature and architecture
comparison with those differences clearly labeled.

## Current Evidence And Missing Evidence

Already implemented:

- deterministic exact-vector, BM25, weighted hybrid, filters, and persistence
- deterministic typed graph traversal and candidate projection
- graph-scoped exact and hybrid retrieval
- generation-bound stale-selection rejection
- graph-only, retrieval-only, and combined databases in Rust, Swift, and Python
- cross-wrapper graph conformance fixture
- TREC-compatible retrieval artifacts and independent metric cross-checking
- canonical SciFact and NFCorpus retrieval-quality runs
- synthetic graph traversal, projection, persistence, and graph-free regression
  benchmarks

Current synthetic development evidence includes a 2K-node/8K-edge fixture on an
Apple M1 Max with three-hop traversal at 18 microseconds P95 and candidate
projection at 2 microseconds P95. These are engineering measurements, not yet
public product claims: the fixture is synthetic, small, and not the pinned
target iPhone environment.

Still required before graph marketing claims:

- one frozen real graph-quality collection under the V1 capacity envelope
- independent graph construction and seed-generation rules
- supporting-evidence and graph-specific metrics
- full comparison matrix across graph-free and graph-scoped retrieval
- target-device graph benchmarks at 10K, 25K, and 50K
- a published competent multi-library reference stack
- byte-reproducible artifacts and a public methodology report
- a claim register tying every statement to exact evidence and qualifiers

## Implementation Roadmap

### Phase 0: Freeze The Benchmark Contract

Status: complete on 2026-07-16.

Approved contract:

- `docs/product/graph-retrieval-evaluation-contract-v3.md`
- Status: approved; iPhone 14 Pro Max with iOS 26 or later is the conservative
  device. After two failed clarification rounds, the third focused revision
  passed two fresh isolated implementation-author reviews. Both reproduced the
  normative A-J fixture, all population hashes, exact artifact/hash schemas,
  and both worked examples without a blocker.

Deliverables:

- approve this document as the implementation source of truth
- define a versioned graph-evaluation collection schema
- define deterministic run identifiers and artifact filenames
- define complete-evidence, candidate, path, and truncation metric semantics
- define the explicit and derived graph-seed contracts
- choose the headline iPhone and conservative older-device target

Exit gate:

- two independent implementations can calculate the same metrics and identify
  the same valid queries from the written contract

### Phase 1: Add Graph-Aware Evaluation Artifacts

Status: complete on 2026-07-17. Phase 1.1 conformance, Phase 1.2a
production-backed whole-corpus A-C retrieval, Phase 1.2b production graph
selection D, and Phase 1.2c production graph-scoped E-G retrieval are complete.
The qualification-only A-G artifact remains deterministic and independently
cross-checked. A separate clean release-context pipeline now verifies pinned
`ir_measures` and official NIST `trec_eval`, validates the closed V3 schemas,
and atomically publishes the exact 44-file public layout. Two fresh emissions
are byte-identical. Phase 2 remains inactive.

The Phase 1.2c completeness review is also closed. Classified execution
failures now serialize canonical query-local or run-wide `invalid_execution`
outcomes and rebuild all affected downstream artifacts without aborting the
qualification. The CLI reports 1.2a, 1.2b, and 1.2c separately. Artifact
finalization enforces the exact 56-file preimage and rechecks a stored index;
the valid frozen bytes and artifact-set SHA-256 are unchanged. Those 56 files
remain qualification-only and are never mixed into the public result root.

First implementation task (complete): add the checked-in V3 conformance fixture
and its schema, population-hash, canonical-serialization, and byte-rerun
validators in evaluation-only tooling.

Second implementation slice (complete): execute all three frozen D seed lanes
through `GraphDatabase`, project/filter stable candidate identities, prove graph
persistence equivalence and stale-selection rejection, emit deterministic
partial artifacts, and obtain exact agreement from an independent Python graph
oracle. See
`docs/product/reports/graph-retrieval-phase-1-graph-selection-report.md`.

Third implementation slice (complete): execute nine E-G runs through their own
production combined databases, prove D-equivalent selection/path logic,
calculate retrieval/evidence/graph metrics and all nine A-E/B-F/C-G
comparisons, prove save/validate/load and stale-selection behavior, reproduce
the results with an independent Python oracle and pinned `ir_measures`, and
emit two byte-identical 56-file canonical qualification sets. See
`docs/product/reports/graph-retrieval-phase-1-graph-scoped-retrieval-report.md`.

Publication slice (complete): pin and checksum official NIST `trec_eval`,
derive release-context identities and run IDs from a clean executable and Git
revision, assemble the exact closed public layout, and validate it independently
including section 4.7 logical-run portability. See
`docs/product/reports/graph-retrieval-phase-1-publication-report.md`.

Extend evaluation-only tooling; do not add benchmark concerns to production
Rust APIs or wrappers.

Deliverables:

- V3 evaluation schema separating corpus, queries, graph inputs, embeddings,
  document qrels, supporting evidence, and expected paths
- deterministic graph-selection and scoped-retrieval run files
- diagnostic JSON retaining native scores, paths, projection counts, limits,
  filters, and timing stages
- complete-evidence, candidate-recall, path-accuracy, reduction, empty-scope,
  and truncation evaluators
- exact independent checks for graph paths and candidate scopes
- persistence-reload equivalence checks

Exit gate:

- repeated executions produce byte-identical evaluation artifacts and identical
  rankings before and after reload

Exit result: PASS. The independent validator confirms the exact 44-file
inventory, 43 manifest entries, valid A-G executions, byte-identical reruns,
official metric agreement, persistence equivalence, and cross-context logical
run mapping. This completes evaluation-artifact Phase 1 only; it is not a
public quality, performance, device, or marketing claim.

### Phase 2: Build The First Public Graph Collection Adapter

Deliverables:

- documented selection between HotpotQA and 2WikiMultiHopQA
- downloader with checksum, license notice, expected-count checks, and schema
  validation
- deterministic under-50K corpus construction
- graph construction independent of qrels
- frozen exact-alias resolver or explicit structured seeds
- canonical MiniLM embeddings matching the existing quality baseline
- official development split for configuration and locked test split for final
  reporting where upstream splits allow it

Exit gate:

- a clean machine can reproduce corpus, graph, embeddings, queries, qrels, and
  evidence artifacts from pinned upstream sources

### Phase 3: Run The Quality Ablation

Deliverables:

- runs A through G from the canonical comparison matrix
- per-query and aggregate reports
- category slices for hop count, seed type, query type, filter selectivity, and
  candidate-scope size
- error analysis for missing evidence, empty scopes, incorrect paths, and
  ranking failures
- configuration selection performed on development data only

Exit gate:

- the locked test run demonstrates the measured benefit or cost of graph
  scoping without per-query tuning or qrels leakage

If graph scoping does not improve the intended queries, publish the negative
result internally and improve the dataset adapter, schema, or supported product
workflow. Do not manufacture a marketing claim from an unfavorable result.

### Phase 4: Add Target-Device Graph Benchmarks

Deliverables:

- deterministic 10K, 25K, and 50K device fixtures
- staged and end-to-end P50/P95/P99 latency
- cold open, save, validation, memory, and component-size measurements
- F32/I8 and graph-free/graph-enabled comparisons
- headline and older-device reports using release builds
- proof that installing graph support does not route graph-free queries through
  graph-aware dispatch or materially regress the graph-free hot path

Exit gate:

- every proposed device claim is reproduced across at least three final runs
  under the documented protocol and fits the product's memory and correctness
  budgets

### Phase 5: Build External Reference Implementations

Deliverables:

- exact engine-isolation harness
- recall-constrained embedded ANN harness where supported
- competent vector-plus-custom-graph application baseline
- pinned dependency versions and public build/run instructions
- feature-parity matrix documenting unsupported operations instead of silently
  omitting them

Exit gate:

- an external developer can reproduce the comparison and inspect all
  configurations, source code, raw measurements, and failures

### Phase 6: Publish The Benchmark Report And Claim Register

Deliverables:

- methodology page and machine-readable manifest
- raw qrels, runs, graph traces, timing samples, metrics, and checksums where
  licensing permits
- one report for public quality and one for real-device systems performance
- claim register containing claim text, evidence, qualifiers, expiration or
  rerun condition, and prohibited broader interpretation

Exit gate:

- every public number maps to a reproducible artifact, device, dataset, metric,
  version, and date

### Phase 7: Add Regression Gates

Deliverables:

- small checked-in graph-quality smoke fixture for ordinary CI
- scheduled or release-only full public collection run
- target-device release qualification
- thresholds for correctness, relevance regression, candidate recall,
  graph-free performance regression, memory, and persisted size

Raw public datasets remain downloaded evaluation inputs and are not committed
to the repository.

Exit gate:

- a release cannot silently regress graph correctness, scoped retrieval
  quality, persistence equivalence, or the documented graph-free performance
  envelope

## Claim Policy

Good claim forms:

> Graph-scoped hybrid retrieval improved Complete Evidence Recall@10 by X% over
> whole-corpus vector search on the VectorKit-50K [collection] benchmark.

> VectorKit reduced the ranked candidate set by X times while preserving Y%
> Supporting Document Recall@10.

> VectorKit completed bounded graph selection and hybrid ranking across 50K
> local chunks in X milliseconds P95 on [device and OS].

> One native SDK provides semantic, hybrid, graph-scoped retrieval, filtering,
> and generation-consistent persistence without a server.

Every numerical claim must name or link to:

- dataset and split
- corpus and chunk counts
- embedding model and dimensions
- graph construction and seed policy
- comparison configuration
- metric and cutoff
- hardware and OS for performance claims
- VectorKit version or commit
- report date

Do not publish:

- "best," "fastest," or "more accurate" without a bounded, reproducible
  comparison
- vector model gains as retrieval-engine gains
- ANN speed comparisons without matched recall
- synthetic traversal timings as real-world retrieval-quality evidence
- per-query distractor results as full-corpus search results
- a graph database claim broader than the supported bounded retrieval surface
- a complete-package claim when only one component was measured

## Desired Marketing Hierarchy

1. **Hero:** private semantic, hybrid, and graph-scoped retrieval on-device.
2. **Quality:** complete evidence retrieval on a public multi-hop collection.
3. **Performance:** P95 complete-workflow latency across 50K local chunks.
4. **Efficiency:** memory, component sizes, persistence, and I8 fidelity.
5. **Correctness:** deterministic paths, filters, lifecycle, and reload parity.
6. **Developer experience:** one native package and one consistent corpus rather
   than several independently coordinated local systems.

The benchmark is successful only when it proves the value of the complete
package while making each capability's individual contribution visible.
