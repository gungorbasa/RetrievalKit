#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Literal


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT_ROOT = ROOT_DIR / "target" / "embedding-models"
INPUT_IDS = "input_ids"
ATTENTION_MASK = "attention_mask"
TOKEN_TYPE_IDS = "token_type_ids"
OUTPUT_EMBEDDING = "embedding"
Pooling = Literal["cls", "mean"]


@dataclass(frozen=True)
class ModelPreset:
    aliases: tuple[str, ...]
    model_id: str
    slug: str
    package_name: str
    dimension: int
    sequence_length: int
    pooling: Pooling
    normalized: bool = True
    query_prefix: str = ""
    passage_prefix: str = ""
    trust_remote_code: bool = False


MODEL_PRESETS: tuple[ModelPreset, ...] = (
    ModelPreset(
        aliases=("bge-small-en-v1.5", "BAAI/bge-small-en-v1.5"),
        model_id="BAAI/bge-small-en-v1.5",
        slug="bge-small-en-v1.5",
        package_name="BGESmallEnV15.mlpackage",
        dimension=384,
        sequence_length=512,
        pooling="cls",
    ),
    ModelPreset(
        aliases=("all-MiniLM-L6-v2", "sentence-transformers/all-MiniLM-L6-v2"),
        model_id="sentence-transformers/all-MiniLM-L6-v2",
        slug="all-MiniLM-L6-v2",
        package_name="AllMiniLML6V2.mlpackage",
        dimension=384,
        sequence_length=256,
        pooling="mean",
    ),
    ModelPreset(
        aliases=("arctic-xs", "snowflake-arctic-embed-xs", "Snowflake/snowflake-arctic-embed-xs"),
        model_id="Snowflake/snowflake-arctic-embed-xs",
        slug="snowflake-arctic-embed-xs",
        package_name="SnowflakeArcticEmbedXS.mlpackage",
        dimension=384,
        sequence_length=512,
        pooling="cls",
        query_prefix="Represent this sentence for searching relevant passages: ",
    ),
    ModelPreset(
        aliases=("arctic-s", "snowflake-arctic-embed-s", "Snowflake/snowflake-arctic-embed-s"),
        model_id="Snowflake/snowflake-arctic-embed-s",
        slug="snowflake-arctic-embed-s",
        package_name="SnowflakeArcticEmbedS.mlpackage",
        dimension=384,
        sequence_length=512,
        pooling="cls",
        query_prefix="Represent this sentence for searching relevant passages: ",
    ),
    ModelPreset(
        aliases=("e5-small-v2", "intfloat/e5-small-v2"),
        model_id="intfloat/e5-small-v2",
        slug="e5-small-v2",
        package_name="E5SmallV2.mlpackage",
        dimension=384,
        sequence_length=512,
        pooling="mean",
        query_prefix="query: ",
        passage_prefix="passage: ",
    ),
    ModelPreset(
        aliases=("gte-small", "thenlper/gte-small"),
        model_id="thenlper/gte-small",
        slug="gte-small",
        package_name="GTESmall.mlpackage",
        dimension=384,
        sequence_length=512,
        pooling="mean",
    ),
    ModelPreset(
        aliases=("jina-small-en", "jinaai/jina-embeddings-v2-small-en"),
        model_id="jinaai/jina-embeddings-v2-small-en",
        slug="jina-embeddings-v2-small-en",
        package_name="JinaEmbeddingsV2SmallEn.mlpackage",
        dimension=512,
        sequence_length=512,
        pooling="mean",
        trust_remote_code=True,
    ),
)


@dataclass(frozen=True)
class ConversionMetadata:
    model: str
    dimension: int
    sequence_length: int
    inputs: list[str]
    output: str
    pooling: str
    normalized: bool
    query_prefix: str
    passage_prefix: str
    token_input_shape: list[int | str]
    package_path: str
    compiled_model_path: str | None
    tokenizer_path: str


