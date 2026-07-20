# Phase 4b Physical-Device Qualification Report

Date: 2026-07-20
Status: active; supported and graph-free evidence complete; 100K stress
evidence pending

## Current result

The supported iPhone 17 Pro Max matrix is complete for `10k-384d-v3`,
`25k-384d-v3`, and `50k-384d-v3` in F32 and I8. It contains 30 query sessions
and 816 lifecycle artifacts: one prepare launch plus the frozen build, save,
read-only-validation, cold-load, warm-load, and replay protocol for each of six
configurations. All 846 accepted supported artifacts have distinct process IDs,
foreground execution, release configuration, Low Power Mode disabled, network
isolation, and nominal or fair thermal state at both boundaries. Load and
replay samples preserve behavioral equivalence.

The supported canonical sorted path/SHA-256 set contains 846 files and hashes
to `f62a0e69c320b5b37d446c96d37f53693ea9e6e4ea2a238a1bffdff06636c93a`.
The standalone Phase 4b validator accepts the complete supported and graph-free
matrices and currently stops only because stress preflight evidence has not yet
been collected. This is not the final Phase 4b qualification result.

## Device and evidence lineage

- Device: iPhone 17 Pro Max (`iPhone18,2`, `V54AP`), CoreDevice ID
  `E342200A-C959-5384-A846-24F4163E5722`.
- The 30 query sessions and the 10K/F32 prepare artifact were collected on
  iOS 26.5.1 (`23F81`). The remaining 815 lifecycle artifacts were collected
  on iOS 26.5.2 (`23F84`), as allowed by the owner-approved reporting variance.
- Authorization v3 SHA-256 is
  `9bc321b7b4ca6970870243a8df0709b9914911b278234bbff229ec1e9fba1240`.
  Its 77 accepted query/prepare/build/save artifacts remain byte-identical at
  artifact-set SHA-256
  `a7d021e0b45fbd2a722482af44428335eac0d8ab188032676c4643e051e7a9dc`.
- Authorization v4 SHA-256 is
  `4f7aab9657bb836e4e434cd701e70ed55dc2cd1adfd4b4d4ec46178f1d76702f`.
  It supplies the other 769 supported lifecycle artifacts.
- The v4 base executable remains `f96b69c5...cae5a9`; the v4 graph executable
  remains `6b6ac8a3...bd97c`. Framework hashes remain the authorized v3/v4
  identities.

## Supported lifecycle latency

Values are nearest-rank milliseconds over 20 measured fresh-process samples.
The three non-cold operations with warmups discarded three launches first;
cold load used no warmup. Inter-process cooling pauses are outside every app
process and measured timer.

| Workload | Encoding | Operation | P50 ms | P95 ms | P99 ms |
|---|---|---|---:|---:|---:|
| 10K | F32 | build | 362.126 | 370.139 | 393.493 |
| 10K | F32 | save | 237.259 | 249.819 | 252.270 |
| 10K | F32 | read-only validation | 127.200 | 129.056 | 129.478 |
| 10K | F32 | cold load | 122.024 | 123.924 | 124.595 |
| 10K | F32 | warm load | 120.688 | 122.862 | 123.794 |
| 10K | F32 | replay | 1.882 | 2.039 | 2.057 |
| 10K | I8 | build | 392.137 | 398.767 | 399.379 |
| 10K | I8 | save | 158.336 | 165.207 | 168.309 |
| 10K | I8 | read-only validation | 102.347 | 104.016 | 104.039 |
| 10K | I8 | cold load | 96.863 | 97.558 | 98.838 |
| 10K | I8 | warm load | 96.779 | 99.993 | 102.806 |
| 10K | I8 | replay | 0.719 | 0.792 | 1.015 |
| 25K | F32 | build | 2,250.838 | 2,355.573 | 2,480.753 |
| 25K | F32 | save | 551.478 | 582.696 | 591.724 |
| 25K | F32 | read-only validation | 251.987 | 255.172 | 255.365 |
| 25K | F32 | cold load | 239.037 | 243.714 | 244.589 |
| 25K | F32 | warm load | 240.566 | 243.602 | 243.708 |
| 25K | F32 | replay | 4.212 | 4.427 | 4.688 |
| 25K | I8 | build | 2,243.697 | 2,344.350 | 2,351.338 |
| 25K | I8 | save | 336.790 | 385.920 | 419.809 |
| 25K | I8 | read-only validation | 200.819 | 204.761 | 207.783 |
| 25K | I8 | cold load | 182.862 | 185.088 | 185.912 |
| 25K | I8 | warm load | 182.857 | 185.321 | 187.026 |
| 25K | I8 | replay | 1.296 | 1.798 | 2.002 |
| 50K | F32 | build | 8,971.179 | 9,978.578 | 11,361.724 |
| 50K | F32 | save | 995.550 | 1,256.771 | 1,315.880 |
| 50K | F32 | read-only validation | 461.183 | 466.053 | 466.249 |
| 50K | F32 | cold load | 434.510 | 438.375 | 453.886 |
| 50K | F32 | warm load | 429.918 | 436.172 | 436.184 |
| 50K | F32 | replay | 7.431 | 7.730 | 7.734 |
| 50K | I8 | build | 8,628.647 | 8,957.722 | 9,816.828 |
| 50K | I8 | save | 599.397 | 611.264 | 627.761 |
| 50K | I8 | read-only validation | 352.749 | 358.131 | 362.576 |
| 50K | I8 | cold load | 327.221 | 342.814 | 360.303 |
| 50K | I8 | warm load | 332.991 | 345.887 | 346.235 |
| 50K | I8 | replay | 2.846 | 5.303 | 5.328 |

