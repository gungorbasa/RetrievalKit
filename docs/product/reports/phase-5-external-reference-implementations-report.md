# Phase 5 External Reference Implementations Report

Status: complete; artifact integrity PASS; final benchmark acceptance failed on
the frozen ANN recall gate; Phase 6 not started

Date: 2026-07-20

## Outcome

Phase 5 delivers reproducible, isolated external implementations for exact
vector search, recall-gated embedded ANN, and a competent SQLite-based custom
graph application. The exact and application lanes passed their correctness,
deletion, filter, persistence, and replay gates. USearch missed the frozen mean
Recall@10 >= 0.99 gate on every final workload, so its latency is retained in
the raw artifacts but excluded from comparative performance conclusions.

The independent validator reports `PASS` for artifact integrity and
`benchmark_acceptance: failed`. This distinction is intentional: the emitted
evidence is complete and internally consistent, while the selected ANN
configuration did not satisfy its quality requirement.

## Selected implementations

| System ID | Implementation/version | Role | License | Final classification |
| --- | --- | --- | --- | --- |
| `numpy_f32_oracle` | NumPy 2.5.1 | independent F32 exact identity and recall oracle | BSD-3-Clause plus bundled compatible licenses | correctness only; timing excluded |
| `vectorkit_f32_exact` | VectorKit source `9c784d2f` | product exact semantic baseline | repository MIT | equivalent exact/filter/delete/save-load |
| `vectorkit_graph_app` | VectorKit source `9c784d2f` | product graph-scoped exact and hybrid workflow | repository MIT | equivalent application workflow |
| `sqlite_vec_exact` | sqlite-vec 0.1.9 | independent embedded exact-vector reference | MIT OR Apache-2.0 | equivalent exact/filter/delete/save-load |
| `usearch_hnsw` | USearch 2.26.0 | embedded HNSW ANN reference | Apache-2.0 | unfiltered ANN measured but recall gate failed; filtered ANN unsupported |
| `sqlite_custom_graph_app` | SQLite 3.50.4, sqlite-vec 0.1.9, FTS5, adjacency tables, application fusion | competent custom assembly baseline | SQLite public domain; sqlite-vec MIT OR Apache-2.0 | application-equivalent structure; hybrid ranking non-equivalent |

No external package became a production dependency. NumPy is independent from
VectorKit and is not used to time a comparison lane. VectorKit graph bindings
run in a separate process from the base bindings because the two native Python
distributions are mutually exclusive.

## Frozen protocol

The checked-in contract, feature-parity matrix, configurations, toolchain, and
hash-locked dependencies define the protocol. Inputs contain four chunks per
record, deterministic F32 cosine vectors, text, tenant/category metadata,
deletions, typed graph edges, and stable query/result identities. Embedding is
excluded.

The final split contains `10k-384d-v1`, `25k-384d-v1`, and `50k-384d-v1`, with
different seeds from the development split. Each timed operation uses 20
untimed warmups and 100 samples in a fresh process. Durations are integer
nanoseconds and distributions use nearest-rank P50/P95/P99. Build, save, load,
peak process RSS, and complete persistence bytes are reported separately.

USearch uses F32 cosine HNSW, connectivity 16, build expansion 128, search
expansion 512, and one build/search thread. Revision 1 used search expansion
128 and achieved development Recall@10 of 0.9875. Before any final-split run,
revision 2 raised only search expansion to 512 and achieved development recall
1.0. The final run was then executed once without workload-specific or
query-specific tuning.

## Feature parity

The normative machine-readable matrix is
`benchmarks/external-reference/feature-parity-v1.json`. The most important
boundaries are:

- VectorKit and sqlite-vec are directly comparable for F32 cosine exact search,
  equality filtering, deletion exclusion, and save/load replay.
- USearch 2.26.0's Python binding does not expose predicate filtering. The
  harness records filtered ANN as unsupported and does not substitute
  post-filtering.
- The SQLite custom application implements graph selection, metadata
  intersection, exact vector scoring, lexical scoring, fusion, hydration,
  deletion, and coordinated persistence. Its FTS5 tokenizer/BM25 and fusion
  semantics differ from VectorKit, so hybrid output and total application
  latency are not direct engine comparisons.
- VectorKit graph state is immutable after build; incremental graph deletion is
  explicitly unsupported. Stable generation-bound selection and composite
  persistence are unsupported in the custom baseline.

## Correctness and recall

All VectorKit and sqlite-vec exact queries returned the NumPy oracle's ordered
top 10 for unfiltered and equality-filtered search. Deleted identities were
excluded, measured results were stable, and save/load replay preserved ordered
identity. Both application lanes returned the expected graph-scoped exact
identity and passed filter/deletion/reload checks; hybrid results were checked
for internal determinism only.

| Workload | USearch mean Recall@10 | Gate | Interpretation |
| --- | ---: | --- | --- |
| 10K × 384d | 0.965 | failed | latency retained, not comparable |
| 25K × 384d | 0.850 | failed | latency retained, not comparable |
| 50K × 384d | 0.775 | failed | latency retained, not comparable |

The decline on the unseen final split means this frozen HNSW configuration is
not an acceptable recall-constrained alternative for these workloads. Phase 5
does not authorize ANN/HNSW in production VectorKit.

## Local exact-search performance

The following values are milliseconds from the single final Apple M1 Max run.
They are local development evidence, not physical-device or marketing claims.

