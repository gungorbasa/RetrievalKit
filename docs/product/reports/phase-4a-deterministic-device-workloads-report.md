# Phase 4a Target-Device Graph Benchmark Readiness Report

Date: 2026-07-18
Status: Phase 4 is active; Phase 4a complete; Phase 4b physical-device
execution pending

## Result

Phase 4a passed the frozen
`docs/product/target-device-graph-benchmark-contract-v1.md`. All four workloads
have two byte-identical generations, independently validated closed counts and
hashes, Apple M1 Max F32/I8 correctness and persistence/replay, stable result,
selection, and path identities, filters and deleted-result exclusion, exact
persisted component accounting, memory preflight, complete staged query
instrumentation, and isolated arm64 release iOS harness builds with linkage
proof. No physical-device execution occurred.

Phase 4a required six focused commits. The full verification matrix passed
before the readiness commit. Phase 4b remains pending and no public quality,
device-performance, or marketing claim is authorized.

## Classification and product boundary

The supported-product benchmark matrix is `10k-384d-v3`, `25k-384d-v3`, and
`50k-384d-v3`. The `100k-384d-v3-stress` workload is diagnostic scaling
evidence outside VectorKit V1's supported capacity envelope. It cannot satisfy
or fail the supported-product gate, cannot create a product, support, quality,
latency, or marketing claim, and does not authorize ANN/HNSW. V1 remains
optimized and supported for fewer than 50K chunks.

## Frozen fixtures

Both complete generation roots were recursively byte-identical. The Rust
bounded parser and the standalone Python validator independently parsed the
fixture grammar, closed active/deleted and graph counts, rejected trailing
bytes, recomputed byte sizes and SHA-256 hashes, and matched the checked-in
identity registry.

| Workload | Class | Active/deleted records | Active/deleted chunks | Nodes/edges | Fixture bytes / SHA-256 | Manifest bytes / SHA-256 |
|---|---|---:|---:|---:|---|---|
| `10k-384d-v3` | supported | 2,500 / 25 | 10,000 / 100 | 12,500 / 39,000 | 17,057,298 / `8e85cfcb235b60175389fef07f7e4cc6aa68db794686bd12aa1a981d53bd10a4` | 1,104 / `fbff45e658a54f50b649a1e6d1eecd24fe93c22ec769364e575de68945e012e9` |
| `25k-384d-v3` | supported | 6,250 / 50 | 25,000 / 200 | 31,250 / 97,500 | 42,558,303 / `4d3cd6087c775e11f5cb9147421115763647e49b0ad97c1889a6a52e933a5ddf` | 1,104 / `5e3fbb4230ad09012aff5e48508efd4497390c4973e7edf472278e88bb69d4fb` |
| `50k-384d-v3` | supported | 12,500 / 125 | 50,000 / 500 | 62,500 / 195,000 | 85,285,318 / `fc93eec7c3fdc0b9bdee95a34dc626b98d5bdca0e77c460506d665a5588a9dda` | 1,108 / `ba06088e2140b1450e2d52d000b44274804272dd6b6ffa29a69c16f202013db9` |
| `100k-384d-v3-stress` | stress | 25,000 / 250 | 100,000 / 1,000 | 125,000 / 390,000 | 170,533,132 / `756949033b63d4163b34162466f968f0b24ca355d3f8341aece0424170a86a29` | 1,110 / `8d76a9044cf46fbbe17416143b69f3bf53c10726da72d682503b4c707c0fc052` |

The previously qualified 100K artifact identities were retained exactly.

## Apple M1 Max correctness and persistence

All eight optimized release configurations passed exact corpus, chunk,
tombstone, node, and edge counts; the eight declared query categories; F32/I8
stable top identities; graph selections and ordered paths; metadata and
graph/filter intersections; deleted-result exclusion; save; read-only
validation; full unload; cold load and replay; and warm load and replay.

Persisted sizes were independently split into corpus/chunks, vectors and
quantization metadata, lexical/BM25, graph/schema, and manifest/validation
metadata. Every component sum equaled the complete directory byte count.

| Workload | Encoding | Persisted total | Retrieval | Graph/schema | Loaded estimate | Conservative peak estimate |
|---|---|---:|---:|---:|---:|---:|
| 10K | F32 | 18,752,876 | 15,755,585 | 2,996,567 | 21,489,872 | 156,269,308 |
| 10K | I8 | 7,158,089 | 4,160,798 | 2,996,567 | 9,895,072 | 133,079,708 |
| 25K | F32 | 46,737,904 | 39,247,363 | 7,489,817 | 53,627,922 | 239,478,408 |
| 25K | I8 | 17,808,317 | 10,317,776 | 7,489,817 | 24,698,322 | 181,619,208 |
| 50K | F32 | 93,711,854 | 78,732,560 | 14,978,567 | 107,447,072 | 378,671,708 |
| 50K | I8 | 35,737,868 | 20,758,574 | 14,978,567 | 49,473,072 | 262,723,708 |
| 100K stress | F32 | 187,486,412 | 157,529,602 | 29,956,075 | 214,847,070 | 656,581,736 |
| 100K stress | I8 | 71,538,425 | 41,581,615 | 29,956,075 | 98,899,070 | 424,685,736 |

