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
- `TextTokenizer`: tokenizer boundary for model-backed providers.
- `BertWordPieceTokenizer`: Hugging Face `BertTokenizer`/WordPiece loader for
  converted Core ML embedding models.
- `CoreMLEmbedder`: Core ML provider behind `canImport(CoreML)`.
- `EmbeddingBenchmark`: shared benchmark runner for single-query and batch
  latency measurement.

## Hardware Acceleration Direction

The production Apple path should add a Core ML-backed provider behind the same
protocol:

```swift
let embedder = CoreMLEmbedder(
    modelInfo: KnownEmbeddingModels.bgeSmallEnV15,
    tokenizer: tokenizer,
    configuration: CoreMLModelConfiguration(
        modelURL: modelURL,
        compute: .cpuAndNeuralEngine,
        backendPoolSize: 1
    )
)
```

For the converted Hugging Face BERT-family models, load the tokenizer from the
generated model directory and pass the same fixed sequence length used during
Core ML conversion:

```swift
let tokenizer = try BertWordPieceTokenizer(
    tokenizerDirectory: modelDirectory.appendingPathComponent("tokenizer"),
    sequenceLength: 512
)
```

Benchmark reports already include requested and actual compute fields so we can
compare:

- `.cpuOnly`
- `.cpuAndGPU`
- `.cpuAndNeuralEngine`
- `.all`

The Core ML provider expects a compiled model whose input/output boundary is:

- tokenizer produces `inputIDs`, `attentionMask`, and optional `tokenTypeIDs`
- token arrays are converted to `MLMultiArray` inputs, defaulting to
  `[1, sequenceLength]`
- model output exposes one pooled embedding vector
- embedding length must equal `EmbeddingModelInfo.dimension`
- unsupported model input/output shapes surface as `unsupportedModelInterface`

Default Core ML feature names:

| Purpose | Feature |
|---|---|
| Token IDs | `input_ids` |
| Attention mask | `attention_mask` |
| Token type IDs | `token_type_ids` |
| Pooled embedding | `embedding` |

`CoreMLModelConfiguration.tokenInputShape` can be changed to `.sequence` for
models that expect one-dimensional token arrays.

`CoreMLModelConfiguration.backendPoolSize` controls how many Core ML model
backend actors are loaded. The default is `1`. Larger values can improve batch
throughput while keeping each `MLModel` isolated to its own actor; benchmark
`1`, `2`, and `4` before choosing a production default.

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

## Benchmark CLI

The package includes `embeddingkit-bench` to validate report generation before
real model providers are added. The current executable uses deterministic
precomputed embeddings, so it measures harness overhead rather than neural
network inference.

Run the default Social Network query fixture:

```bash
cd wrappers/swift/EmbeddingKit
swift run embeddingkit-bench \
  --models bge-small-en-v1.5,all-MiniLM-L6-v2 \
  --warmup 50 \
  --measured 750 \
  --batch-sizes 1,8,16,32,64
```

Write JSON for later comparison:

```bash
swift run embeddingkit-bench \
  --format json \
  --output embedding-benchmark.json
```

Use a custom query file:

```bash
swift run embeddingkit-bench --queries-file queries.json
```

Supported query JSON shapes:

```json
["query one", "query two"]
```

```json
{"queries": ["query one", "query two"]}
```

```json
{"query": "single query"}
```

## Build And Test

```bash
cd wrappers/swift/EmbeddingKit
swift test
```
