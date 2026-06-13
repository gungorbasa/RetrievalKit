# Embedding Model Conversion

This document records the local conversion path for producing Core ML embedding
models used by EmbeddingKit. Generated model artifacts are intentionally kept
out of the Swift package and written under `target/`.

## Scope

The first conversion target is:

```text
BAAI/bge-small-en-v1.5
```

Reasons:

- It is the current Python/FastEmbed benchmark baseline.
- It outputs 384-dimensional vectors.
- It is small enough to be a realistic first Apple-device embedding model.
- The model card documents CLS pooling plus L2 normalization for Hugging Face
  Transformers usage.

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
scripts/embedding/convert-bge-small-coreml.py
```

Convert and compile with Apple's Core ML compiler:

```bash
scripts/embedding/convert-bge-small-coreml.py --compile
```

Convert and run parity checks against the traced PyTorch wrapper:

```bash
scripts/embedding/convert-bge-small-coreml.py --verify
```

## Outputs

Default output directory:

```text
target/embedding-models/bge-small-en-v1.5/
```

Expected files:

```text
BGESmallEnV15.mlpackage
BGESmallEnV15.mlmodelc        # only when --compile is passed
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

The output must be a numeric `MLMultiArray` with 384 values. The converter wraps
the Hugging Face transformer model with:

1. CLS pooling from the first token.
2. L2 normalization.

## Notes

- Do not commit generated `.mlpackage` or `.mlmodelc` artifacts unless release
  packaging explicitly decides to vendor a model.
- Keep tokenizer assets beside the generated model so Swift-side tokenizer work
  can consume the same vocabulary/config.
- Benchmark compute modes and `backendPoolSize` after conversion; do not assume
  `.cpuAndNeuralEngine` is fastest for every exported model.

## References

- Core ML Tools PyTorch conversion workflow:
  https://apple.github.io/coremltools/docs-guides/source/convert-pytorch-workflow.html
- Core ML Tools model prediction notes:
  https://apple.github.io/coremltools/docs-guides/source/model-prediction.html
- `BAAI/bge-small-en-v1.5` model card:
  https://huggingface.co/BAAI/bge-small-en-v1.5
