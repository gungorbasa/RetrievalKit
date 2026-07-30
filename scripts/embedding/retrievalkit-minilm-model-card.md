---
license: apache-2.0
pipeline_tag: feature-extraction
library_name: onnxruntime
tags:
  - sentence-transformers
  - onnx
  - coreml
  - embeddings
  - retrieval
---

# RetrievalKit all-MiniLM-L6-v2 artifacts

This repository contains deterministic runtime exports used by RetrievalKit's
optional embedding experiment. They are derived from
[`sentence-transformers/all-MiniLM-L6-v2`](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
at source revision
`c9745ed1d9f207416be6d2e6f8de32d1f16199bf`.

## Shared output contract

- BERT WordPiece tokenization
- maximum input length of 256 tokens
- masked mean pooling
- L2 normalization
- 384-dimensional F32 output
- cosine similarity

The ONNX models have dynamic batch and sequence axes. Callers must enforce the
256-token limit and pad batches only as needed.

## Profiles

| Profile | ONNX | Core ML |
|---|---|---|
| FP32 | FP32 weights | FP32 ML Program |
| FP16 | FP16 weights with F32 I/O | FP16 ML Program |
| Q8 | dynamic signed-INT8 weights/activations | weight-only signed-INT8 with FP16 compute |

All profiles return the same normalized F32 embedding contract, but different
runtimes and precision profiles are not expected to be byte-identical.
Applications should use the same profile for document and query embeddings
unless a cross-profile combination has been separately qualified.

`manifest-v1.json` is the authoritative artifact inventory. It records the
source revision, tokenizer files, input/output contract, sizes, quantization
schemes, and SHA-256 digests. RetrievalKit clients pin an immutable repository
revision rather than downloading from `main`.

These artifacts are experimental. No latency or retrieval-quality claim should
be inferred from their presence; measured qualification is published
separately in the RetrievalKit repository.

## License

The source model declares Apache-2.0. RetrievalKit's conversion tooling and
packaging are also Apache-2.0. See `LICENSE`, `NOTICE`, the upstream model card,
and `manifest-v1.json` for provenance.
