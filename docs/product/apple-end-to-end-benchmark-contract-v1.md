# Apple End-to-End Text Retrieval Benchmark Contract V1

Status: draft approved for implementation planning

Date: 2026-08-13

This document defines the first directly sampled Apple text-to-result benchmark
for RetrievalKit. It measures the production Swift query-embedding and search
boundaries together on a pinned Apple Silicon Mac and a physical iPhone. The
model and persisted database are ready before timing begins.

The compact normative descriptors are:

- `benchmarks/apple-end-to-end/workloads-v1.json`
- `benchmarks/apple-end-to-end/protocol-v1.json`

If prose and a descriptor disagree, the stricter safety or claim restriction
wins and the contract must be corrected before execution. Changing a workload,
model identity, query population, measurement boundary, sample policy, device
rule, or claim classification requires a new contract version and new workload
IDs.

## 1. Product question

The benchmark answers one question:

> When a user searches from a ready local application, how long does it take to
> embed the query text, search RetrievalKit, and return decoded top-10 vector or
> hybrid results on Mac and iPhone?

The benchmark does not use separately sampled percentile sums as end-to-end
evidence. Every end-to-end duration surrounds one complete operation.

## 2. Scope and workload classification

The frozen corpus sizes are exactly 10,000, 50,000, and 100,000 active chunks,
all at 384 dimensions with cosine search, I8 scalar-quantized database storage,
and top K 10.

| Workload | Classification | Interpretation |
| --- | --- | --- |
| `apple-e2e-10k-384d-i8-v1` | `supported_product` | Normal V1 product evidence. |
| `apple-e2e-50k-384d-i8-boundary-v1` | `qualification_boundary` | Exact boundary/headroom evidence; it does not change the documented fewer-than-50K support envelope. |
| `apple-e2e-100k-384d-i8-stress-v1` | `stress` | Diagnostic scaling evidence only; never a support, release, product-gate, or marketing claim. |

The 100K lane does not authorize ANN or HNSW, does not change RetrievalKit's
supported capacity, and cannot cause a supported-product gate to pass or fail.
Every 100K manifest and result must state `marketing_eligible: false` and
`supported_v1_capacity_changed: false`.

## 3. Systems under comparison

Both lanes use direct Core ML, the same fixed 256-token WordPiece/pooling/output
contract, Core ML compute units `.all`, public Swift embedding and retrieval
APIs, and an I8 RetrievalKit database. Both return exactly 384 finite,
unit-normalized F32 values before RetrievalKit performs query validation and I8
query quantization internally.

### 3.1 Control

`coreml-fp32-production-v1` is the production Swift profile:

- model: `sentence-transformers/all-MiniLM-L6-v2`
- source revision: `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
- production artifact repository commit:
  `405818d6afef1aaf2fc8da67da6caf20b55f0a28`
- archive: `all-MiniLM-L6-v2-coreml-fp32-v1.tar`
- archive bytes: `90,664,960`
- archive SHA-256:
  `e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f`
- fixed sequence length: 256
- Core ML compute units: `.all`

This is the only production-qualified embedding profile in this contract.

### 3.2 Candidate

`coreml-weight-only-q8-experimental-v1` is an experimental performance lane:

- model: `sentence-transformers/all-MiniLM-L6-v2`
- source revision: `c9745ed1d9f207416be6d2e6f8de32d1f16199bf`
- artifact repository commit:
  `617ce926c1f9e0289365d3e999474cc28b1645d4`
- artifact manifest SHA-256:
  `b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`
- artifact: `coreml/all-MiniLM-L6-v2-q8.mlpackage`
- downloaded canonical-tree bytes: `22,724,832`
- canonical-tree SHA-256:
  `72c82477ad518acdf88f95727f1af695702a9e3da7ae48799902bac3adc55281`
- quantization: weight-only signed INT8 with FP16 compute and F32 output
- fixed sequence length: 256
- Core ML compute units: `.all`

This lane does not restore the retired Swift ONNX package, add ONNX Runtime to
an Apple product, or qualify Q8 for production. Its index and query embeddings
must both come from this exact Q8 profile. Cross-profile index/query results are
diagnostic only and are excluded from latency claims.

The manifest at the pinned artifact commit is internally inconsistent with its
own Core ML package. It reports `22,724,760` bytes and canonical-tree SHA-256
`f9f78284766a1dd8352d85e7663fb366a938304c76204828badd7e52c2f05292`,
while a clean HTTPS download of the three package files contains `22,724,832`
bytes and hashes to the value frozen above. The 72-byte difference is in the
package `Manifest.json`. Benchmark acquisition must verify the actual pinned
tree and retain both values in its evidence. This inconsistency independently
prevents the candidate from becoming production-qualified under this contract.

## 4. Frozen corpus and query inputs

The benchmark uses a dedicated graph-free corpus family. It must not reuse the
closed Phase 4 workload IDs or artifact roots.

Each corpus source contains deterministic UTF-8 chunk text, stable record and
chunk identities, BM25-visible terms, realistic hydration payloads, and fixed
metadata. It contains four chunks per record and no graph state. Source corpus
bytes are model-independent. Each embedding profile produces a separate set of
document embeddings and therefore a separate persisted I8 database from the
same source corpus.

Before device execution, unmeasured corpus preparation must:

1. generate each source corpus twice into distinct empty directories;
2. prove byte-identical source corpus and query files;
3. embed every active chunk with the selected profile;
4. build, save, validate, unload, reload, and replay the database on Mac before
   the timed query benchmark;
5. record source, model, database, and component hashes in a closed manifest;
6. prove the database contains I8 vectors plus one F32 scale per vector and no
   duplicate F32 vector payload; and
7. keep generated corpora, embeddings, databases, and raw results under ignored
   `target/` roots.

The query suite contains 100 distinct frozen texts. After tokenization, its
length distribution is:

| Token count | Query count |
| --- | ---: |
| 1-16 | 20 |
| 17-32 | 35 |
| 33-64 | 25 |
| 65-128 | 15 |
| 129-256 | 5 |

The semantic population contains 40 paraphrase queries, 30 exact-name or
identifier queries, 20 mixed semantic-plus-keyword queries, and 10 difficult
near-distractor or no-natural-match queries. The same source queries and
deterministic schedule are used for every device, workload, profile, and search
mode. Query truncation beyond 256 tokens is forbidden in the frozen suite.

## 5. Search matrix

Every eligible workload/profile/device pair runs two independent scenarios:

1. exact vector search;
2. weighted hybrid search with `alpha = 0.6`, 50 vector candidates, 50 BM25
   candidates, and top K 10.

Each scenario starts in a fresh process. Vector and hybrid samples are not
interleaved in one process. No filter, graph selection, generation model,
network request, filesystem write, index mutation, or corpus embedding may
occur inside a measured query.

The reference devices are:

- Apple M1 Max MacBook Pro in native arm64 release mode; and
- physical iPhone 17 Pro Max in arm64 release mode with no debugger attached.

Every artifact records the exact hardware identifier, OS version/build,
toolchain, RetrievalKit revision, embedding profile, compiled-model cache key,
database manifest identity, process ID, selected SIMD backend, AArch64 dot
product availability, and whether a debugger was attached.

## 6. Normative query boundary

The headline `end_to_end_text_search` sample begins immediately before the
public embed call and ends only after the public RetrievalKit call has returned
decoded Swift result values:

```text
query text
  -> public EmbeddingKit embed
       -> tokenization
       -> Core ML inference
       -> pooling/normalization contract validation
  -> public RetrievalKit vector or hybrid search
       -> embedding validation
       -> I8 query quantization
       -> exact scoring and optional BM25 fusion
       -> top-K result decoding and hydration
  -> decoded Swift results
