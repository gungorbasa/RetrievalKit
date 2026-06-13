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
| `jina-small-en` | `jinaai/jina-embeddings-v2-small-en` | 512 | 512 | mean | Requires Hugging Face remote code |

## Script

From the repository root:

```bash
python3 -m venv target/embedding-conversion-venv
source target/embedding-conversion-venv/bin/activate
python -m pip install --upgrade pip
python -m pip install torch transformers coremltools numpy
```

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
  gte-small \
  jina-small-en
do
  scripts/embedding/convert-embedding-coreml.py --preset "$preset" --compile --verify
done
```

The older BGE-specific command remains available as a compatibility wrapper:

```bash
scripts/embedding/convert-bge-small-coreml.py --compile --verify
```

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
- `jina-small-en` uses `trust_remote_code=True` during conversion and should be
  reviewed separately before adopting as a default app model.

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
- `jinaai/jina-embeddings-v2-small-en` model card:
  https://huggingface.co/jinaai/jina-embeddings-v2-small-en