The 100K preflight retained the previously qualified estimate of 656,581,736
bytes against a 1,610,612,736-byte memory budget and retained the exact F32/I8
persisted totals. It is safe to attempt on the headline device in Phase 4b,
one encoding per fresh process. This remains a safety decision, not a support
or performance claim.

## Staged protocol qualification

Each F32/I8 configuration completed 100 excluded warmups and 1,000 raw query
samples. Every sample records non-overlapping seed resolution, traversal,
projection, filter intersection, ranking, and hydration stages plus a directly
measured end-to-end total. Results, graph selections, paths, and filter
identities were stable across all 8,000 samples, with zero deleted results.
The validator recomputed count, minimum, maximum, integer mean, and nearest-rank
P50/P95/P99 from the raw integer-nanosecond samples.

These M1 Max microbenchmarks qualify instrumentation and artifact structure.
They are not device-performance results and are not eligible for marketing.
The frozen lifecycle protocol additionally requires build, save, read-only
validation, cold load, warm load, and replay-equivalence evidence; 1 ms RSS;
five memory repetitions; one scenario per fresh process; and three thermally
valid final sessions during Phase 4b.

## Isolated iOS products

The release Xcode project now produces two distinct arm64 iOS apps:

- `VectorKitIOSBench` links only the base `VectorKitFFI`. Binary symbol
  inspection found no `_vectorkit_graph_*` symbol. Its environment evidence
  records zero graph state creations, graph file opens, and graph dispatches.
- `VectorKitIOSGraphBench` links only the aggregate `VectorKitGraphFFI` and
  contains `_vectorkit_graph_ffi_abi_version`. It preflights workload and
  encoding classification, release/fresh-process rules, physical versus
  simulator state, device identity, thermal/power/storage state, the seven
  stages, lifecycle sample counts, 1 ms RSS, five repetitions, and three
  sessions. A serious/critical thermal state is rejected.

Both release device builds passed without code signing. Linkage validation
passed through `scripts/verify-ios-benchmark-linkage.sh --skip-build`. No app
was installed on or run on a physical device.

## Independent validation

`benchmarks/device-graph/validate_artifacts.py` does not import or call the
Rust generator or benchmark implementation. Phase 4a validation passed:

- four exact fixture identities across two complete generations;
- eight F32/I8 Mac correctness and persistence rows;
- 8,000 raw staged samples and all nearest-rank summaries;
- stable result, selection, path, and filter identities;
- lifecycle boundaries and persisted-component sums;
- 100K stress/non-marketing enforcement; and
- base/graph arm64 binary linkage and graph-free instrumentation presence.

Its Phase 4b mode additionally rejects missing three-session evidence,
reused processes, simulators, serious/critical thermal state, incomplete
device identity, non-1-ms RSS, fewer than five memory repetitions, incomplete
lifecycle samples, and nonzero graph-free state/file/dispatch evidence.
Negative tests cover percentile, sample-count, stage-boundary, thermal,
session, graph-free, and 100K classification failures.

## Verification

The final verification ran Rust formatting, all-feature tests, warning-denying
Clippy, Swift base/graph suites, complete base and graph XCFramework builds,
arm64 release device builds and linkage checks, Python validator tests,
`py_compile`, Ruff, two complete generations and recursive comparison of all
workloads, all eight Mac correctness/persistence rows, all eight staged rows,
the independent Phase 4a validator, generated-data Git audit, frozen
HotpotQA/V3 checks, `git diff --check`, and the clean-worktree check.

## Exact Phase 4b task

Build the frozen release `VectorKitFFI` and `VectorKitGraphFFI` XCFrameworks and
the two isolated iOS products once; execute the 10K/25K/50K F32/I8 supported
matrix on physical iPhone 17 Pro Max and the contract-required conservative
iPhone 14 Pro Max lanes, plus the preflight-authorized 100K stress lane on
iPhone 17 Pro Max only; use one scenario per fresh process, 100 warmups and
1,000 query samples, 20 lifecycle samples, five 1 ms RSS repetitions, and three
thermally valid final sessions; retain raw stage/total, lifecycle,
component-size, correctness, linkage, graph-free, device, power, and thermal
evidence; then run the standalone validator in `--mode phase4b`. Do not pool
sessions or convert the 100K stress row into a support or marketing claim.
