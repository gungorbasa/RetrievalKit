# Mac systems performance

Frozen Apple M1 Max exact-search benchmark; macOS 26.5.2 / Darwin 25.5.0; 384d top-10; 20 warmups and 100 measured queries; embedding excluded.

| Size | Filtered | VectorKit P50/P95 ms | sqlite-vec P50/P95 ms | P50 sqlite/VectorKit |
| --- | --- | ---: | ---: | ---: |
| 10K | no | 0.881 / 2.240 | 6.315 / 6.683 | 7.17x |
| 10K | yes | 0.315 / 0.384 | 3.265 / 3.621 | 10.38x |
| 25K | no | 2.039 / 2.248 | 15.497 / 16.097 | 7.60x |
| 25K | yes | 0.915 / 1.058 | 8.314 / 8.883 | 9.08x |
| 50K | no | 4.184 / 4.321 | 30.480 / 31.189 | 7.29x |
| 50K | yes | 1.911 / 2.146 | 16.111 / 17.321 | 8.43x |

VectorKit revision `9c784d2f11b91bb907150aa1b6046880ff89fde6` and sqlite-vec 0.1.9 both passed frozen exact identity, filtering, deletion, determinism, and reload gates against the NumPy 2.5.1 oracle.

## ANN negative result

| Size | USearch 2.26.0 mean Recall@10 | Gate | Timing comparison |
| --- | ---: | --- | --- |
| 10K | 0.965 | failed | disqualified |
| 25K | 0.850 | failed | disqualified |
| 50K | 0.775 | failed | disqualified |

No USearch performance comparison is made. The graph application timings are also omitted from winner comparison because the hybrid semantics are non-equivalent. These observations do not establish universal VectorKit superiority.
