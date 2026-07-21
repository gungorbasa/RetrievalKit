# Retrieval quality and correctness

Scope: frozen HotpotQA test workload; 296 common valid queries; weighted-I8 whole-corpus baseline versus graph-scoped retrieval.

| Metric | Baseline | Graph-scoped | Delta | Relative | W/T/L |
| --- | ---: | ---: | ---: | ---: | ---: |
| NDCG@10 | 0.858036 | 0.927909 | 0.069873 | 8.14% | 121/157/18 |
| Recall@10 | 0.871622 | 0.957770 | 0.086149 | 9.88% | 69/211/16 |
| Complete evidence@10 | 0.743243 | 0.922297 | 0.179054 | 24.09% | 69/211/16 |

The graph-scoped lane's mean per-query candidate reduction was 972.65x. Candidate recall was 96.79% and candidate complete evidence was 94.26%; empty scopes: 0.

For contrast, the pooled totals were 3,750,320 eligible and 6,326 projected chunks (592.84x). The 972.65x figure is the macro mean of per-query ratios, not this pooled ratio.

These are workload-scoped quality observations. Losses are preserved, latency is not inferred, and no universal graph winner claim is permitted.