def main() -> None:
    args = parse_args()
    if args.list_models:
        print_model_presets()
        return

    preset = resolve_preset(args)
    output_dir = Path(args.output_dir) if args.output_dir else DEFAULT_OUTPUT_ROOT / preset.slug
    package_name = args.package_name or preset.package_name
    package_path = output_dir / package_name
    tokenizer_path = output_dir / "tokenizer"
    sequence_length = args.sequence_length or preset.sequence_length
    dimension = args.dimension or preset.dimension
    pooling = args.pooling or preset.pooling
    normalized = preset.normalized if args.normalized is None else args.normalized
    query_prefix = preset.query_prefix if args.query_prefix is None else args.query_prefix
    passage_prefix = preset.passage_prefix if args.passage_prefix is None else args.passage_prefix
    trust_remote_code = preset.trust_remote_code or args.trust_remote_code

    output_dir.mkdir(parents=True, exist_ok=True)

    tokenizer, traced_model = build_traced_model(
        model_id=preset.model_id,
        sequence_length=sequence_length,
        pooling=pooling,
        normalized=normalized,
        dimension=dimension,
        trust_remote_code=trust_remote_code,
    )

    convert_to_coreml(
        traced_model=traced_model,
        package_path=package_path,
        sequence_length=sequence_length,
        minimum_deployment_target=args.minimum_deployment_target,
        precision=args.precision,
    )
    tokenizer.save_pretrained(tokenizer_path)

    compiled_path = compile_model(package_path, output_dir) if args.compile else None

    metadata = ConversionMetadata(
        model=preset.model_id,
        dimension=dimension,
        sequence_length=sequence_length,
        inputs=[INPUT_IDS, ATTENTION_MASK, TOKEN_TYPE_IDS],
        output=OUTPUT_EMBEDDING,
        pooling=pooling,
        normalized=normalized,
        query_prefix=query_prefix,
        passage_prefix=passage_prefix,
        token_input_shape=[1, "sequence_length"],
        package_path=str(package_path.relative_to(ROOT_DIR)),
        compiled_model_path=relative_or_none(compiled_path),
        tokenizer_path=str(tokenizer_path.relative_to(ROOT_DIR)),
    )
    metadata_path = output_dir / "metadata.json"
    metadata_path.write_text(json.dumps(asdict(metadata), indent=2, sort_keys=True) + "\n")

    if args.verify:
        verify_coreml_parity(
            package_path=package_path,
            tokenizer=tokenizer,
            traced_model=traced_model,
            sequence_length=sequence_length,
            sample_texts=args.verify_text,
            tolerance=args.verify_tolerance,
            prefix=query_prefix,
        )

    print(f"Wrote Core ML package: {package_path}")
    if compiled_path is not None:
        print(f"Wrote compiled model: {compiled_path}")
    print(f"Wrote tokenizer: {tokenizer_path}")
    print(f"Wrote metadata: {metadata_path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Convert supported text embedding models to EmbeddingKit's Core ML contract."
    )
    parser.add_argument(
        "--preset",
        default="bge-small-en-v1.5",
        help="model preset alias; use --list-models to see supported presets",
    )
    parser.add_argument("--list-models", action="store_true", help="list supported model presets")
    parser.add_argument("--output-dir", help="directory for generated model artifacts")
    parser.add_argument("--package-name", help="Core ML package filename")
    parser.add_argument("--sequence-length", type=int, help="fixed token sequence length")
    parser.add_argument("--dimension", type=int, help="expected embedding dimension")
    parser.add_argument("--pooling", choices=["cls", "mean"], help="pooling strategy override")
    parser.add_argument(
        "--no-normalize",
        dest="normalized",
        action="store_false",
        default=None,
        help="disable L2 normalization in the exported model",
    )
    parser.add_argument("--query-prefix", help="query prefix recorded in metadata and used by --verify")
    parser.add_argument("--passage-prefix", help="passage prefix recorded in metadata")
    parser.add_argument(
        "--trust-remote-code",
        action="store_true",
        help="allow Hugging Face custom model code when loading the model",
    )
    parser.add_argument(
        "--minimum-deployment-target",
        default="macOS13",
        choices=["iOS16", "iOS17", "macOS13", "macOS14"],
        help="minimum Core ML deployment target for ML Program conversion",
    )
    parser.add_argument(
        "--precision",
        default="float16",
        choices=["float16", "float32"],
        help="Core ML compute precision",
    )
    parser.add_argument(
        "--compile",
        action="store_true",
        help="also compile the mlpackage to mlmodelc using xcrun coremlcompiler",
    )
    parser.add_argument(
        "--verify",
        action="store_true",
        help="compare Core ML predictions against the traced PyTorch wrapper",
    )
    parser.add_argument(
        "--verify-text",
        action="append",
        default=[
            "Mark and Erica arguing in a dim bar",
            "shots on the Harvard campus at night",
        ],
        help="sample text for parity verification; can be repeated",
    )
    parser.add_argument(
        "--verify-tolerance",
        type=float,
        default=0.05,
        help="maximum allowed absolute difference during parity verification",
    )
    return parser.parse_args()


