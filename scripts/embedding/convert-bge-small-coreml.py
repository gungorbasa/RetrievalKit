#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MODEL_ID = "BAAI/bge-small-en-v1.5"
DEFAULT_OUTPUT_DIR = ROOT_DIR / "target" / "embedding-models" / "bge-small-en-v1.5"
DEFAULT_PACKAGE_NAME = "BGESmallEnV15.mlpackage"
DEFAULT_SEQUENCE_LENGTH = 512
DEFAULT_DIMENSION = 384
INPUT_IDS = "input_ids"
ATTENTION_MASK = "attention_mask"
TOKEN_TYPE_IDS = "token_type_ids"
OUTPUT_EMBEDDING = "embedding"


@dataclass(frozen=True)
class ConversionMetadata:
    model: str
    dimension: int
    sequence_length: int
    inputs: list[str]
    output: str
    pooling: str
    normalized: bool
    token_input_shape: list[int | str]
    package_path: str
    compiled_model_path: str | None
    tokenizer_path: str


def main() -> None:
    args = parse_args()
    output_dir = Path(args.output_dir)
    package_path = output_dir / args.package_name
    tokenizer_path = output_dir / "tokenizer"

    output_dir.mkdir(parents=True, exist_ok=True)

    tokenizer, traced_model, example_inputs = build_traced_model(
        model_id=args.model,
        sequence_length=args.sequence_length,
    )

    convert_to_coreml(
        traced_model=traced_model,
        package_path=package_path,
        sequence_length=args.sequence_length,
        minimum_deployment_target=args.minimum_deployment_target,
        precision=args.precision,
    )
    tokenizer.save_pretrained(tokenizer_path)

    compiled_path = None
    if args.compile:
        compiled_path = compile_model(package_path, output_dir)

    metadata = ConversionMetadata(
        model=args.model,
        dimension=args.dimension,
        sequence_length=args.sequence_length,
        inputs=[INPUT_IDS, ATTENTION_MASK, TOKEN_TYPE_IDS],
        output=OUTPUT_EMBEDDING,
        pooling="cls",
        normalized=True,
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
            sequence_length=args.sequence_length,
            sample_texts=args.verify_text,
            tolerance=args.verify_tolerance,
        )

    print(f"Wrote Core ML package: {package_path}")
    if compiled_path is not None:
        print(f"Wrote compiled model: {compiled_path}")
    print(f"Wrote tokenizer: {tokenizer_path}")
    print(f"Wrote metadata: {metadata_path}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Convert BAAI/bge-small-en-v1.5 to EmbeddingKit's Core ML contract."
    )
    parser.add_argument("--model", default=DEFAULT_MODEL_ID, help="Hugging Face model id")
    parser.add_argument(
        "--output-dir",
        default=str(DEFAULT_OUTPUT_DIR),
        help="directory for generated model artifacts",
    )
    parser.add_argument(
        "--package-name",
        default=DEFAULT_PACKAGE_NAME,
        help="Core ML package filename",
    )
    parser.add_argument(
        "--sequence-length",
        type=int,
        default=DEFAULT_SEQUENCE_LENGTH,
        help="fixed token sequence length used by the exported Core ML model",
    )
    parser.add_argument(
        "--dimension",
        type=int,
        default=DEFAULT_DIMENSION,
        help="expected embedding dimension recorded in metadata",
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


def build_traced_model(model_id: str, sequence_length: int) -> tuple[Any, Any, tuple[Any, Any, Any]]:
    try:
        import torch
        from transformers import AutoModel, AutoTokenizer
    except ImportError as error:
        raise SystemExit(
            "Missing conversion dependencies. Install torch, transformers, and coremltools."
        ) from error

    tokenizer = AutoTokenizer.from_pretrained(model_id)
    base_model = AutoModel.from_pretrained(model_id)
    base_model.eval()

    class BGEEmbeddingWrapper(torch.nn.Module):
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
            cls_embedding = outputs[0][:, 0]
            return torch.nn.functional.normalize(cls_embedding, p=2, dim=1)

    wrapper = BGEEmbeddingWrapper(base_model).eval()
    input_ids = torch.zeros((1, sequence_length), dtype=torch.int32)
    attention_mask = torch.ones((1, sequence_length), dtype=torch.int32)
    token_type_ids = torch.zeros((1, sequence_length), dtype=torch.int32)
    example_inputs = (input_ids, attention_mask, token_type_ids)

    with torch.no_grad():
        traced_model = torch.jit.trace(wrapper, example_inputs, strict=False)
        traced_model.eval()

    return tokenizer, traced_model, example_inputs


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
) -> None:
    try:
        import coremltools as ct
        import numpy as np
        import torch
    except ImportError as error:
        raise SystemExit("Missing verification dependencies.") from error

    model = ct.models.MLModel(str(package_path))
    for text in sample_texts:
        encoded = encode_text(tokenizer, text, sequence_length)
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
