# Target-Device Graph Benchmark Contract V1

Status: frozen Phase 4 contract; device scope amended by
`target-device-graph-benchmark-contract-v1-amendment-1.md`

Date frozen: 2026-07-18

This document is the normative contract for RetrievalKit Phase 4 target-device
graph benchmarks. It specializes the device and performance requirements in
section 9 of the previously frozen Graph Retrieval Evaluation Contract V3
without changing any V3 quality population, query, judgment, ranking, metric,
or artifact identity.

Amendment 1, approved on 2026-07-18, makes iPhone 17 Pro Max the sole required
Phase 4b device. iPhone 14 Pro Max is optional future qualification and does
not block the current gate.

## 1. Scope and classification

Phase 4a pre-registers, generates, qualifies, instruments, packages, and
independently validates the benchmark inputs and harnesses on the pinned Apple
M1 Max development Mac. Phase 4a does not execute a physical device and is not
complete until every requirement in section 12 passes for all four workloads.

Phase 4b is the separately authorized physical-device execution task. No
physical-device run may begin during Phase 4a.

The supported-product matrix is `10k-384d-v3`, `25k-384d-v3`, and
`50k-384d-v3`. The separately reported `100k-384d-v3-stress` lane is diagnostic
scaling evidence. It is outside the V1 capacity envelope, cannot satisfy or
fail the supported-product gate, and cannot create a support, quality,
latency, product, or marketing claim. A 100K result or artifact labeled as
supported, production, product-gate, or marketing is invalid. V1 remains
optimized and supported for fewer than 50K chunks. This contract does not
authorize ANN or HNSW.

## 2. Frozen workload policy

All workloads use the same generator and policy descriptor, 384-dimensional
little-endian F32 source embeddings, cosine exact-vector retrieval, F32 and I8
runtime configurations, four chunks per record, top K 10, and the eight query
categories listed below. Active records are generated first, followed by
deleted records. Deleted records are inserted and then tombstoned before graph
construction.

Deleted chunks equal `floor(active_chunks / 10,000) * 100`; deleted records
equal deleted chunks divided by four. Graph nodes are active record nodes plus
active chunk nodes. Directed edges include owns/owned-by per active chunk,
next/previous and deduplicated links/linked-by per active record, plus
optional/optional-by for records whose ordinal is not divisible by five.

| Workload | Class | Active records | Deleted records | Generated records | Active chunks | Deleted chunks | Generated chunks | Nodes | Directed edges |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `10k-384d-v3` | `supported_product` | 2,500 | 25 | 2,525 | 10,000 | 100 | 10,100 | 12,500 | 39,000 |
| `25k-384d-v3` | `supported_product` | 6,250 | 50 | 6,300 | 25,000 | 200 | 25,200 | 31,250 | 97,500 |
| `50k-384d-v3` | `supported_product` | 12,500 | 125 | 12,625 | 50,000 | 500 | 50,500 | 62,500 | 195,000 |
| `100k-384d-v3-stress` | `stress` | 25,000 | 250 | 25,250 | 100,000 | 1,000 | 101,000 | 125,000 | 390,000 |

The query categories are semantic exact-vector, exact-name lexical, weighted
hybrid, metadata-filtered semantic, one-hop graph, two-hop graph, three-hop
graph, and graph selection plus metadata-filter intersection. Fixtures include
cyclic next references, repeated collection references with deterministic
deduplication, absent optional references, deleted records, and deterministic
near distractors.

The compact machine-readable policy is
`benchmarks/device-graph/workloads-v1.json`. Any change to its semantic values,
the binary fixture grammar, embedding generator, reference generator, text or
metadata generator, query categories, stable result policy, or classification
requires a new contract version and workload IDs. Existing artifact identities
must never be silently regenerated under the same ID.

## 3. Fixture generation and qualification

Each workload emits exactly `fixture.bin` and `manifest.json`. The manifest
closes every count in section 2, the generator policy SHA-256, fixture byte
size and SHA-256, manifest schema, workload classification, dimension, source
encoding, runtime configurations, top K, query categories, and result policy.
The 100K manifest must also state that V1 capacity is unchanged.

