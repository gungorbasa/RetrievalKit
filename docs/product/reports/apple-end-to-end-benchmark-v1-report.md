# Apple End-to-End Text Search Benchmark Report (Mac V1, iPhone V2)

Status: Mac and physical-iPhone matrices complete and independently validated.

This report implements `docs/product/apple-end-to-end-benchmark-contract-v1.md`
for Mac and the USB-powered iPhone amendment in
`docs/product/apple-end-to-end-benchmark-contract-v2.md`. It measures the
ready-app operation a user invokes when searching:

```text
query text -> public Core ML embedding -> public RetrievalKit search
           -> decoded/hydrated Swift top-10 results
```

Model initialization, database load, corpus embedding, index construction,
save, network access, answer generation, sustained throughput, memory, and
energy are outside the measured boundary. Every value below is the median of
three fresh-process session P95 values, with 50 discarded warmups and 750
retained mixed queries per session.

## Mac result

Device: `MacBookPro18,4`, Apple M1 Max, 32 GB, arm64, macOS 26.5.2
(`25F84`), Release configuration, Core ML compute units `.all`.

| Profile | Chunks | Mode | Embedding P95 | Retrieval P95 | Direct total P95 |
| --- | ---: | --- | ---: | ---: | ---: |
| FP32 production | 10K | vector | 6.780 ms | 0.277 ms | 7.011 ms |
| FP32 production | 10K | weighted hybrid | 9.983 ms | 6.705 ms | 15.190 ms |
| FP32 production | 50K | vector | 8.664 ms | 1.009 ms | 9.552 ms |
| FP32 production | 50K | weighted hybrid | 11.919 ms | 50.975 ms | 59.833 ms |
| FP32 production | 100K | vector | 8.100 ms | 1.861 ms | 9.865 ms |
| FP32 production | 100K | weighted hybrid | 12.145 ms | 132.731 ms | 142.565 ms |
| Q8 experimental | 10K | vector | 5.345 ms | 0.405 ms | 5.781 ms |
| Q8 experimental | 10K | weighted hybrid | 7.087 ms | 6.605 ms | 12.585 ms |
| Q8 experimental | 50K | vector | 5.413 ms | 1.109 ms | 6.348 ms |
| Q8 experimental | 50K | weighted hybrid | 5.440 ms | 50.444 ms | 55.348 ms |
| Q8 experimental | 100K | vector | 4.967 ms | 1.920 ms | 6.743 ms |
| Q8 experimental | 100K | weighted hybrid | 5.453 ms | 135.047 ms | 139.886 ms |

The total is timed directly around the complete public operation; it is not the
sum of stage percentiles. At vector 10K/50K/100K, experimental Q8 changes total
P95 by `-17.5%`, `-33.5%`, and `-31.6%` versus FP32. For weighted hybrid the
changes are `-17.2%`, `-7.5%`, and `-1.9%`. This is consistent with query
embedding dominating vector search while BM25/fusion retrieval dominates the
larger hybrid lanes. Small retrieval deltas between profiles are not evidence
that Q8 accelerates retrieval; each profile has its own matching index and the
retrieval implementation is otherwise the same.

Workload interpretation remains frozen: 10K is normal product evidence, 50K is
a qualification boundary and does not change the fewer-than-50K V1 support
envelope, and 100K is non-marketing stress evidence only.

## Physical iPhone result (USB-powered V2)

Device: physical `iPhone18,2` (iPhone 17 Pro Max), arm64, iOS 26.5.2
(`23F84`), Release configuration, Core ML compute units `.all`, offline with
Wi-Fi and cellular unavailable. The device remained USB-powered at 80% so the
wired CoreDevice channel could launch a fresh process and retrieve evidence for
every session. These numbers are USB-powered latency, not unplugged-user,
energy, or battery-life evidence.

| Profile | Chunks | Mode | Embedding P95 | Retrieval P95 | Direct total P95 |
| --- | ---: | --- | ---: | ---: | ---: |
| FP32 production | 10K | vector | 4.661 ms | 0.117 ms | 4.771 ms |
| FP32 production | 10K | weighted hybrid | 5.716 ms | 3.656 ms | 9.133 ms |
| FP32 production | 50K | vector | 4.572 ms | 0.482 ms | 5.045 ms |
| FP32 production | 50K | weighted hybrid | 10.204 ms | 19.929 ms | 27.532 ms |
| FP32 production | 100K | vector | 4.639 ms | 0.969 ms | 5.572 ms |
| FP32 production | 100K | weighted hybrid | 16.344 ms | 47.258 ms | 57.970 ms |
| Q8 experimental | 10K | vector | 3.507 ms | 0.118 ms | 3.621 ms |
| Q8 experimental | 10K | weighted hybrid | 3.031 ms | 3.204 ms | 6.132 ms |
| Q8 experimental | 50K | vector | 3.253 ms | 0.507 ms | 3.733 ms |
| Q8 experimental | 50K | weighted hybrid | 3.964 ms | 19.494 ms | 22.701 ms |
| Q8 experimental | 100K | vector | 3.235 ms | 1.003 ms | 4.199 ms |
| Q8 experimental | 100K | weighted hybrid | 4.354 ms | 45.973 ms | 49.732 ms |

