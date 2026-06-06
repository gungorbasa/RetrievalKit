# Size and Speed Report

Date: 2026-06-06

## Goal

Keep roughly `24K` vectors plus required local retrieval/display data under a
`20 MiB` persisted footprint while keeping retrieval fast on local devices.
RAM usage is also important, so the same compact layout should be friendly to
in-memory search.

This report uses `MiB` (`1024 * 1024` bytes). A strict decimal `20 MB` budget is
about `19.07 MiB`, so it is tighter than the numbers below by about `0.93 MiB`.

## Current Measured Format

The benchmark now saves a real local index and reports persisted file sizes.
The measured package contains:

- `manifest.json`: small format and configuration metadata.
- `vectors.vec`: raw encoded vector payload.
- `chunks.bin`: compact binary chunk records with text and metadata payloads.
- `bm25.bin`: binary BM25 state, including postings, chunk lengths, and active
  chunk ids.
- `tombstones.bin`: binary tombstone state.

The format is intentionally simple and local-first. It does not include HNSW,
ANN graph data, server state, sync state, or external database files.

## Benchmark Method

The main matrix was run with actual persistence enabled:

```bash
rm -rf /tmp/vectorkit-report-matrix
cargo run --release -p vectorkit-cli -- bench matrix \
  --chunks 24000 \
  --dimensions 384,768 \
  --queries 100 \
  --top-k 5,10 \
  --encodings f32,f16,i8 \
  --persist-dir /tmp/vectorkit-report-matrix \
  --budget-mb 20 \
  --avg-chunk-data-bytes 256 \
  --avg-metadata-bytes 32 \
  --avg-bm25-terms 24
```

The `avg-*` flags still feed the conservative estimator columns printed by the
CLI. The conclusions below use the actual saved file sizes from the persisted
index, not the old estimate.

Detailed file breakdowns were then run for the closest configurations:

```bash
cargo run --release -p vectorkit-cli -- bench synthetic \
  --chunks 24000 \
  --dimension 384 \
  --queries 100 \
  --top-k 10 \
  --encoding i8 \
  --persist-dir /tmp/vectorkit-report-384-i8 \
  --budget-mb 20 \
  --avg-chunk-data-bytes 256 \
  --avg-metadata-bytes 32 \
  --avg-bm25-terms 24
```

The same command shape was used for `384d F16` and `768d I8`.

## Persisted Size, Speed, And Recall

Headroom is calculated from actual persisted size: `20 MiB - persisted MiB`.

| chunks | dim | top_k | encoding | persisted MiB | headroom MiB | avg ms | p95 ms | recall@k vs F32 |
|---:|---:|---:|:---|---:|---:|---:|---:|---:|
| 24K | 384 | 5 | F32 | 39.048 | -19.048 | 3.574 | 3.699 | 1.0000 |
| 24K | 384 | 5 | F16 | 21.470 | -1.470 | 3.689 | 3.811 | 1.0000 |
| 24K | 384 | 5 | I8 | 12.772 | 7.228 | 3.954 | 4.088 | 0.9900 |
| 24K | 384 | 10 | F32 | 39.048 | -19.048 | 3.813 | 3.941 | 1.0000 |
| 24K | 384 | 10 | F16 | 21.470 | -1.470 | 3.881 | 3.990 | 0.9990 |
| 24K | 384 | 10 | I8 | 12.772 | 7.228 | 4.646 | 4.749 | 0.9940 |
| 24K | 768 | 5 | F32 | 74.204 | -54.204 | 6.585 | 6.780 | 1.0000 |
| 24K | 768 | 5 | F16 | 39.048 | -19.048 | 6.661 | 6.756 | 1.0000 |
| 24K | 768 | 5 | I8 | 21.561 | -1.561 | 7.041 | 7.165 | 0.9900 |
| 24K | 768 | 10 | F32 | 74.204 | -54.204 | 6.840 | 6.963 | 1.0000 |
| 24K | 768 | 10 | F16 | 39.048 | -19.048 | 6.852 | 7.023 | 1.0000 |
| 24K | 768 | 10 | I8 | 21.561 | -1.561 | 7.786 | 7.945 | 0.9940 |

## File Breakdown For Key Configurations

`manifest.json` rounded to `0.000 MiB` in these runs, so the table focuses on
the material files.

