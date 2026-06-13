# EmbeddingKit Swift

EmbeddingKit is a provider-neutral embedding layer intended to pair with
VectorKit without making VectorKit depend on an embedding model.

VectorKit keeps the retrieval boundary explicit:

```swift
let hits = try await index.search(embedding: queryEmbedding, topK: 5)
```

EmbeddingKit provides the text-to-vector step:

```swift
import EmbeddingKit
import VectorKit

let embedder: any TextEmbedder = try PrecomputedEmbedder(
    modelInfo: KnownEmbeddingModels.bgeSmallEnV15,
    embeddings: ["Mark and Erica arguing": Array(repeating: 0, count: 384)]
)

let embedding = try await embedder.embed("Mark and Erica arguing")
let hits = try await index.search(embedding: embedding, topK: 5)
```

## Current Scope

- `TextEmbedder`: async, `Sendable` text embedding protocol.
- `EmbeddingModelInfo`: model identity, revision, dimension, token limit, and
  recommended similarity metric.
- `EmbeddingRuntimeInfo`: runtime and requested/actual compute metadata.
- `PrecomputedEmbedder`: deterministic provider for fixtures and tests.
- `EmbeddingBenchmark`: shared benchmark runner for single-query and batch
  latency measurement.

## Hardware Acceleration Direction

The production Apple path should add a Core ML-backed provider behind the same
protocol:

```swift
let embedder = try await CoreMLEmbedder(
    model: ...,
    compute: .cpuAndNeuralEngine
)
```

Benchmark reports already include requested and actual compute fields so we can
compare:

- `.cpuOnly`
- `.cpuAndGPU`
- `.cpuAndNeuralEngine`
- `.all`

Core ML and model conversion stay out of this initial package pass so the public
API can settle before adding runtime dependencies.

## Benchmark Shape

Use the same shape as VectorKit retrieval benchmarks:

- cold model initialization reported separately by concrete providers
- warmup iterations excluded
- measured single-query latency
- measured batch latency and throughput
- model, dimension, runtime, and compute mode recorded in every report

Target report table:

| Model | Runtime | Compute | Dim | Batch | Init | P50 | P95 | P99 | Mean | Throughput |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|

## Build And Test

```bash
cd wrappers/swift/EmbeddingKit
swift test
```
