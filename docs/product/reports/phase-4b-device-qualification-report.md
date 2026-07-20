# Phase 4b Physical-Device Qualification Report

Date: 2026-07-20
Status: supported-product qualification passed; full Phase 4b contract
incomplete because the 100K diagnostic stress lane was canceled for device
safety

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
matrices and stops only at the absent stress preflight. The 100K diagnostic
lane was permanently canceled at the owner's direction after the physical
device became excessively hot. Under the frozen contract, 100K cannot satisfy
or fail the supported-product gate, so the supported-product result is PASS.
The same contract requires eligible stress evidence for a full Phase 4b
validator result, so the full contract result is incomplete rather than PASS.

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

## 100K diagnostic stress cancellation

The owner permanently stopped `100k-384d-v3-stress` after reporting that the
device was becoming excessively hot. The collector and benchmark application
were stopped, and no VectorKit benchmark process remained on the device. No
further 100K execution is authorized by this qualification.

The accepted stress tree is empty. Five partial F32 files—one preflight and
four query sessions—were moved without byte changes to timestamped
`canceled-by-user` rejected evidence together with a cancellation manifest and
their original-path/SHA-256 inventory. No F32 lifecycle, I8 preflight, I8
query, or I8 lifecycle artifact was accepted. These partial files are not a
stress result and cannot be promoted, relabeled, or used for support or
marketing.

This cancellation does not change the V1 fewer-than-50K product boundary or
the supported 10K/25K/50K result. It does prevent the frozen full Phase 4b
validator from returning PASS: after accepting the supported and graph-free
evidence, it fails closed because the F32 stress preflight is absent.

## Rejected and canceled evidence

The raw rejected tree preserves 35 JSON records. Six belong to the owner-
directed stress cancellation: the cancellation manifest plus the five partial
stress files moved from the accepted tree. The other 29 records preserve
failed attempts, cooling diagnostics, and quarantined evidence, including
serious-thermal outcomes, locked-device launch denials, foreground-false and
no-JSON/transport failures, a CoreDevice disconnect, and a save-directory
collision. The collision exposed a host-collector naming bug; the collector
now uses configuration-scoped device sample IDs and writes each failed attempt
to a unique rejected path without occupying an accepted destination. One
initially accepted 25K/I8 save sample ended in serious thermal state; the
independent audit quarantined it and reran only that v4 path with a new device-
side identity.

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
  fails closed at the intentionally absent stress preflight.
- The accepted stress tree contains zero files; the five partial stress files
  and cancellation manifest are preserved only as rejected evidence.

Supported-product physical-device qualification is complete and passes. Full
Phase 4b contract qualification remains incomplete because the diagnostic
100K lane was canceled; it must not be reported as a full validator PASS. No
production Rust, FFI, Swift API, frozen fixture, workload, threshold, support
boundary, or marketing classification changed.
