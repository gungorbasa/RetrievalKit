# Size and Speed Report

Date: 2026-06-06

## Goal

Keep roughly `24K` vectors plus required local retrieval/display data under a
`20 MiB` persisted footprint while keeping retrieval fast on local devices. RAM
usage is also important, so the same compact layout should be friendly to
in-memory search.

This report uses `MiB` (`1024 * 1024` bytes). A strict decimal `20 MB` budget is
about `19.07 MiB`, so it is tighter than the numbers below by about `0.93 MiB`.

## Method

Benchmarks were run with:

```bash
cargo run --release -p vectorkit-cli -- bench matrix \
  --chunks 24000 \
  --dimensions 384,768 \
  --queries 100 \
  --top-k 5,10 \
  --encodings f32,f16,i8 \
  --budget-mb 20 \
  --avg-chunk-data-bytes 256 \
  --avg-metadata-bytes 32 \
  --avg-bm25-terms 24
```

The CLI currently reports an estimate, not actual disk files, because index
persistence is not implemented yet. The estimate is intentionally explicit:

```text
vector bytes:
  F32 = chunks * dim * 4
  F16/BF16 = chunks * dim * 2
  I8 = chunks * dim + chunks * 4 scale bytes

auxiliary bytes:
  fixed chunk record = chunks * 64
  chunk data = chunks * avg_chunk_data_bytes
  metadata = chunks * avg_metadata_bytes
  BM25 = chunks * avg_bm25_terms_per_chunk * 8
  file/header overhead = 4096
```

The default compact-data estimate is:

```text
avg chunk data bytes: 256
avg metadata bytes: 32
avg BM25 terms per chunk: 24
```

## Baseline Results

Baseline assumes vectors plus compact chunk data, metadata, and BM25 postings.

| chunks | dim | encoding | vector MiB | aux MiB | est total MiB | headroom vs 20 MiB |
|---:|---:|:---|---:|---:|---:|---:|
| 24K | 384 | F32 | 35.156 | 12.455 | 47.611 | -27.611 |
| 24K | 384 | F16 | 17.578 | 12.455 | 30.033 | -10.033 |
| 24K | 384 | I8 | 8.881 | 12.455 | 21.336 | -1.336 |
| 24K | 768 | F32 | 70.312 | 12.455 | 82.768 | -62.768 |
| 24K | 768 | F16 | 35.156 | 12.455 | 47.611 | -27.611 |
| 24K | 768 | I8 | 17.670 | 12.455 | 30.125 | -10.125 |

Conclusion: with BM25 and `256 B` average chunk data, no current encoding fits
the full `24K` target under `20 MiB`. `384d I8` is close, missing by about
`1.34 MiB`. `768d I8` is not close enough.

## Speed and Recall

Same baseline run, release build, synthetic vectors, cosine search, `100`
queries.

| dim | top_k | encoding | avg ms | p95 ms | recall@k vs F32 |
|---:|---:|:---|---:|---:|---:|
| 384 | 5 | F32 | 3.594 | 3.696 | 1.0000 |
| 384 | 5 | F16 | 3.771 | 4.129 | 1.0000 |
| 384 | 5 | I8 | 3.890 | 4.012 | 0.9900 |
| 384 | 10 | F32 | 3.785 | 3.902 | 1.0000 |
| 384 | 10 | F16 | 3.860 | 3.983 | 0.9990 |
| 384 | 10 | I8 | 4.625 | 4.775 | 0.9940 |
| 768 | 5 | F32 | 6.575 | 6.738 | 1.0000 |
| 768 | 5 | F16 | 6.671 | 6.781 | 1.0000 |
| 768 | 5 | I8 | 7.029 | 7.176 | 0.9900 |
| 768 | 10 | F32 | 6.794 | 6.956 | 1.0000 |
| 768 | 10 | F16 | 6.863 | 6.975 | 1.0000 |
| 768 | 10 | I8 | 7.699 | 7.822 | 0.9940 |

Conclusion: `24K x 384d I8` is fast enough in the current exact full-scan
benchmark. `24K x 768d I8` is also likely fast enough, but it misses the size
budget with current storage assumptions.

## Footprint Sensitivity

### I8 Without BM25 In The Compact Package

Assumptions:

```text
avg chunk data bytes: 256
avg metadata bytes: 32
avg BM25 terms per chunk: 0
```

| chunks | dim | encoding | vector MiB | aux MiB | est total MiB | headroom vs 20 MiB |
|---:|---:|:---|---:|---:|---:|---:|
| 24K | 384 | I8 | 8.881 | 8.061 | 16.941 | 3.059 |
| 24K | 768 | I8 | 17.670 | 8.061 | 25.730 | -5.730 |

Conclusion: `384d I8` fits if BM25 is omitted from the compact package.
`768d I8` still does not fit.

### I8 With Smaller Chunk Data And BM25

Assumptions:

```text
avg chunk data bytes: 128
avg metadata bytes: 32
avg BM25 terms per chunk: 24
```

| chunks | dim | encoding | vector MiB | aux MiB | est total MiB | headroom vs 20 MiB |
|---:|---:|:---|---:|---:|---:|---:|
| 24K | 384 | I8 | 8.881 | 9.525 | 18.406 | 1.594 |
| 24K | 768 | I8 | 17.670 | 9.525 | 27.195 | -7.195 |

Conclusion: `384d I8` fits if chunk data averages around `128 B` even with BM25.
`768d I8` still does not fit.

### Minimum Current I8 Shape

Assumptions:

```text
avg chunk data bytes: 128
avg metadata bytes: 16
avg BM25 terms per chunk: 0
```

| chunks | dim | encoding | vector MiB | aux MiB | est total MiB | headroom vs 20 MiB |
|---:|---:|:---|---:|---:|---:|---:|
| 24K | 384 | I8 | 8.881 | 4.765 | 13.645 | 6.355 |
| 24K | 768 | I8 | 17.670 | 4.765 | 22.434 | -2.434 |

Conclusion: even a very compact `768d I8` package misses `20 MiB`. To make
`768d + data` fit, VectorKit likely needs `BinaryQuantized`, fewer chunks, lower
dimension vectors, or a much smaller definition of required data.

## Recommendation

For the current `24K + data < 20 MiB` target:

1. Use `384d` embeddings with `I8ScalarQuantized` for the first compact target.
2. Keep full chunk text outside the hot compact index. Store only compact
   display/retrieval data in the package.
3. Treat BM25 as optional for the sub-20 MiB target unless its postings can be
   compacted enough to keep total size below budget.
4. Add persistence with file-size reporting as soon as practical, because the
   estimator should be replaced by actual saved file sizes.
5. Explore `BinaryQuantized` only if `768d + required data < 20 MiB` becomes a
   hard requirement.

## Next Implementation Step

Add persistence-size instrumentation around the eventual saved index format:

```text
vectors file bytes
chunk metadata/data file bytes
BM25 file bytes
tombstone/version bytes
total package bytes
loaded RAM bytes
```

Until persistence exists, the CLI footprint estimator should remain in the
benchmark output and be updated whenever the storage model changes.
