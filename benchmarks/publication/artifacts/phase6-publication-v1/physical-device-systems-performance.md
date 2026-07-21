# Physical-device systems performance

Physical iPhone 17 Pro Max (`iPhone18,2`, `V54AP`). Query sessions and 10K F32 prepare evidence: iOS 26.5.1 (23F81). Remaining lifecycle evidence: iOS 26.5.2 (23F84). Embedding excluded.

Each value below is the median of five session nearest-rank percentiles; each session contains 1,000 measured queries after 100 warmups.

| Workload | Encoding | Category | P50/P95/P99 ms |
| --- | --- | --- | ---: |
| 10k-384d-v3 | f32 | semantic | 0.518 / 0.528 / 0.535 |
| 10k-384d-v3 | f32 | exact_name | 0.001 / 0.001 / 0.002 |
| 10k-384d-v3 | f32 | hybrid | 0.538 / 0.555 / 0.564 |
| 10k-384d-v3 | f32 | metadata_filter | 0.311 / 0.331 / 0.339 |
| 10k-384d-v3 | f32 | graph_1hop | 0.002 / 0.003 / 0.003 |
| 10k-384d-v3 | f32 | graph_2hop | 0.002 / 0.003 / 0.004 |
| 10k-384d-v3 | f32 | graph_3hop | 0.003 / 0.003 / 0.003 |
| 10k-384d-v3 | f32 | graph_filter | 0.003 / 0.003 / 0.004 |
| 10k-384d-v3 | i8 | semantic | 0.096 / 0.103 / 0.107 |
| 10k-384d-v3 | i8 | exact_name | 0.001 / 0.001 / 0.002 |
| 10k-384d-v3 | i8 | hybrid | 0.116 / 0.123 / 0.128 |
| 10k-384d-v3 | i8 | metadata_filter | 0.197 / 0.211 / 0.223 |
| 10k-384d-v3 | i8 | graph_1hop | 0.002 / 0.002 / 0.002 |
| 10k-384d-v3 | i8 | graph_2hop | 0.002 / 0.002 / 0.003 |
| 10k-384d-v3 | i8 | graph_3hop | 0.002 / 0.003 / 0.003 |
| 10k-384d-v3 | i8 | graph_filter | 0.003 / 0.003 / 0.003 |
| 25k-384d-v3 | f32 | semantic | 1.371 / 1.424 / 1.478 |
| 25k-384d-v3 | f32 | exact_name | 0.001 / 0.001 / 0.002 |
| 25k-384d-v3 | f32 | hybrid | 1.437 / 1.503 / 1.530 |
| 25k-384d-v3 | f32 | metadata_filter | 1.384 / 1.426 / 1.439 |
| 25k-384d-v3 | f32 | graph_1hop | 0.002 / 0.003 / 0.004 |
| 25k-384d-v3 | f32 | graph_2hop | 0.003 / 0.003 / 0.004 |
| 25k-384d-v3 | f32 | graph_3hop | 0.003 / 0.003 / 0.004 |
| 25k-384d-v3 | f32 | graph_filter | 0.003 / 0.003 / 0.004 |
| 25k-384d-v3 | i8 | semantic | 0.272 / 0.305 / 0.324 |
| 25k-384d-v3 | i8 | exact_name | 0.001 / 0.001 / 0.002 |
| 25k-384d-v3 | i8 | hybrid | 0.302 / 0.327 / 0.343 |
| 25k-384d-v3 | i8 | metadata_filter | 0.660 / 0.695 / 0.717 |
| 25k-384d-v3 | i8 | graph_1hop | 0.002 / 0.003 / 0.003 |
| 25k-384d-v3 | i8 | graph_2hop | 0.002 / 0.003 / 0.003 |
| 25k-384d-v3 | i8 | graph_3hop | 0.003 / 0.003 / 0.003 |
| 25k-384d-v3 | i8 | graph_filter | 0.003 / 0.003 / 0.003 |
| 50k-384d-v3 | f32 | semantic | 3.452 / 3.576 / 3.730 |
| 50k-384d-v3 | f32 | exact_name | 0.001 / 0.001 / 0.003 |
| 50k-384d-v3 | f32 | hybrid | 3.243 / 3.425 / 3.472 |
| 50k-384d-v3 | f32 | metadata_filter | 5.183 / 5.293 / 5.436 |
| 50k-384d-v3 | f32 | graph_1hop | 0.002 / 0.004 / 0.005 |
| 50k-384d-v3 | f32 | graph_2hop | 0.003 / 0.003 / 0.004 |
| 50k-384d-v3 | f32 | graph_3hop | 0.003 / 0.003 / 0.004 |
| 50k-384d-v3 | f32 | graph_filter | 0.003 / 0.003 / 0.005 |
| 50k-384d-v3 | i8 | semantic | 0.562 / 0.592 / 0.617 |
| 50k-384d-v3 | i8 | exact_name | 0.001 / 0.001 / 0.002 |
| 50k-384d-v3 | i8 | hybrid | 0.583 / 0.615 / 0.633 |
| 50k-384d-v3 | i8 | metadata_filter | 2.097 / 2.160 / 2.337 |
| 50k-384d-v3 | i8 | graph_1hop | 0.002 / 0.003 / 0.004 |
| 50k-384d-v3 | i8 | graph_2hop | 0.003 / 0.003 / 0.004 |
| 50k-384d-v3 | i8 | graph_3hop | 0.003 / 0.003 / 0.004 |
| 50k-384d-v3 | i8 | graph_filter | 0.003 / 0.003 / 0.004 |

## Graph-free isolation gate

| Encoding | Category | Candidate/baseline median-session P95 | Gate |
| --- | --- | ---: | --- |
| f32 | exact_vector | 1.00x | passed |
| f32 | bm25 | 0.99x | passed |
| f32 | hybrid | 1.00x | passed |
| i8 | exact_vector | 1.01x | passed |
| i8 | bm25 | 0.98x | passed |
| i8 | hybrid | 1.02x | passed |

Supported-product qualification passed for 10K, 25K, and 50K. The graph-free gate passed. This is device qualification, not an external-system winner comparison.

## 100K safety outcome

The 100K stress outcome is `not_run_device_safety`: zero accepted stress artifacts and five rejected partial artifacts. It is not a pass, not supported capacity, and not eligible for performance, latency, quality, product, or marketing use. No device execution was resumed in Phase 6.
