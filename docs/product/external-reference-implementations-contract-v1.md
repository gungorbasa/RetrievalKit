# External Reference Implementations Contract V1

Status: frozen Phase 5 methodology

Date frozen: 2026-07-20

This document is the normative contract for the benchmark roadmap's Phase 5,
"External Reference Implementations." It is unrelated to the implementation
roadmap's release-and-distribution Phase 5. It does not reopen Phase 4, change
the fewer-than-50K V1 product boundary, authorize ANN in production VectorKit,
or authorize any physical-device execution.

## 1. Purpose and lanes

Phase 5 publishes reproducible benchmark-only adapters in three lanes:

1. **Exact engine isolation:** VectorKit F32 exact and sqlite-vec brute-force
   `vec0` receive the same normalized F32 vectors, queries, top K, equality
   filters, and active/deleted identities. NumPy F32 matrix scoring is the
   independent result oracle and is not included in comparative latency.
2. **Recall-constrained embedded ANN:** USearch HNSW receives the same
   unfiltered normalized F32 inputs. Latency is reportable only when mean
   Recall@10 against the NumPy F32 oracle is at least `0.99`. The Python
   USearch binding does not expose predicate filtering, so filtered ANN is
   `unsupported`, not replaced with post-filtering or an unfiltered query.
3. **Complete application:** VectorKit graph selection and scoped retrieval are
   compared with a competent SQLite application stack using sqlite-vec scalar
   cosine distance, FTS5, explicit adjacency tables, application-side fusion,
   metadata joins, and one SQLite transaction. Graph selection and retrieval
   remain distinct timed stages. FTS5 tokenization/BM25 and fusion are not
   semantically identical to VectorKit, so hybrid rankings are
   `non_equivalent`; only separately reported application workflows are valid.

Adapters live only under `benchmarks/external-reference/`. They must not add a
production dependency, alter `vectorkit-core`, add ANN/HNSW to VectorKit, or
change a public Rust, Swift, Python, or FFI API.

## 2. Selected systems and versions

| System ID | Version | Role | License | Selection reason |
| --- | --- | --- | --- | --- |
| `vectorkit_f32_exact` | source commit recorded per run | product exact reference | repository MIT | measures the production exact engine without adding an adapter to production code |
| `vectorkit_graph_app` | source commit recorded per run | product complete-application reference | repository MIT | measures production graph selection and scoped retrieval in a process isolated from the graph-free Python distribution |
| `numpy_f32_oracle` | `2.5.1` | independent exact identities and recall | BSD-3-Clause and bundled compatible licenses | mature independent dense-array implementation; excluded from comparative timing |
| `sqlite_vec_exact` | `0.1.9` | embedded brute-force exact reference | MIT OR Apache-2.0 | small embedded C/SQLite engine with cosine distance, metadata columns, deletion, and persistence |
| `usearch_hnsw` | `2.26.0` | embedded ANN reference | Apache-2.0 | mature cross-platform HNSW with explicit connectivity/build/search expansion and persistence |
| `sqlite_custom_graph_app` | SQLite from CPython plus sqlite-vec `0.1.9` | vector-plus-custom-graph application | SQLite public domain; sqlite-vec MIT OR Apache-2.0 | representative single-file application stack with explicit graph, lexical, filter, join, and transaction logic |

`sqlite-vec` is pre-1.0 and its bindings do not promise semantic-versioning
stability. That risk is why the exact package version, wheel hashes, runtime
extension version, and SQL schema are captured. No sqlite-vec experimental ANN
prerelease is used.

The canonical measured platform is Apple Silicon macOS with CPython `3.12.13`.
Linux x86-64/aarch64 and Windows x86-64 may reproduce dependency-supported
lanes, but their output is a distinct environment and must not be pooled with
the canonical Mac run. Swift and Rust tool versions are recorded because the
VectorKit wheels are built from this repository, but no Swift/device harness is
used by Phase 5. Maturin `1.14.1` is the pinned local VectorKit wheel builder.