```

Each measured operation records three nested monotonic-clock durations around
the same query:

1. `embedding_total`;
2. `retrieval_total`; and
3. `end_to_end_text_search`.

The total must be directly measured. It must not be produced by adding stage
durations or stage percentiles. Stage instrumentation must not change the
public model or retrieval behavior.

## 7. Query measurement protocol

For each device/workload/profile/search-mode configuration:

1. start a fresh release process;
2. require the iPhone app to be active in the foreground before setup or timing;
3. load the already verified local model with no network fallback before timing;
4. load the already prepared persisted database read-only before timing;
5. execute 50 complete-operation warmups and discard them; and
6. execute exactly 750 measured mixed-query operations.

The 750-query schedule is a deterministic permutation/cycle of the frozen 100
queries. It must not consist of one repeated query. There are at least three
thermally valid final sessions per configuration. Samples from different
sessions are never pooled to hide variance. Comparison uses the median of the
three session P95 values.

Raw durations are integer nanoseconds from a monotonic clock. For every sorted
distribution of `n` samples and percentile `p`, use nearest rank:

```text
index = max(1, ceil(p * n)) - 1
percentile = sorted_samples[index]
```

Report count, minimum, maximum, arithmetic mean, P50, P95, and P99 for every
stage and the directly measured total. Retain every raw sample with query ID,
session ID, process ID, scenario identity, start/end clock values, duration,
result count, top result identity, and a deterministic digest of all returned
identities.

## 8. iPhone execution validity

All iPhone assets must already be present and verified. Interactive execution
is offline. A valid session requires:

- a physical device, never a simulator;
- release configuration with no debugger attached;
- application active in the foreground;
- network disabled after asset preparation;
- Low Power Mode disabled;
- battery between 50% and 90%, not charging;
- nominal thermal state at session start; and
- nominal or fair thermal state at session end.

A serious or critical thermal state, background transition, debugger
attachment, network access, low-memory warning or termination, result mismatch,
or sample count mismatch invalidates the session. After a thermal invalidation,
the collector must stop and require a cooldown before any supported workload is
retried.

## 9. 100K stress safety policy

Mac must build, validate, reload, and successfully query the 100K database
before it is copied to the iPhone. Those setup operations are outside the timed
benchmark.

The iPhone 100K lane is query-only over a prebuilt I8 database. On iPhone it is
strictly forbidden to:

- embed the 100K corpus;
- build, save, compact, or mutate the 100K database;
- run a separate sustained or throughput loop; or
- reuse, resume, or relabel the closed `100k-384d-v3-stress` Phase 4 workload.

The iPhone may time 100K only when the prebuilt database loads successfully
outside the measured interval, the model and database remain resident without a
memory warning, free storage is at least three times the database size plus
1 GiB, and thermal state remains nominal before measurement. Otherwise the only
valid outcome is `not_run_memory_safety`. The collector must abort on the first
memory warning or serious/critical thermal transition. An aborted 100K run is
terminal for this contract version and cannot be retried without an
owner-approved amendment.

This new query-only workload does not reopen the prior graph-enabled device
qualification or reinterpret its permanent cancellation.

## 10. Correctness and Q8 quality prerequisite

Latency samples are invalid unless every result satisfies the public API
contract, contains no deleted or unknown identity, and is deterministic for the
same profile/query/database state. Before performance execution, save/load
replay must produce identical result identities and ordering within each
profile.

The Q8 lane is eligible for performance comparison only after the frozen
provider conformance fixture passes against the FP32 control with:

- median cosine at least `0.995`;
- mean Top-10 set overlap at least `0.95`;
- minimum per-query Top-10 overlap at least `0.80`; and
- exactly 384 finite, unit-normalized F32 output values for every query.

Exact Top-10 set rate must be reported but is not a gate in this performance
contract. Passing these prerequisites does not qualify Q8 for production. BEIR
and production-derived quality evidence remain separate release decisions.

## 11. Baseline and regression policy

V1 is observational. It freezes methodology and produces the first validated
baseline; it does not invent an absolute end-to-end latency threshold before
measurement.

After the first complete result is independently validated, a separate owner
decision may freeze per-device, per-workload, per-profile budgets. Future
regression gates compare like-for-like medians of session P95 values. They may
not compare different hardware, OS builds, model identities, workloads, query
populations, or search modes as though they were the same baseline.

The report must show absolute latency and the Q8-versus-FP32 delta for
`embedding_total`, `retrieval_total`, and directly measured
`end_to_end_text_search` P50/P95/P99. Model and database sizes may be reported as
context but are not benchmark outcomes.

## 12. Harness isolation and artifact validation

Implementation uses a new graph-free Apple end-to-end benchmark family. It may
share source between its macOS executable and iOS app, but both products call
the same public `EmbeddingKit` and `RetrievalKit` APIs and link no graph or ONNX
runtime code. It must not execute the closed `RetrievalKitIOSBench` Phase 4
device workloads or write into their artifact roots.

Every run writes to a fresh ignored output root and publishes atomically only
after validation. A standalone validator that does not import benchmark runtime
code fails closed on:

- unknown schema fields or workload/profile identities;
- unpinned model, corpus, query, database, binary, or framework identity;
- incorrect workload classification or claim eligibility;
- wrong sample counts, percentile method, session count, or pooled sessions;
- derived rather than direct end-to-end totals;
- missing raw samples or mismatched result digests;
- network use during measurement, simulator use, debugger attachment,
  foreground failure, memory warning, or invalid power/thermal state;
- Q8 conformance failure;
- unsafe or mislabeled iPhone 100K execution; and
- any 100K support, release-gate, product, or marketing claim.

Validator tests require one positive fixture and negative mutations for every
rule family. Generated corpora, model copies, compiled models, databases, raw
samples, and device logs remain ignored and are never committed.

## 13. Explicitly out of scope

- changing the production FP32 Swift embedder;
- shipping or advertising Q8;
- changing RetrievalKit's I8 quantizer or public APIs;
- graph-only or graph-scoped retrieval;
- metadata-filter performance;
- model acquisition, compilation, initialization, and loading;
- database loading;
- corpus chunking, corpus embedding, index construction, save, or compaction;
- startup-to-first-result, memory profiling, sustained throughput, and energy;
- browser, Android, Python, Node, Kotlin, or server comparisons;
- answer generation or an LLM/SLM pipeline;
- ANN/HNSW;
- network-download performance;
- reopening frozen Phase 4 or sealed retrieval-quality artifacts; and
- treating 50K or 100K as a change to the fewer-than-50K V1 support boundary.

## 14. Completion criteria

The contract implementation is complete only when:

1. the corpus/query generator, model preparation, Mac harness, separate iOS
   harness, collector, and independent validator are versioned and tested;
2. two fresh generations of every source corpus and query suite are
   byte-identical;
3. both profile-specific I8 databases pass Mac save/load/replay validation for
   10K, 50K, and 100K;
4. all eligible Mac and iPhone query sessions validate with raw samples and
   direct totals;
5. the iPhone 100K row is either valid query-only stress evidence or a valid
   allocation-free `not_run_memory_safety` outcome;
6. Q8 passes the prerequisite or is reported as quality-ineligible with no
   performance conclusion;
7. the final report reports embedding, retrieval, directly measured total, and
   quality eligibility without adding separately measured percentiles; and
8. normal repository tests and release/claim validators remain green.
