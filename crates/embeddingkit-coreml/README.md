# EmbeddingKit Core ML

Apple-only Rust embedding helper for running Core ML text embedding models and
passing the resulting `Vec<f32>` to VectorKit.

This crate intentionally does not live inside `vectorkit-core`. VectorKit stays
retrieval-only; embedding model execution remains a separate layer.

## MiniLM Smoke Benchmark

Generate or place the MiniLM Core ML assets at:

```text
target/embedding-models/all-MiniLM-L6-v2/
  AllMiniLML6V2.mlmodelc
  tokenizer/tokenizer.json
```

Then run:

```bash
cargo run -p embeddingkit-coreml --release --example minilm_smoke
```

To compare Core ML compute modes:

```bash
cargo run -p embeddingkit-coreml --release --example minilm_smoke -- all
cargo run -p embeddingkit-coreml --release --example minilm_smoke -- cpuAndNeuralEngine
cargo run -p embeddingkit-coreml --release --example minilm_smoke -- cpuAndGPU
cargo run -p embeddingkit-coreml --release --example minilm_smoke -- cpu
```

The example:

- loads the compiled Core ML model with `ComputeUnits::All`
- loads the Hugging Face tokenizer JSON
- tokenizes to fixed `seq=256`
- sends `input_ids`, `attention_mask`, and `token_type_ids` as `Int32`
- reads the `embedding` output as `f32`
- reports first-query and warmed single-query latency

## Usage

```rust
use embeddingkit_coreml::{CoreMlEmbeddingConfig, CoreMlTextEmbedder};

let embedder = CoreMlTextEmbedder::load(CoreMlEmbeddingConfig::new(
    "AllMiniLML6V2.mlmodelc",
    "tokenizer/tokenizer.json",
))?;

let embedding = embedder.embed("Mark and Erica arguing at the party")?;
```

Pass `embedding` directly into VectorKit search.

## Streaming CLI

Build the JSONL embedder used by Python fixture builders:

```bash
cargo build -p embeddingkit-coreml --release --bin embeddingkit-coreml-embed
```

Run it directly:

```bash
printf '{"text":"Mark and Erica arguing at the party"}\n' | \
  target/release/embeddingkit-coreml-embed \
    --model-dir target/embedding-models/all-MiniLM-L6-v2 \
    --compute cpuAndNeuralEngine
```

The command reads one JSON object per line from stdin:

```json
{"text":"..."}
```

and writes one JSON object per line to stdout:

```json
{"embedding":[...]}
```