## 3. Workloads, splits, and identities

Every workload is generated from a checked-in JSON configuration and the
canonical generator version `phase5-generator-v1`. Inputs are normalized F32
cosine vectors in little-endian row-major order. Embedding execution is absent.
The input identity is SHA-256 over the generator ID, workload configuration,
vector bytes, query bytes, record/chunk identities, metadata, deletion set,
graph edges, query specifications, and expected graph paths.

The development split is `phase5-development-v1`. It may be used only for
adapter correctness and ANN configuration checks. The final local comparison
split is `phase5-mac-comparison-v1`; it has different seeds and must not change
after any result from that split is inspected. The frozen USearch parameters
are connectivity `16`, build expansion `128`, search expansion `128`, F32
storage, one build thread, and one search thread. Phase 5 has no per-workload or
per-query tuning.

The final workload IDs are `10k-384d-v1`, `25k-384d-v1`, and
`50k-384d-v1`. The 50K row is boundary benchmark evidence only and does not
change the supported fewer-than-50K product statement. The smoke workload is
`256-32d-smoke-v1` and cannot support a performance or marketing claim.

Each corpus uses four chunks per record, deterministic tenant/category fields,
active and deleted records, explicit `next`/`linked` graph relationships,
deterministic lexical tokens, exact semantic queries, equality-filtered
semantic queries, and graph-selection-plus-filter workflows. Query IDs, result
ties, graph paths, and deletion identities are deterministic. Equal scores sort
by stable chunk identity in ascending byte order.

## 4. Capability mapping

`benchmarks/external-reference/feature-parity-v1.json` is normative. Each cell
is one of:

- `equivalent`: the operation and observable correctness contract are directly
  comparable;
- `application_equivalent`: the application implements the same product task
  from separately coordinated components, while stage boundaries remain
  visible;
- `non_equivalent`: an implementation exists but semantics differ materially;
- `unsupported`: no suitable operation exists in the selected binding/version;
- `not_measured`: the system may support it, but this contract does not test it.

Unsupported and non-equivalent cells must be emitted into every result root.
They must not be dropped, scored as zero, substituted with an easier operation,
or used in a direct relative-performance statement.

## 5. Correctness and recall gates

The exact lane passes only when, for every executed unfiltered and supported
filtered query:

- result IDs exactly equal the NumPy F32 oracle through top 10;
- no deleted identity appears;
- repeated samples retain the same ordered result identity; and
- save/load replay retains the same ordered result identity.

The ANN lane passes only when:

- mean Recall@10 against the exact oracle is at least `0.99`;
- every query returns ten active unique IDs when the corpus permits it;
- no deleted identity appears;
- repeated samples retain one ordered result identity per query; and
- save/load replay preserves the ANN result identity.

ANN latency from a row that misses recall is retained but classified
`recall_gate_failed` and is prohibited from a latency comparison. Filtered ANN
is always an explicit `unsupported_operation` for the selected Python binding.

The application lane passes exact scoped retrieval only when graph selection,
metadata intersection, final IDs, deleted exclusion, and reload replay match
the independent fixture expectations. Hybrid output is checked for internal
determinism, complete stage execution, deletion/filter correctness, and reload
stability, but is not required to equal VectorKit because its tokenizer, BM25,
normalization, and fusion semantics are non-equivalent.

## 6. Measurement protocol

The final local profile uses one fresh process per system/workload, `20`
untimed warmups, and `100` integer-nanosecond measured samples per operation.
The smoke profile uses `2` warmups and `5` samples. Adapters run single-threaded
where the dependency exposes controls. Query inputs are prepared before the
timer. Embedding, fixture generation, package import, build, save, load, and
result serialization are outside retrieval timing and are reported separately.

Exact and ANN operations time only the engine call plus compact ordered ID
materialization. Application operations record graph selection, candidate
projection/filter intersection, vector ranking, lexical ranking, fusion,
hydration, and a directly measured end-to-end total. End-to-end samples are
never derived by summing stage samples or percentiles.