Qualification requires two complete generations into distinct empty roots,
recursive byte comparison, independent bounded parsing of every binary field,
recomputed file sizes and SHA-256 hashes, rejection of trailing bytes, and a
generated-data Git audit. Generated fixtures, persisted databases, raw samples,
and reports live under ignored output roots and must not be committed. The
checked-in identity registry records only compact counts, sizes, and hashes.

## 4. Correctness and persistence matrix

Every workload runs on the pinned Apple M1 Max in optimized release mode with
both F32 and I8. Each row must prove exact corpus, active/deleted chunk, node,
and edge counts; correct dimension and encoding; the declared stable record
and chunk at rank 1; stable graph selections and ordered paths; metadata
filter and graph/filter intersection behavior; and exclusion of every deleted
record from results.

Each F32/I8 row must build, save to a fresh directory, account for persisted
components, run read-only validation, fully unload, cold-load, replay all
checks, warm-load/replay again where the platform cache is intentionally warm,
and prove stable results, selections, and paths before and after reload.

Persisted component accounting is exact and non-overlapping for corpus/chunks,
vectors and quantization metadata, lexical/BM25, graph/schema, manifest and
validation metadata, and the complete directory. The component sum must equal
the complete-directory byte count.

## 5. Staged measurement protocol

The compact machine-readable protocol is
`benchmarks/device-graph/protocol-v1.json`. Release Swift and optimized
Rust/XCFramework code are required. Embedding execution is excluded.

For each measured graph retrieval operation, record independent integer
nanosecond samples around these non-overlapping boundaries:

1. seed resolution;
2. graph traversal;
3. graph-to-chunk projection;
4. metadata-filter intersection;
5. semantic or hybrid ranking;
6. result hydration; and
7. a directly measured end-to-end total surrounding the complete operation.

The total distribution must never be derived by adding stage samples or stage
percentiles. A sample records the workload, encoding, query/scenario identity,
session, repetition, process identity, stage name, start/end monotonic clock
values, duration, result identities, selection identity, path identity, filter
identity, and lifecycle state.

Warm query measurement uses 100 excluded complete-operation warmups followed
by exactly 1,000 measured samples per configuration. Build, save, read-only
validation, and warm/repeated load use three discarded warmups followed by 20
measurements on fresh uniquely named directories. Cold load uses 20 fresh
processes with no warmup. Reload-equivalence is recorded with every lifecycle
sample.

For each sorted distribution of `n` integer-nanosecond samples and percentile
`p`, nearest rank is mandatory:

```text
index = max(1, ceil(p * n)) - 1
percentile = sorted_samples[index]
```

Report sample count, minimum, maximum, arithmetic mean, P50, P95, and P99.
There are at least three final sessions per device/configuration. Gates use the
median of the three session P95 values; samples from different sessions are
not pooled to conceal variance.

## 6. Memory and device state

Memory measurement runs one scenario per fresh process and samples process RSS
every 1 ms from before the operation until after it completes. Build, save,
load, validation, query, and measured maintenance phases report process
baseline, peak RSS, and peak delta independently. Each scenario has five
fresh-process memory repetitions. Per-component resident memory must not be
inferred by subtraction.

Every device artifact records physical-versus-simulator status; hardware model
and identifier; OS version/build; total RAM; AArch64 dot-product availability;
selected SIMD backend; toolchain and RetrievalKit revision; process ID; release
configuration; workload and fixture hashes; power source; battery level range;
low-power-mode state; thermal state at start and end; free storage; network
state; and foreground state. Serious or critical thermal state invalidates and
requires repeating the session. Simulator output is development evidence and
cannot satisfy a physical-device rule.