def print_model_presets() -> None:
    print("Supported model presets:")
    for preset in MODEL_PRESETS:
        print(
            f"  {preset.aliases[0]:<28} {preset.dimension:>4}d "
            f"seq={preset.sequence_length:<4} pooling={preset.pooling:<4} "
            f"model={preset.model_id}"
        )


def resolve_preset(args: argparse.Namespace) -> ModelPreset:
    for preset in MODEL_PRESETS:
        if args.preset in preset.aliases:
            return preset
    supported = ", ".join(preset.aliases[0] for preset in MODEL_PRESETS)
    raise SystemExit(f"unknown preset {args.preset!r}. Supported presets: {supported}")


def build_traced_model(
    model_id: str,
    sequence_length: int,
    pooling: Pooling,
    normalized: bool,
    dimension: int,
    trust_remote_code: bool,
) -> tuple[Any, Any]:
    try:
        import torch
        from transformers import AutoModel, AutoTokenizer
    except ImportError as error:
        raise SystemExit(
            "Missing conversion dependencies. Install torch, transformers, coremltools, and numpy."
        ) from error

    tokenizer = AutoTokenizer.from_pretrained(model_id, trust_remote_code=trust_remote_code)
    base_model = AutoModel.from_pretrained(model_id, trust_remote_code=trust_remote_code)
    base_model.eval()

    class EmbeddingWrapper(torch.nn.Module):
        def __init__(self, model: Any) -> None:
            super().__init__()
            self.model = model

        def forward(
            self,
            input_ids: Any,
            attention_mask: Any,
            token_type_ids: Any,
        ) -> Any:
            outputs = self.model(
                input_ids=input_ids.to(torch.long),
                attention_mask=attention_mask.to(torch.long),
                token_type_ids=token_type_ids.to(torch.long),
                return_dict=False,
            )
            last_hidden_state = outputs[0]
            if pooling == "cls":
                embedding = last_hidden_state[:, 0]
            else:
                mask = attention_mask.to(last_hidden_state.dtype).unsqueeze(-1)
                summed = (last_hidden_state * mask).sum(dim=1)
                counts = mask.sum(dim=1).clamp(min=1e-9)
                embedding = summed / counts
            if normalized:
                embedding = torch.nn.functional.normalize(embedding, p=2, dim=1)
            return embedding

    wrapper = EmbeddingWrapper(base_model).eval()
    input_ids = torch.zeros((1, sequence_length), dtype=torch.int32)
    attention_mask = torch.ones((1, sequence_length), dtype=torch.int32)
    token_type_ids = torch.zeros((1, sequence_length), dtype=torch.int32)
    example_inputs = (input_ids, attention_mask, token_type_ids)

    with torch.no_grad():
        example_output = wrapper(*example_inputs)
        actual_dimension = int(example_output.shape[-1])
        if actual_dimension != dimension:
            raise SystemExit(
                f"{model_id} produced dimension {actual_dimension}, expected {dimension}"
            )
        traced_model = torch.jit.trace(wrapper, example_inputs, strict=False)
        traced_model.eval()

    return tokenizer, traced_model