Every distribution retains raw samples and reports count, minimum, maximum,
integer arithmetic mean, P50, P95, and P99 using nearest rank:

```text
index = max(1, ceil(p * n)) - 1
percentile = sorted_samples[index]
```

Peak RSS is process-level `ru_maxrss`, normalized to bytes by platform. It is
not component memory and cannot be compared across different Python versions
or processes as though it were an isolated index allocation. Persistence bytes
include every file below the adapter's persistence root. SQLite rows record
database, WAL, and shared-memory bytes separately before a final checkpoint and
the complete post-checkpoint size. Build and load time are separately reported.

## 7. Failures and unsupported operations

Missing wheels, extension-load failures, unsupported platforms, missing
VectorKit bindings, recall failures, build/load errors, schema mismatches,
timeouts, and correctness failures are artifacts. The runner records a stable
classification, system/workload/operation, exception type, message, and stage.
It must not fabricate measurements, silently skip the row, retry with changed
parameters, or substitute another engine.

The final acceptance gate requires successful exact VectorKit and sqlite-vec
rows, a successful recall-gated USearch row or an honestly preserved external
dependency/platform failure, and a successfully built and exercised custom
graph application. A dependency failure prevents a complete comparison result
but does not invalidate the harness implementation.

## 8. Artifact schema and canonical hashing

Each result root contains exactly:

```text
config.json
environment.json
feature-parity.json
input-manifests.json
raw-measurements.jsonl
raw-results.jsonl
failures.jsonl
summary.json
checksums.json
manifest.json
```

All JSON is UTF-8, sorted-key, compact canonical JSON followed by one LF. JSONL
applies that rule independently to every line and sorts rows by their declared
identity tuple. Non-finite numbers are forbidden. Durations and byte counts are
integers. `checksums.json` contains the exact nine-file preimage excluding
itself and `manifest.json`; `manifest.json` binds the contract, configuration,
source revision, file inventory, and canonical sorted path/SHA-256 artifact-set
identity. Paths are relative, normalized, and may not escape the root.

Timing samples are intentionally environment-dependent, so complete result
roots are not expected to be byte-identical across executions. Input manifests,
query/result identities, capability classifications, calculations, and file
ordering must be deterministic. A `--determinism-check` run executes two smoke
roots and compares those deterministic projections while retaining both raw
timing roots.

## 9. Independent validation and mutation coverage

`validate_artifacts.py` must not import the runner or adapter modules. It
independently:

- enforces the closed file inventory and rejects symlinks/unknown fields;
- recomputes every SHA-256 and canonical artifact-set identity;
- regenerates input identities from the checked-in configuration;
- checks system/package/runtime versions and environment completeness;
- recomputes distributions, exact equality, ANN Recall@10, deletion/filter
  checks, stage separation, persistence sums, and acceptance gates;
- enforces every unsupported/non-equivalent feature classification; and
- rejects prohibited interpretations or any device/100K Phase 4 evidence.

Tests include unit, integration, determinism, and negative mutations for
hashes, inventory, percentiles, recall, deleted IDs, result identity, capability
classification, timing boundary, persistence accounting, and path traversal.

## 10. Acceptance and prohibited interpretations

Phase 5 is complete when an external developer can install the pinned tools,
generate inputs, execute every supported lane, inspect raw samples/results and
failures, independently validate the emitted root, and reproduce deterministic
identities from the public instructions.

Phase 5 does not authorize:

- "fastest", "best", or general superiority claims;
- comparing ANN latency when Recall@10 is below `0.99`;
- comparing unfiltered ANN with a filtered VectorKit query;
- treating FTS5/application fusion as VectorKit hybrid parity;
- treating process RSS as isolated engine memory;
- treating smoke or one-Mac output as device, iPhone, energy, thermal, public
  performance, or marketing evidence;
- claiming broader graph database functionality;
- changing the fewer-than-50K V1 boundary;
- adding ANN/HNSW to production VectorKit; or
- installing, launching, resuming, or executing a physical-device benchmark.