| config | total MiB | vectors MiB | chunks MiB | BM25 MiB | tombstones MiB | headroom MiB |
|:---|---:|---:|---:|---:|---:|---:|
| 24K x 384d I8 | 12.772 | 8.881 | 1.751 | 2.118 | 0.023 | 7.228 |
| 24K x 384d F16 | 21.470 | 17.578 | 1.751 | 2.118 | 0.023 | -1.470 |
| 24K x 768d I8 | 21.561 | 17.670 | 1.751 | 2.118 | 0.023 | -1.561 |

The main storage bottleneck is still vector payload. After binary chunk and BM25
persistence, the non-vector persisted data in this synthetic benchmark is about
`3.89 MiB` for `24K` chunks. That makes `384d I8` comfortable, but leaves
`384d F16` and `768d I8` just over the `20 MiB` target.

## Comparison Against The Old Estimator

The earlier report used a conservative estimate with `256 B` chunk data, `32 B`
metadata, and `24` BM25 terms per chunk. Actual binary persistence is materially
smaller for the synthetic data shape:

| config | old estimate MiB | persisted MiB | delta MiB |
|:---|---:|---:|---:|
| 24K x 384d I8 | 21.336 | 12.772 | -8.564 |
| 24K x 384d F16 | 30.033 | 21.470 | -8.563 |
| 24K x 768d I8 | 30.125 | 21.561 | -8.564 |

This improvement comes from real binary chunk/BM25 persistence and the current
synthetic payloads being smaller than the conservative placeholder assumptions.
It should not be treated as proof that arbitrary real app data will have the
same footprint.

## Conclusions

`24K x 384d I8ScalarQuantized` is now the clear first compact target. It
persists at `12.772 MiB`, leaving `7.228 MiB` of headroom under a `20 MiB`
budget while retaining `0.9900` recall at `top_k=5` and `0.9940` recall at
`top_k=10` against F32 on the synthetic benchmark.

`24K x 384d F16` is close but still misses the budget at `21.470 MiB`. It is
quality-preserving in the current synthetic run, but needs about `1.47 MiB` of
additional savings to fit.

`24K x 768d I8ScalarQuantized` is also close but misses the budget at
`21.561 MiB`. It needs about `1.56 MiB` of additional savings or fewer chunks
under the same data shape.

`24K x 768d F16`, `24K x 384d F32`, and `24K x 768d F32` are not viable for the
sub-`20 MiB` package target without changing the target, reducing chunks, or
removing required persisted data.

Retrieval-only latency is acceptable on this development machine for the tested
exact full-scan shapes. The relevant compact target, `384d I8`, measured
`4.646 ms` average and `4.749 ms` p95 at `top_k=10`. These are not iPhone or
Swift wrapper numbers, so target-device validation is still required.

## Remaining Risks

- The benchmark uses synthetic chunk text and sparse metadata. Real app payloads
  may make `chunks.bin` materially larger.
- Real BM25 term distributions can differ from synthetic text, especially with
  longer documents, different languages, or more repeated terms.
- The benchmark measures retrieval-only latency. It does not include embedding
  generation, Swift wrapper overhead, UI work, or full app lifecycle costs.
- Current memory reporting is payload-oriented. It is not a complete allocator
  resident-set-size measurement.
- Persistence load timing is not yet part of the benchmark report.
- Recall is measured against synthetic vectors. Real corpus quality and user
  query distributions still need a fixture-backed benchmark.

## Recommendation

Use `24K x 384d I8ScalarQuantized` as the first sub-`20 MiB` persisted package
target, with exact vector search, real BM25 state, compact chunk records, and
tombstones included.

Keep `384d F16` as the quality-preserving fallback when the product can tolerate
roughly `21.5 MiB`, or if a later compression pass can recover at least
`1.5 MiB` without complicating the format.

Do not prioritize `768d` for the first compact target. `768d I8` is close, but
it is still over budget before real metadata and platform overhead are counted.
It should wait until after realistic corpus benchmarks show the quality gain is
worth the extra size work.

## Next Implementation Step

Add a realistic fixture-backed benchmark that persists actual representative
chunk text, metadata, and BM25 state. The current synthetic report proves the
storage format can meet the target for `384d I8`, but the product decision needs
realistic corpus data before locking the compact package contract.
