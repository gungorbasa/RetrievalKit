# Apple End-to-End Benchmark

This directory contains the compact descriptors for the graph-free Apple
text-to-result benchmark defined by
`docs/product/apple-end-to-end-benchmark-contract-v1.md`.

The completed Mac baseline remains V1. Physical-iPhone collection uses the
USB-powered V2 amendment in
`docs/product/apple-end-to-end-benchmark-contract-v2.md`; V2 changes only the
power/control condition, uses new iPhone workload IDs, and must be labeled
USB-powered.

Current status: contract and executable harness implemented. A performance
claim is valid only for a platform matrix whose raw reports pass the independent
validator with three valid fresh-process sessions per configuration.

- `workloads-v1.json` freezes the Apple model profiles, corpus sizes,
  classifications, and 100K restrictions.
- `protocol-v1.json` freezes the query population, search-plus-embedding timing
  boundary, sample policy, device validity rules, Q8 prerequisite, and iPhone
  100K preflight.
- `workloads-v2.json` and `protocol-v2.json` define the USB-powered iPhone
  amendment while inheriting unchanged V1 corpus/model/search methodology.
- `generate_inputs.py` writes deterministic graph-free corpus JSONL, the frozen
  100-query suite, its 750-query schedule, and source hashes.
- `prepare_models.py` downloads the immutable FP32 and Q8 Core ML artifacts,
  verifies their bytes and hashes, records the known Q8 manifest mismatch, and
  compiles both packages outside query timing.
- `validate_results.py` independently recomputes summaries from all retained
  raw samples and rejects invalid timing, classification, process, or iPhone
  evidence.
- `run_mac_matrix.py` and `run_iphone_matrix.py` execute the frozen matrix one
  fresh process at a time; the iPhone collector also retrieves every report
  from the app container before validation.
- `summarize_results.py` renders the contract headline: the median of three
  fresh-session P95 values, separately for embedding, retrieval, and total.

The shared public-API runner is the Swift package at
`wrappers/swift/RetrievalKitAppleE2EBench`. The physical-device app is generated
from `wrappers/swift/RetrievalKitAppleE2EIOSBench/project.yml` with XcodeGen.

## Hybrid stage profiling

The Rust-core diagnostic benchmark isolates filter planning, I8 vector
candidate generation, BM25 candidate generation, fusion, hydration, and total
hybrid retrieval at 25K and 49,999 chunks. It reuses this benchmark family's
realistic text shape, runs unfiltered and indexed-filter scenarios, and keeps
instrumentation behind an off-by-default feature:

```bash
RETRIEVALKIT_PROFILE_WARMUP=50 \
RETRIEVALKIT_PROFILE_SAMPLES=300 \
cargo bench -q -p retrievalkit-core \
  --features benchmark-instrumentation \
  --bench hybrid_stage_profile
```

This diagnostic excludes embedding and does not replace the independently
validated public Swift/Core ML or physical-iPhone matrices. See
`docs/product/reports/hybrid-performance-milestone-v1-report.md` for the first
before/after result.

## Preparation

Use Xcode's Swift toolchain; the standalone Command Line Tools Swift can be
older than this repository's Swift tools version.

```bash
python3 benchmarks/apple-end-to-end/prepare_models.py \
  --output target/apple-end-to-end/models-v1

python3 benchmarks/apple-end-to-end/generate_inputs.py \
  --output target/apple-end-to-end/source-10k-a --active-records 2500
python3 benchmarks/apple-end-to-end/generate_inputs.py \
  --output target/apple-end-to-end/source-10k-b --active-records 2500
cmp target/apple-end-to-end/source-10k-a/corpus.jsonl \
  target/apple-end-to-end/source-10k-b/corpus.jsonl
cmp target/apple-end-to-end/source-10k-a/queries.json \
  target/apple-end-to-end/source-10k-b/queries.json
```

Repeat source generation with 12,500 and 25,000 records for the 50K and 100K
workloads. The `prepare-index` command embeds and builds only on Mac. Supply the
compiled model and matching tokenizer for the selected profile:

```bash
xcrun swift run -c release \
  --package-path wrappers/swift/RetrievalKitAppleE2EBench \
  retrievalkit-apple-e2e prepare-index \
  --corpus target/apple-end-to-end/source-10k-a/corpus.jsonl \
  --output-index target/apple-end-to-end/indexes/coreml-fp32-production-v1/10k \
  --model target/apple-end-to-end/models-v1/coreml-fp32-production-v1/compiled/all-MiniLM-L6-v2-fp32.mlmodelc \
  --tokenizer target/apple-end-to-end/models-v1/coreml-fp32-production-v1/extracted/tokenizer/tokenizer.json \
  --expected-chunks 10000
```

## Mac session

Each invocation runs one mode in one fresh process. Change `--session-id` and
repeat three times for both `vector` and `weighted_hybrid`.

```bash
xcrun swift run -c release \
  --package-path wrappers/swift/RetrievalKitAppleE2EBench \
  retrievalkit-apple-e2e run \
  --queries target/apple-end-to-end/source-10k-a/queries.json \
  --index target/apple-end-to-end/indexes/coreml-fp32-production-v1/10k \
  --model target/apple-end-to-end/models-v1/coreml-fp32-production-v1/compiled/all-MiniLM-L6-v2-fp32.mlmodelc \
  --tokenizer target/apple-end-to-end/models-v1/coreml-fp32-production-v1/extracted/tokenizer/tokenizer.json \
  --output target/apple-end-to-end/results/mac/fp32/10k/vector/session-1.json \
  --workload-id apple-e2e-10k-384d-i8-v1 \
  --workload-classification supported_product \
  --profile-id coreml-fp32-production-v1 \
  --profile-classification production_control \
  --session-id mac-fp32-10k-vector-1 \
  --mode vector \
  --retrievalkit-revision "$(git rev-parse HEAD)"
```

Validate reports without trusting Swift's summaries:

```bash
python3 benchmarks/apple-end-to-end/validate_results.py \
  --queries target/apple-end-to-end/source-10k-a/queries.json \
  --require-complete-sessions \
  target/apple-end-to-end/results/mac/fp32/10k/vector/*.json
```

## Physical iPhone

Build the local base XCFramework, generate the app project, then use a Release
device build. Assets are copied to the app data container with `devicectl`; the
app expects one asset root containing `queries.json`, `tokenizer.json`, the
verified source `model.mlpackage/`, and `index/`. Model compilation happens on
device before the timed query loop. Launch arguments identify the profile,
workload, mode, and session. The app refuses simulator, attached debugger,
available networking, charging, invalid battery/thermal state, backgrounding,
or memory warnings. The 100K lane additionally checks the frozen free-storage
rule before loading the database.

```bash
scripts/build-xcframework.sh
cd wrappers/swift/RetrievalKitAppleE2EIOSBench
xcodegen generate --spec project.yml
xcodebuild -project RetrievalKitAppleE2EIOSBench.xcodeproj \
  -scheme RetrievalKitAppleE2EIOSBench -configuration Release \
  -destination 'generic/platform=iOS' build

python3 benchmarks/apple-end-to-end/run_iphone_matrix.py \
  --device '<CoreDevice UUID or UDID>' \
  --attempt-id usb-powered-observational-v2 \
  --retrievalkit-revision 'HEAD+binary-sha256:<XCFramework binary SHA-256>'
```

If a start is rejected for a non-nominal thermal state, stop and cool the
device. Resume the same attempt without rerunning already retrieved reports:

```bash
python3 benchmarks/apple-end-to-end/run_iphone_matrix.py \
  --device '<CoreDevice UUID or UDID>' \
  --attempt-id usb-powered-observational-v2 \
  --retrievalkit-revision 'HEAD+binary-sha256:<XCFramework binary SHA-256>' \
  --resume --inter-session-cooldown-seconds 30
```

Generated corpora, model copies, compiled models, databases, raw measurements,
and reports must remain under ignored `target/` roots.
