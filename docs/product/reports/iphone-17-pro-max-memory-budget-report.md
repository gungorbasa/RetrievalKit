# iPhone 17 Pro Max Memory-Budget Report

Date: 2026-07-11

Device and build:

- iPhone 17 Pro Max (`iPhone18,2`).
- iOS 26.5.1.
- Release Swift app and optimized Rust XCFramework.
- One scenario per fresh app process.
- RSS sampled every 1 ms through Mach task info.

## 24K × 384d I8 Hybrid

Configuration: 50 measured hybrid queries after 3 warm-ups, top 10, 50 vector
and 50 keyword candidates, BM25 persisted, and 25% tombstones before compaction.

| Run | Peak RSS MiB | Delta MiB | Post-load P95 ms | Persisted MiB | Compaction increase MiB | Build ms | Load ms | Compact ms |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 119.98 | 72.94 | 5.627 | 9.226 | 3.95 | 1284.7 | 55.9 | 740.2 |
| 2 | 121.94 | 80.16 | 5.881 | 9.226 | 3.94 | 1297.0 | 56.5 | 753.9 |
| 3 | 122.55 | 80.67 | 5.982 | 9.226 | 4.36 | 1369.0 | 56.1 | 768.8 |
| 4 | 116.97 | 75.23 | 6.187 | 9.226 | 3.89 | 1391.4 | 57.0 | 791.4 |
| 5 | 124.89 | 77.61 | 5.815 | 9.226 | 4.08 | — | — | — |

Provisional gates derived from the observed maxima:

- Peak RSS: 140 MiB.
- Peak delta from process baseline: 96 MiB.
- Persisted size: 20 MiB.
- Post-load P95: 10 ms.
- Compaction peak increase over its starting RSS: 8 MiB.

The fifth run used the final checked-in budgets and passed every gate. The
compact target therefore has repeatable headroom on this device.

## 50K × 384d I8 Hybrid

The same workload was repeated three times at 50K chunks.

| Run | Peak RSS MiB | Delta MiB | Post-load P95 ms | Persisted MiB | Compaction increase MiB | Build ms | Load ms | Compact ms |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 198.02 | 156.23 | 13.564 | 19.208 | 11.64 | 5345.4 | 121.5 | 3155.4 |
| 2 | 196.92 | 155.16 | 13.802 | 19.208 | 8.92 | 5808.7 | 120.0 | 3477.1 |
| 3 | 198.02 | 156.23 | 14.479 | 19.208 | 28.84 | 6073.9 | 129.4 | 3432.5 |

Provisional extended-capacity gates:

- Peak RSS: 224 MiB.
- Peak delta: 180 MiB.
- Persisted size: 24 MiB.
- Post-load P95: 16 ms.
- Compaction peak increase: 40 MiB.

50K is usable on this device but does not meet RetrievalKit's original 5–10 ms
retrieval goal. Treat it as an extended-capacity profile, not the primary V1
performance tier. Compaction also needs substantially more safety headroom than
the 24K profile because one run observed a 28.84 MiB transient increase.

## 768d I8 Hybrid

The primary 24K tier remained within the latency and persisted-size goals when
the vector dimension doubled.

| Run | Peak RSS MiB | Delta MiB | Post-load P95 ms | Persisted MiB | Compaction increase MiB | Build ms | Load ms | Compact ms |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 162.42 | 120.61 | 6.392 | 18.015 | 3.86 | 1444.0 | 74.6 | 779.4 |
| 2 | 137.78 | 96.09 | 7.827 | 18.015 | 3.67 | 1323.6 | 75.4 | 735.1 |
| 3 | 133.84 | 92.08 | 6.464 | 18.015 | 2.13 | 1355.0 | 76.9 | 782.2 |
| 4 | 140.67 | 93.47 | 6.240 | 18.015 | 3.34 | — | — | — |

The checked-in 24K × 768d gates are 184 MiB peak RSS, 136 MiB baseline delta,
20 MiB persisted size, 10 ms post-load P95, and 8 MiB compaction increase.

A diagnostic `50K × 768d I8` run measured 220.02 MiB peak RSS, 178.33 MiB
baseline delta, 13.850 ms P95, 37.519 MiB persisted size, and a 41.91 MiB
compaction increase. It is operational on this device but misses the compact
size target and remains in the extended-capacity tier. More samples are needed
before assigning it a regression budget.

## F16 and F32 Reference Profiles

Single 24K comparison runs show that all encodings remain within the 10 ms
latency target on this device, but persisted size and RSS separate their roles.

| Dimension | Encoding | Peak RSS MiB | Delta MiB | Post-load P95 ms | Persisted MiB | Compaction increase MiB |
|---:|:---|---:|---:|---:|---:|---:|
| 384 | F16 | 137.33 | 95.48 | 6.967 | 17.923 | 3.52 |
| 384 | F32 | 192.83 | 151.17 | 8.195 | 35.501 | 3.89 |
| 768 | F16 | 192.77 | 150.45 | 8.943 | 35.501 | 2.17 |
| 768 | F32 | 300.02 | 258.17 | 9.219 | 70.658 | 3.69 |

`384d F16` fits 20 MiB for this synthetic corpus, but previous fuller-data
measurements exceeded that target. It remains an optional quality-preserving
encoding rather than the universal compact default. F32 is the correctness and
recall reference. At 768d, only I8 meets the persisted-size goal.

50K F16/F32 profiles were not run because their vector payloads alone exceed
the compact budget. The 24K measurements already establish the expected scope
limit without spending additional device time on configurations that cannot
meet the product's size requirement.

## Vector-Only Comparison

One `24K × 384d I8` vector-only run omitted BM25 from persistence:

- Peak RSS: 95.72 MiB.
- Persisted size: 9.023 MiB.
- Post-load P95: 0.248 ms.
- Load: 24.3 ms.
- Compaction peak increase: 2.94 MiB.

BM25 adds only about 0.203 MiB to this compressed synthetic snapshot, but
reconstructing it materially affects load RSS and hybrid query latency. The
current build path still constructs BM25 before vector-only persistence, so the
vector-only build phase is not a keyword-disabled core configuration.

## Recommendation

- Keep `24K × 384d/768d I8` hybrid as the supported compact target.
- Document 50K exact hybrid retrieval as a higher-latency tier on current V1.
- Require at least 8 MiB maintenance headroom for the compact target and 40 MiB
  for the measured 50K profile on this device class.
- Repeat F16/F32 measurements only if they become supported non-compact tiers.
- Do not start ANN work from these results alone. Measure candidate limits and
  realistic retrieval quality first, then test exact-scan optimizations if 50K
  latency remains a product requirement.
