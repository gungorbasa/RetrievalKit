# Isolated Memory Benchmark

VectorKit's memory benchmark runs one index scenario per process. It measures
the full lifecycle rather than treating search RSS as the index's total cost.

## What It Measures

Every report contains these sampled phases:

1. Build.
2. Cold search.
3. Warm search after excluded warm-up queries.
4. Save.
5. Unload.
6. Load.
7. Post-load warm search.
8. Tombstone deletion.
9. Compaction.

On Apple platforms, each phase records RSS before and after the operation plus
the largest RSS observed by a 1 ms sampler. Reports also include process
baseline RSS, the scenario peak and baseline delta, persisted file sizes,
cold latency, and warm P50/P95/P99 latency.

The sampler can miss allocations whose complete lifetime is shorter than its
interval. Treat the result as a close observed peak, not an allocator trace.
Use Instruments to investigate a surprising phase.

## Run From the CLI

Use a release build. Debug measurements are retained in the report and should
not be used for device budgets.

```bash
cargo run --release -p vectorkit-cli -- \
  bench memory --config benchmarks/memory/24k-384d-i8-hybrid-t25.json
```

The command prints one JSON report. It exits nonzero if execution fails or any
configured budget is exceeded. A small integration check is available at
`benchmarks/memory/smoke.json`.

## Run on iOS

Build the XCFramework, open `VectorKitIOSBench`, select a Memory preset, and run
on physical hardware. Run Memory before any other benchmark after launch. Once
any benchmark starts, the app requires a relaunch before Memory can run so
allocator state and the process high-water mark cannot contaminate the scenario.

For unattended Xcode runs, add these launch arguments:

```text
--memory-scenario 24k-384d-i8-hybrid-t25
```

The app exposes the full `24K`/`50K` × `384d`/`768d` × `F32`/`F16`/`I8` ×
vector-only/hybrid matrix at a 25% tombstone ratio. It also includes 10% and
50% compaction presets for the compact `24K × 384d I8` hybrid case.

`vector_only` selects vector queries and omits BM25 from the saved snapshot.
The current core still constructs BM25 while initially adding chunks, so its
build-phase RSS is full-index construction; its post-load phases represent the
compact vector-only snapshot.

## Scenario Configuration

Memory scenarios intentionally accept one dimension and one encoding. Arrays
are rejected because matrix execution inside one process invalidates peak RSS.

Supported budget fields are:

- `max_peak_rss_mib`
- `max_peak_delta_mib`
- `max_persisted_mib`
- `max_search_p95_ms`
- `max_compaction_peak_increase_mib`

RSS budgets fail clearly on platforms where process RSS is unavailable.
Persisted and latency budgets work everywhere. The iOS presets apply the known
10 ms target to 24K scenarios, a provisional 20 ms diagnostic target to 50K,
and the product's 20 MiB persisted gate to `24K × 384d I8`.

Peak-RSS limits are deliberately not baked into the presets yet. Establish them
from repeated release runs on supported device classes, then check those values
into scenario configs instead of choosing them from simulator or Mac results.
