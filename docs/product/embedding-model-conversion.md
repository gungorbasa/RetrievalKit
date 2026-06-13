# Embedding Model Conversion

This document records the local conversion path for producing Core ML embedding
models used by EmbeddingKit. Generated model artifacts are intentionally kept
out of the Swift package and written under `target/`.

## Scope

The first conversion target remains:

```text
BAAI/bge-small-en-v1.5
```

Reasons:

- It is the current Python/FastEmbed benchmark baseline.
- It outputs 384-dimensional vectors.
- It is small enough to be a realistic first Apple-device embedding model.
- The model card documents CLS pooling plus L2 normalization for Hugging Face
  Transformers usage.

The conversion script also supports the first recommended comparison batch:

| Preset | Model | Dim | Seq | Pooling | Notes |
|---|---|---:|---:|---|---|
| `bge-small-en-v1.5` | `BAAI/bge-small-en-v1.5` | 384 | 512 | CLS | Current baseline |
| `all-MiniLM-L6-v2` | `sentence-transformers/all-MiniLM-L6-v2` | 384 | 256 | mean | Fast tiny baseline |
| `arctic-xs` | `Snowflake/snowflake-arctic-embed-xs` | 384 | 512 | CLS | Query prefix recorded |
| `arctic-s` | `Snowflake/snowflake-arctic-embed-s` | 384 | 512 | CLS | Query prefix recorded |
| `e5-small-v2` | `intfloat/e5-small-v2` | 384 | 512 | mean | Query/passsage prefixes recorded |
| `gte-small` | `thenlper/gte-small` | 384 | 512 | mean | Compact quality candidate |
| `bge-base-en-v1.5` | `BAAI/bge-base-en-v1.5` | 768 | 512 | CLS | Higher-cost BGE quality comparison |
| `arctic-m` | `Snowflake/snowflake-arctic-embed-m` | 768 | 512 | CLS | Higher-cost Arctic quality comparison |

## Script

From the repository root:

```bash
python3.11 -m venv target/embedding-conversion-venv
source target/embedding-conversion-venv/bin/activate
python -m pip install --upgrade pip
python -m pip install torch transformers coremltools numpy
```

Use Python 3.11 or 3.12 for conversion. Python 3.14 currently installs a
Core ML Tools package without the native storage/model bindings needed to write
`.mlpackage` artifacts, which fails with errors such as `BlobWriter not loaded`.

Convert the model:

```bash
scripts/embedding/convert-embedding-coreml.py --preset bge-small-en-v1.5
```

Convert and compile with Apple's Core ML compiler:

```bash
scripts/embedding/convert-embedding-coreml.py \
  --preset bge-small-en-v1.5 \
  --compile
```

Convert and run parity checks against the traced PyTorch wrapper:

```bash
scripts/embedding/convert-embedding-coreml.py \
  --preset bge-small-en-v1.5 \
  --verify
```

List supported presets:

```bash
scripts/embedding/convert-embedding-coreml.py --list-models
```

Convert the recommended first batch:

```bash
for preset in \
  bge-small-en-v1.5 \
  all-MiniLM-L6-v2 \
  arctic-xs \
  arctic-s \
  e5-small-v2 \
  gte-small
do
  scripts/embedding/convert-embedding-coreml.py --preset "$preset" --compile --verify
done
```

Convert the quality-ceiling comparison batch:

```bash
for preset in \
  bge-base-en-v1.5 \
  arctic-m
do
  scripts/embedding/convert-embedding-coreml.py --preset "$preset" --compile --verify
done
```

The older BGE-specific command remains available as a compatibility wrapper:

```bash
scripts/embedding/convert-bge-small-coreml.py --compile --verify
```

## Performance Evaluation Plan

The first benchmark pass should compare the six generated 384-dimensional
models:

```text
bge-small-en-v1.5
all-MiniLM-L6-v2
snowflake-arctic-embed-xs
snowflake-arctic-embed-s
e5-small-v2
gte-small
```

Measure in three layers:

1. Embedding-only latency in `EmbeddingKit`.
   - Load each generated `.mlmodelc` and matching tokenizer.
   - Run the same social-network query set for every model.
   - Measure cold model load, first query, warmed single-query p50/p95/p99,
     and batch sizes `1,8,16,32,64`.
   - Repeat for Core ML compute modes `.cpuOnly`, `.cpuAndGPU`,
     `.cpuAndNeuralEngine`, and `.all`.

2. Retrieval-only latency in `VectorKit`.
   - Build or reuse one index per model, because model output vectors are not
     interchangeable even when dimensions match.
   - Keep `top_k`, chunk count, filters, vector encoding, and query texts
     identical across models.
   - Report exact vector search p50/p95/p99 separately from embedding latency.

3. End-to-end latency and result sanity.
   - Measure query embedding plus exact vector search with the same 750 measured
     queries used by the existing social-network benchmark.
   - Save a small result sample per model for qualitative inspection.
   - Select the default model only after latency and result quality are both
     acceptable.

Suggested output table:

| Model | Compute | Init ms | First query ms | Embed p95 ms | Search p95 ms | Total p95 ms | Batch 32 texts/s |
|---|---|---:|---:|---:|---:|---:|---:|

## Outputs

Default output directory pattern:

```text
target/embedding-models/<preset-slug>/
```

Expected files per preset:

```text
<ModelName>.mlpackage
<ModelName>.mlmodelc        # only when --compile is passed
metadata.json
tokenizer/
```

The `metadata.json` sidecar records:

```json
{
  "model": "BAAI/bge-small-en-v1.5",
  "dimension": 384,
  "sequence_length": 512,
  "inputs": ["input_ids", "attention_mask", "token_type_ids"],
  "output": "embedding",
  "pooling": "cls",
  "normalized": true,
  "query_prefix": "",
  "passage_prefix": "",
  "token_input_shape": [1, "sequence_length"]
}
```

## EmbeddingKit Contract

The generated model is expected to match EmbeddingKit's Core ML provider:

| Purpose | Feature |
|---|---|
| Token IDs | `input_ids` |
| Attention mask | `attention_mask` |
| Token type IDs | `token_type_ids` |
| Pooled embedding | `embedding` |

Token inputs use shape:

```text
[1, sequence_length]
```

The output must be a numeric `MLMultiArray` with the preset dimension. The
converter wraps the Hugging Face transformer model with:

1. CLS or masked mean pooling, depending on the preset.
2. L2 normalization.

## Notes

- Do not commit generated `.mlpackage` or `.mlmodelc` artifacts unless release
  packaging explicitly decides to vendor a model.
- Keep tokenizer assets beside the generated model so Swift-side tokenizer work
  can consume the same vocabulary/config.
- Benchmark compute modes and `backendPoolSize` after conversion; do not assume
  `.cpuAndNeuralEngine` is fastest for every exported model.
- For `e5-small-v2`, callers should apply the recorded `query_prefix` and
  `passage_prefix` consistently when generating query and document embeddings.
- The 768-dimensional presets are quality-ceiling comparisons. They increase
  VectorKit storage, memory, and exact-search cost, so benchmark them separately
  from the small-model default candidates.

## References

- Core ML Tools PyTorch conversion workflow:
  https://apple.github.io/coremltools/docs-guides/source/convert-pytorch-workflow.html
- Core ML Tools model prediction notes:
  https://apple.github.io/coremltools/docs-guides/source/model-prediction.html
- `BAAI/bge-small-en-v1.5` model card:
  https://huggingface.co/BAAI/bge-small-en-v1.5
- `sentence-transformers/all-MiniLM-L6-v2` model card:
  https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2
- `intfloat/e5-small-v2` model card:
  https://huggingface.co/intfloat/e5-small-v2
- `thenlper/gte-small` model card:
  https://huggingface.co/thenlper/gte-small
- `Snowflake/snowflake-arctic-embed-xs` model card:
  https://huggingface.co/Snowflake/snowflake-arctic-embed-xs
- `BAAI/bge-base-en-v1.5` model card:
  https://huggingface.co/BAAI/bge-base-en-v1.5
- `Snowflake/snowflake-arctic-embed-m` model card:
  https://huggingface.co/Snowflake/snowflake-arctic-embed-m