At vector 10K/50K/100K, experimental Q8 changes iPhone direct-total P95 by
`-24.1%`, `-26.0%`, and `-24.6%` versus FP32. For weighted hybrid the changes
are `-32.9%`, `-17.5%`, and `-14.2%`. Retrieval dominates the larger hybrid
lanes, so faster Q8 embedding has a smaller effect on their complete-operation
latency. Q8 remains experimental regardless of these speedups.

All 36 accepted iPhone sessions began nominal. Thirty-five ended nominal and
one ended fair; none reported a memory warning or left the foreground. An early
10K start and a later Q8 100K start were refused before setup because thermal
state was not nominal. The collector stopped, cooled the device, retained only
already valid reports, and resumed in fresh processes with longer spacing. No
rejected preflight contributed samples, and neither refusal loaded or mutated
the 100K database.

## Q8 prerequisite and limitations

The separate 42-query provider-conformance gate passed:

- median query-vector cosine: `0.9990515950` (minimum `0.995`);
- mean top-10 set overlap: `0.9595238095` (minimum `0.95`);
- minimum per-query top-10 overlap: `0.80` (minimum `0.80`); and
- exact ordered top-10 rate: `0.4761904762` (reported, not gated).

This establishes enough FP32 fidelity to compare performance under this
contract. It does not production-qualify Q8. The pinned upstream Q8 manifest is
also inconsistent with its downloadable package tree, as documented in the
contract. The synthetic latency corpus produced mean top-10 overlap `0.852` in
a separate diagnostic because its templated chunks contain many near-ties; that
diagnostic is retained and is not substituted for the frozen provider gate.

## Retrieval quality: BEIR/TREC-compatible interpretation

The latency corpus is not relevance evidence: it has no independent human
qrels, and result stability or FP32/Q8 overlap cannot answer whether results are
useful to a real user. The provider gate is a model-conformance comparison, not
a relevance evaluation.

Retrieval-quality claims use RetrievalKit's separate collection-plus-qrels
path. It accepts TREC-style qrels and emits deterministic TREC rankings, then
reports macro `NDCG@5`, `NDCG@10`, `Recall@5`, `Recall@10`, `Success@1`, `P@5`,
`MRR@10`, `AP/MAP`, and judged coverage. The existing adapters include BEIR
SciFact and NFCorpus-compatible collections. Rankings and metrics are
independently recomputed rather than trusted from the runtime under test.

Therefore the evidence stack is intentionally split:

1. this Apple benchmark answers ready-app latency on real hardware;
2. provider conformance answers Q8-vs-FP32 embedding/ranking fidelity; and
3. BEIR/TREC qrels runs answer retrieval relevance on judged collections.

A real-application quality gate should add production-derived, privacy-safe
queries and independently judged document qrels to the same TREC-compatible
path, preserve query strata, report per-query wins/ties/losses as well as macro
means, and keep the judged set sealed from alpha/model selection. It should not
reuse this synthetic latency corpus as a relevance benchmark.

## Validation and artifacts

The independent validator checked 36 Mac V1 reports and 36 iPhone V2 reports
(54,000 retained samples total): raw monotonic durations, nested direct timing,
recomputed summaries, deterministic top-10 identity digests within and across
sessions, 72 distinct process IDs, hardware identity, device-validity evidence,
workload/profile classifications, prohibited linkage, and the Q8 prerequisite.
Both complete collections passed.

Ignored evidence roots:

- raw Mac reports and validation: `target/apple-end-to-end/results/mac/observational-v1/`;
- raw iPhone reports and validation:
  `target/apple-end-to-end/results/iphone/usb-powered-observational-v2/`;
- provider gate: `target/apple-end-to-end/quality/q8-vs-fp32-provider-v1.json`;
- latency-corpus overlap diagnostic: `target/apple-end-to-end/quality/q8-vs-fp32-10k.json`;
- verified models and compiled artifacts: `target/apple-end-to-end/models-v1/`;
- six persisted I8 indexes: `target/apple-end-to-end/indexes/`.