def convert_to_coreml(
    traced_model: Any,
    package_path: Path,
    sequence_length: int,
    minimum_deployment_target: str,
    precision: str,
) -> None:
    try:
        import coremltools as ct
        import numpy as np
    except ImportError as error:
        raise SystemExit("Missing conversion dependency: coremltools.") from error

    deployment_target = {
        "iOS16": ct.target.iOS16,
        "iOS17": ct.target.iOS17,
        "macOS13": ct.target.macOS13,
        "macOS14": ct.target.macOS14,
    }[minimum_deployment_target]
    compute_precision = {
        "float16": ct.precision.FLOAT16,
        "float32": ct.precision.FLOAT32,
    }[precision]

    if package_path.exists():
        if package_path.is_dir():
            shutil.rmtree(package_path)
        else:
            package_path.unlink()

    coreml_model = ct.convert(
        traced_model,
        convert_to="mlprogram",
        minimum_deployment_target=deployment_target,
        compute_precision=compute_precision,
        inputs=[
            ct.TensorType(name=INPUT_IDS, shape=(1, sequence_length), dtype=np.int32),
            ct.TensorType(name=ATTENTION_MASK, shape=(1, sequence_length), dtype=np.int32),
            ct.TensorType(name=TOKEN_TYPE_IDS, shape=(1, sequence_length), dtype=np.int32),
        ],
        outputs=[ct.TensorType(name=OUTPUT_EMBEDDING)],
    )
    coreml_model.save(str(package_path))


def compile_model(package_path: Path, output_dir: Path) -> Path:
    compiler = shutil.which("xcrun")
    if compiler is None:
        raise SystemExit("xcrun is required to compile Core ML models.")

    subprocess.run(
        ["xcrun", "coremlcompiler", "compile", str(package_path), str(output_dir)],
        check=True,
    )
    compiled = output_dir / package_path.with_suffix(".mlmodelc").name
    if not compiled.exists():
        raise SystemExit(f"coremlcompiler did not produce expected output: {compiled}")
    return compiled


def verify_coreml_parity(
    package_path: Path,
    tokenizer: Any,
    traced_model: Any,
    sequence_length: int,
    sample_texts: list[str],
    tolerance: float,
    prefix: str,
) -> None:
    try:
        import coremltools as ct
        import numpy as np
        import torch
    except ImportError as error:
        raise SystemExit("Missing verification dependencies.") from error

    model = ct.models.MLModel(str(package_path))
    for text in sample_texts:
        encoded = encode_text(tokenizer, prefix + text, sequence_length)
        coreml_output = model.predict(encoded)[OUTPUT_EMBEDDING]

        with torch.no_grad():
            torch_output = traced_model(
                torch.from_numpy(encoded[INPUT_IDS]),
                torch.from_numpy(encoded[ATTENTION_MASK]),
                torch.from_numpy(encoded[TOKEN_TYPE_IDS]),
            ).detach().cpu().numpy()

        max_abs_diff = float(np.max(np.abs(coreml_output - torch_output)))
        if max_abs_diff > tolerance:
            raise SystemExit(
                f"Core ML parity failed for {text!r}: max abs diff "
                f"{max_abs_diff:.6f} exceeds tolerance {tolerance:.6f}"
            )
        print(f"Verified {text!r}: max_abs_diff={max_abs_diff:.6f}")


def encode_text(tokenizer: Any, text: str, sequence_length: int) -> dict[str, Any]:
    import numpy as np

    encoded = tokenizer(
        text,
        padding="max_length",
        truncation=True,
        max_length=sequence_length,
        return_tensors="np",
    )
    token_type_ids = encoded.get(TOKEN_TYPE_IDS)
    if token_type_ids is None:
        token_type_ids = np.zeros_like(encoded[INPUT_IDS])
    return {
        INPUT_IDS: encoded[INPUT_IDS].astype(np.int32),
        ATTENTION_MASK: encoded[ATTENTION_MASK].astype(np.int32),
        TOKEN_TYPE_IDS: token_type_ids.astype(np.int32),
    }


def relative_or_none(path: Path | None) -> str | None:
    if path is None:
        return None
    return str(path.relative_to(ROOT_DIR))


if __name__ == "__main__":
    main()