| Workload | System | Unfiltered P50 / P95 | Filtered P50 / P95 |
| --- | --- | ---: | ---: |
| 10K × 384d | VectorKit | 0.881 / 2.240 | 0.315 / 0.384 |
| 10K × 384d | sqlite-vec | 6.315 / 6.683 | 3.265 / 3.621 |
| 25K × 384d | VectorKit | 2.039 / 2.248 | 0.915 / 1.058 |
| 25K × 384d | sqlite-vec | 15.497 / 16.097 | 8.314 / 8.883 |
| 50K × 384d | VectorKit | 4.184 / 4.321 | 1.911 / 2.146 |
| 50K × 384d | sqlite-vec | 30.480 / 31.189 | 16.111 / 17.321 |

On this environment and protocol, VectorKit's exact retrieval path was faster
than sqlite-vec for every accepted exact row. This statement applies only to
the frozen local workloads, selected versions, bindings, filtering semantics,
and timing boundaries. It does not generalize to other devices or datasets.

## Application-stage observations

Directly measured end-to-end application P50/P95 values were 0.073/0.089,
0.119/0.164, and 0.172/0.250 ms for VectorKit at 10K/25K/50K. The custom SQLite
application recorded 0.052/0.060, 0.066/0.078, and 0.086/0.103 ms. Raw stage
rows separately retain graph selection, candidate/filter intersection where
applicable, vector ranking, lexical ranking, fusion, hydration, and direct
end-to-end timing.

These numbers are diagnostic profiles, not a relative winner: VectorKit uses
its native graph-scoped weighted hybrid contract, while the custom application
uses SQLite FTS5 BM25 and different normalization/fusion semantics. The narrow
result is that both complete workflows execute, persist, reload, and return
deterministic filter/deletion-safe results.

## Build, persistence, and memory

Build/save/load times and byte counts are retained per system and workload in
`summary.json`. At 50K, persisted sizes were 75.08 MiB for VectorKit exact,
77.12 MiB for the VectorKit graph application, 77.18 MiB for sqlite-vec exact,
81.13 MiB for USearch, and 115.07 MiB for the custom SQLite application.

Peak RSS is process-level `ru_maxrss`, not isolated index allocation. At 50K it
ranged from 193.41 MiB for sqlite-vec exact to 657.20 MiB for the VectorKit
graph application; these values include Python, bindings, generated fixtures,
build-time state, and process high-water marks. They must not be read as steady
state component memory or compared to Phase 4 physical-device budgets.

## Failures and unsupported operations

The final root contains zero adapter/build/load failures and six explicit
unsupported-operation records: filtered ANN for each workload and incremental
VectorKit graph deletion for each workload. USearch's three recall failures are
represented by failed acceptance rows rather than fabricated adapter errors.

Pre-final development also surfaced and fixed four harness/toolchain issues:

1. CPython 3.12.13 was unavailable in the pinned `uv` runtime catalog, so the
   toolchain was frozen to available CPython 3.12.12 before dependency setup.
2. Resolving the virtualenv interpreter symlink bypassed the environment and
   hid installed modules; the runner now preserves the virtualenv path.
3. Isolated adapter state parents were absent; each worker now creates its
   explicit state root.
4. sqlite-vec rejected a secondary `rowid` KNN order term; the adapter uses its
   supported `ORDER BY distance`. USearch restores its frozen query-time search
   expansion explicitly because version 2.26.0 restores the runtime default of
   64 rather than the configured 512.

No failed development result was used as final comparison evidence.

## Artifacts and validation

The checked-in final root contains exactly 10 files, 5,100 raw measurement
rows, 1,200 raw result rows, and six unsupported-operation rows. Its artifact
set SHA-256 is:

```text
1e7283359f1781dacca1ced3c2fa1794e19a02a2b9669a782465e8f42a8c5602
```

The independent validator reports:

```text
result: PASS
benchmark_acceptance: failed
failure_count: 0
unsupported_operation_count: 6
validated_file_count: 10
validated_measurement_count: 5100
validated_result_count: 1200
```

The manifest binds source revision
`9c784d2f11b91bb907150aa1b6046880ff89fde6`; the source tree was clean before
measurement. Per-file SHA-256 values are in `checksums.json` and
`manifest.json`. Tests independently regenerate input manifests, recompute
hashes/distributions/recall/acceptance, replay deterministic projections, and
reject percentile, parity, inventory, and dishonest gate mutations.

## Reproduction

From the repository root on a compatible Apple Silicon Mac:

```bash
scripts/benchmarks/setup-phase5-external.sh
scripts/benchmarks/run-phase5-external.sh smoke
scripts/benchmarks/run-phase5-external.sh development
scripts/benchmarks/run-phase5-external.sh comparison
scripts/benchmarks/validate-phase5-external.sh \
  benchmarks/external-reference/artifacts/mac-comparison-v1
```

The comparison command exits nonzero because the ANN recall gate failed; the
independent validator exits zero because the resulting failed-gate artifact is
complete and honest.

## Scope and next phase

No physical iPhone was queried, built for execution, heated, or benchmarked in
Phase 5. Phase 4 and Phase 4b accepted evidence was not modified. The 100K
stress branch remains empty and classified `not_run_device_safety`. The 50K
row remains boundary evidence and does not change the supported fewer-than-50K
V1 statement.

Phase 6 has not started. Any public claim or publication step must separately
apply the Phase 6 claim register and evidence rules.
