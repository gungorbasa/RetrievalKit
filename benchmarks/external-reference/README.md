# Phase 5 External Reference Implementations

This directory contains benchmark-only adapters for the retrieval benchmark
roadmap's Phase 5. Nothing here is linked into VectorKit production crates or
public wrappers, and nothing executes on a physical device.

The normative methodology is
[`docs/product/external-reference-implementations-contract-v1.md`](../../docs/product/external-reference-implementations-contract-v1.md).

## Systems

- VectorKit F32 exact and VectorKit graph-scoped retrieval from the checked-out
  source revision.
- NumPy `2.5.1` as the independent exact-result oracle. Oracle latency is not a
  comparative result.
- sqlite-vec `0.1.9` as the embedded brute-force exact engine.
- USearch `2.26.0` HNSW with frozen F32/connectivity/expansion settings and a
  mandatory mean Recall@10 gate of `0.99`.
- SQLite plus sqlite-vec scalar distance, FTS5, explicit adjacency tables,
  metadata joins, and application-side fusion as the custom graph application.

The exact feature mapping is in `feature-parity-v1.json`. In particular,
USearch predicate filtering is unsupported in the selected Python binding, and
the custom application's FTS5/hybrid semantics are non-equivalent to
VectorKit's tokenizer, BM25, normalization, and fusion.

## Supported build environment

Canonical local measurements use Apple Silicon macOS and CPython `3.12.13`.
The pinned external packages also publish wheels for several Linux and Windows
targets, but output from another platform is a separate environment and cannot
be pooled with the checked Mac result.

Required host tools:

- `uv 0.9.27`
- Rust/Cargo sufficient to build this checkout; the exact versions are emitted
  in `environment.json`
- maturin `1.14.1`, invoked through `uv tool run`

Create the isolated environment and build both local VectorKit wheels:

```bash
scripts/benchmarks/setup-phase5-external.sh
```

The script creates only ignored paths under `target/`, installs the hash-locked
external requirements, builds the graph-free and graph aggregate wheels in
release mode, and installs both distributions. Each adapter still imports only
one native distribution per subprocess.

## Run

Run the compact smoke profile:

```bash
scripts/benchmarks/run-phase5-external.sh smoke
```

Run the frozen development split:

```bash
scripts/benchmarks/run-phase5-external.sh development
```

Run the frozen 10K/25K/50K local comparison:

```bash
scripts/benchmarks/run-phase5-external.sh comparison
```

The comparison command does not install, launch, or communicate with an iPhone.
It runs macOS subprocesses only. The 50K row is boundary benchmark evidence and
does not change the supported fewer-than-50K V1 statement.

Direct invocation is also supported:

```bash
target/phase5-external-venv/bin/python \
  benchmarks/external-reference/run_phase5.py \
  --config benchmarks/external-reference/configs/smoke-v1.json \
  --output target/benchmarks/phase5/smoke-v1 \
  --python target/phase5-external-venv/bin/python
```

## Validate

```bash
scripts/benchmarks/validate-phase5-external.sh \
  target/benchmarks/phase5/smoke-v1
```

The standalone validator does not import the runner, adapter worker, or shared
benchmark helpers. It enforces the closed ten-file inventory, canonical JSON,
hashes, generator replay, result identities, exact equality, ANN recall,
percentiles, persistence accounting, unsupported operations, and prohibited
device/capacity interpretations.

## Tests

Unit tests need NumPy only:

```bash
target/phase5-external-venv/bin/python -m unittest discover \
  -s benchmarks/external-reference/tests -p 'test_common.py'
```

The integration suite executes two complete smoke roots, validates both,
compares deterministic projections, and performs mutation tests:

```bash
PHASE5_RUN_INTEGRATION=1 \
  target/phase5-external-venv/bin/python -m unittest discover \
  -s benchmarks/external-reference/tests
```

## Artifact layout

Every result root contains exactly:

```text
config.json
environment.json
feature-parity.json
input-manifests.json
raw-measurements.jsonl
raw-results.jsonl
failures.jsonl
summary.json
checksums.json
manifest.json
```

Unsupported operations and failed attempts remain in `failures.jsonl`.
Generated vectors and indexes are reproducible scratch inputs and are not part
of the published result root. Their deterministic identities are recorded in
`input-manifests.json`.

## Licenses and upstream references

- sqlite-vec `0.1.9`: MIT OR Apache-2.0,
  <https://github.com/asg017/sqlite-vec/tree/v0.1.9>
- USearch `2.26.0`: Apache-2.0,
  <https://github.com/unum-cloud/usearch/tree/v2.26.0>
- NumPy `2.5.1`: BSD-3-Clause plus compatible bundled licenses,
  <https://github.com/numpy/numpy/tree/v2.5.1>
- SQLite: public domain, <https://www.sqlite.org/copyright.html>

No upstream source is vendored. Exact versions and distribution hashes are in
`requirements.lock.txt`.