## Persisted component accounting

Every prepare, save, validation, and load artifact with component accounting
has an exact component sum equal to its complete directory size.

| Workload | Encoding | Complete bytes | Corpus/chunks | Vectors | BM25 | Graph/schema | Manifest/validation |
|---|---|---:|---:|---:|---:|---:|---:|
| 10K | F32 | 18,752,876 | 132,190 | 15,513,600 | 108,606 | 2,996,567 | 1,913 |
| 10K | I8 | 7,158,087 | 132,190 | 3,918,800 | 108,606 | 2,996,567 | 1,924 |
| 25K | F32 | 46,737,902 | 287,442 | 38,707,200 | 251,531 | 7,489,817 | 1,912 |
| 25K | I8 | 17,808,315 | 287,442 | 9,777,600 | 251,531 | 7,489,817 | 1,925 |
| 50K | F32 | 93,711,852 | 589,638 | 77,568,000 | 573,730 | 14,978,567 | 1,917 |
| 50K | I8 | 35,737,866 | 589,638 | 19,594,000 | 573,730 | 14,978,567 | 1,931 |

Save and read-only-validation artifacts intentionally serialize
`correctness_checks: null`; they prove successful persistence or validation
plus component accounting. Prepare, load, and replay artifacts retain the 11
behavioral checks, and load/replay operations require replay equivalence.

## Graph-free regression

All 12 required sessions are complete: baseline and graph-linked candidate
products, F32 and I8, three fresh-process sessions per product/encoding. Every
session has 100 discarded warmups and 1,000 measured samples for exact,
internal BM25, and weighted hybrid retrieval. Result identities match between
products and all graph-query, visited-node, traversed-edge, and projected-
candidate counters are zero.

The graph-free canonical sorted path/SHA-256 set contains 12 files and hashes
to `6ea55b935ea79933f1ec64d77e88438682d2ae613c7fc0c92c863d58e91f4f3a`.
The gate uses the ratio between candidate and baseline median session P95.

| Encoding | Scenario | Baseline median P95 ns | Candidate median P95 ns | Ratio | Result |
|---|---|---:|---:|---:|---|
| F32 | exact | 528,625 | 529,959 | 1.002524 | pass |
| F32 | internal BM25 | 310,667 | 307,667 | 0.990343 | pass |
| F32 | weighted hybrid | 864,542 | 861,750 | 0.996771 | pass |
| I8 | exact | 96,459 | 96,958 | 1.005173 | pass |
| I8 | internal BM25 | 307,500 | 301,792 | 0.981437 | pass |
| I8 | weighted hybrid | 434,666 | 442,875 | 1.018886 | pass |

Every ratio is below the frozen maximum of `1.03`. The largest measured ratio
is I8 weighted hybrid at `1.018886`.

## Rejected attempts so far

The raw root preserves 27 rejected JSON files representing 25 distinct
attempts: 11 serious-thermal attempts, seven locked-device launch denials,
three foreground-false attempts, two other no-JSON launch/transport attempts,
one CoreDevice disconnect, and one save-directory collision. The collision
exposed a host-collector naming bug; the collector now uses
configuration-scoped device sample IDs. It also writes each failed attempt to
a unique rejected path without occupying an accepted destination. One
initially accepted 25K/I8 save sample ended in serious thermal state; the
independent audit quarantined it and reran only that v4 path with a new
device-side identity.

No rejected artifact was promoted or relabeled. Cooling pauses were used
between fresh processes and are excluded from measured durations.

## Verification at this milestone

- 15 standalone device-tool Python tests pass.
- Python compilation and Ruff pass for `benchmarks/device-graph`.
- The standalone Swift foreground-gate test passes.
- `cargo fmt --all --check` and the eight `vectorkit-phase4-bench` tests pass.
- Isolated iOS release linkage and all authorized app/framework hashes pass.
- Independent inventory confirms 846 accepted supported artifacts and 846
  unique process IDs.
- The split-lineage validator accepts supported and graph-free evidence and
  stops at the missing stress preflight.

The diagnostic 100K stress lane, final split-lineage validation, and final
independent calculations remain. No production Rust, FFI, Swift API, frozen
fixture, workload, threshold, support boundary, or marketing classification
changed.