The 100K lane first consumes the qualified Mac persisted sizes and conservative
peak estimate. If either exceeds the declared device safety budget, the
iPhone 17 Pro Max row must be `not_run_memory_safety`; repeated unsafe attempts
are forbidden. If safe, it may run one encoding per fresh process. Optional
future devices are not required to run 100K. Every 100K outcome remains
`stress` and non-marketing.

## 7. Isolated release iOS harnesses

Two distinct release application products are required:

- the graph-free product links only `RetrievalKitFFI` and contains no graph
  framework, symbol, state initialization, graph file access, or graph-aware
  dispatch; and
- the graph-capable product links `RetrievalKitGraphFFI` and runs the staged graph
  protocol.

The products must not use runtime flags to turn one shared graph-linked binary
into both lanes. Build logs plus binary linkage/symbol inspection are retained
as proof. Graph-free instrumentation records zero graph state creations, zero
graph file opens, and zero graph dispatches, and proves identical result
identities. Each scenario starts in a fresh release app process.

## 8. Independent artifact validation

The validator is a standalone Python entry point that does not import or call
the Rust generator or benchmark implementation. It fails closed on unknown
fields and validates:

- exact fixture/manifest counts, sizes, hashes, deterministic-generation proof,
  and identity-registry agreement;
- F32 and I8 Mac correctness, stable result/selection/path identities, filters,
  deleted exclusion, save/validate/load/replay, persisted components, and
  memory preflight for all workloads;
- exact sample counts, nearest-rank percentiles, required non-overlapping stage
  boundaries, directly measured totals, lifecycle samples, 1 ms RSS evidence,
  and component-size sums;
- complete device identity, physical/simulator distinction, power and thermal
  validity, fresh-process isolation, five memory repetitions, and three final
  sessions;
- linkage proof and graph-free zero-state/zero-file/zero-dispatch evidence; and
- mandatory `stress`/non-marketing classification for every 100K artifact,
  with rejection of any 100K support, production, product-gate, or marketing
  classification.

Validator unit tests must include positive fixtures and negative mutations for
each rule family. Phase 4a accepts development/simulator preflight artifacts
where physical-device evidence is not yet required; Phase 4b validation
requires the complete physical-device matrix selected by the active scope
amendment, currently iPhone 17 Pro Max only.

## 9. Frozen-artifact non-regression

Phase 4 work must not modify the frozen HotpotQA/V3 quality inputs, manifests,
sealed rankings, result roots, or previously disclosed artifact identities.
The repository's existing frozen-artifact checks must pass after every Phase 4
change. This Phase 4 contract is a later addendum only; it does not reopen V3.

## 10. Git and evidence rules

Phase 4a is committed as six focused commits in this order: protocol,
fixtures, instrumentation, isolated iOS harnesses, independent validation, and
readiness documentation. No prior commit is amended and nothing is pushed.
All verification results and compact fixture identities are documented; large
generated data remains ignored.

## 11. Phase 4b physical-device execution task

After Phase 4a passes and only under a new explicit authorization, build the
frozen release XCFrameworks and the two isolated iOS products once; execute the
10K/25K/50K F32/I8 supported matrix on physical iPhone 17 Pro Max, plus the
preflight-authorized 100K stress lane on iPhone 17 Pro Max only; use one
scenario per fresh process,
100 warmups and 1,000 query samples, 20 lifecycle samples, five 1 ms RSS
repetitions, and three thermally valid final sessions; retain linkage,
graph-free, device/power/thermal, component-size, correctness, and raw-sample
evidence; then run the independent validator in Phase 4b physical-device mode.

## 12. Phase 4a completion gate

Phase 4a is complete only when all four workloads have two byte-identical
generations, closed identities, independent fixture validation, Apple M1 Max
F32/I8 correctness and persistence/replay, complete staged and lifecycle
instrumentation, memory preflight, isolated release iOS harness builds and
linkage proof, passing independent artifact validation, passing frozen V3 and
HotpotQA checks, all repository language checks, a generated-data Git audit,
and a clean worktree containing the six required commits. Until then the
roadmap must say Phase 4 is active, Phase 4a is incomplete, and Phase 4b is
pending.
