#!/usr/bin/env python3
"""Build and validate the pinned RetrievalKit MiniLM model artifact set.

Heavy conversion dependencies are imported only while exporting. Manifest
validation and the unit tests intentionally run with the Python standard
library alone.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import uuid
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Literal, Sequence


ROOT_DIR = Path(__file__).resolve().parents[2]
TARGET_DIR = ROOT_DIR / "target"
MODEL_CARD_SOURCE = ROOT_DIR / "scripts" / "embedding" / "retrievalkit-minilm-model-card.md"
DEFAULT_OUTPUT_DIR = TARGET_DIR / "embedding-models" / "retrievalkit-minilm"
DEFAULT_CACHE_DIR = TARGET_DIR / "huggingface-cache"

MODEL_ID = "sentence-transformers/all-MiniLM-L6-v2"
MODEL_REVISION = "c9745ed1d9f207416be6d2e6f8de32d1f16199bf"
MODEL_DIMENSION = 384
MAX_SEQUENCE_LENGTH = 256
ONNX_OPSET = 17
INPUT_NAMES = ("input_ids", "attention_mask", "token_type_ids")
OUTPUT_NAME = "embedding"
ONNX_Q8_NODES_TO_EXCLUDE = (
    "/model/encoder/layer.0/attention/self/query/MatMul",
    "/model/encoder/layer.0/attention/self/key/MatMul",
    "/model/encoder/layer.0/attention/self/value/MatMul",
    "/model/encoder/layer.0/attention/output/dense/MatMul",
    "/model/encoder/layer.0/intermediate/dense/MatMul",
    "/model/encoder/layer.0/output/dense/MatMul",
    "/model/encoder/layer.2/intermediate/dense/MatMul",
)
TOKENIZER_FILES = (
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "vocab.txt",
)
SOURCE_FILES = ("config.json", "model.safetensors", *TOKENIZER_FILES)
SCHEMA_VERSION = 1
SCHEMA_NAME = "retrievalkit-embedding-artifacts-manifest-v1"

ArtifactFormat = Literal["onnx", "coreml"]
SequenceShape = Literal["dynamic", "fixed", "flexible"]


@dataclass(frozen=True)
class ArtifactSpec:
    name: str
    relative_path: str
    format: ArtifactFormat
    precision: str
    quantization: str
    sequence_shape: SequenceShape
    candidate: bool = False


@dataclass(frozen=True)
class ExportOptions:
    output_dir: Path
    cache_dir: Path
    offline: bool
    include_onnx: bool
    include_coreml: bool
    include_flexible_coreml_candidate: bool


def artifact_plan(
    *,
    include_onnx: bool = True,
    include_coreml: bool = True,
    include_flexible_coreml_candidate: bool = False,
) -> tuple[ArtifactSpec, ...]:
    artifacts: list[ArtifactSpec] = []
    if include_onnx:
        artifacts.extend(
            (
                ArtifactSpec(
                    "onnx-fp32",
                    "onnx/all-MiniLM-L6-v2-fp32.onnx",
                    "onnx",
                    "float32",
                    "none",
                    "dynamic",
                ),
                ArtifactSpec(
                    "onnx-fp16",
                    "onnx/all-MiniLM-L6-v2-fp16.onnx",
                    "onnx",
                    "float16",
                    "none",
                    "dynamic",
                ),
                ArtifactSpec(
                    "onnx-dynamic-q8",
                    "onnx/all-MiniLM-L6-v2-q8.onnx",
                    "onnx",
                    "float32",
                    "dynamic-int8",
                    "dynamic",
                ),
            )
        )
    if include_coreml:
        artifacts.extend(
            (
                ArtifactSpec(
                    "coreml-fp32",
                    "coreml/all-MiniLM-L6-v2-fp32.mlpackage",
                    "coreml",
                    "float32",
                    "none",
                    "fixed",
                ),
                ArtifactSpec(
                    "coreml-fp16",
                    "coreml/all-MiniLM-L6-v2-fp16.mlpackage",
                    "coreml",
                    "float16",
                    "none",
                    "fixed",
                ),
                ArtifactSpec(
                    "coreml-weight-only-q8",
                    "coreml/all-MiniLM-L6-v2-q8.mlpackage",
                    "coreml",
                    "float16",
                    "weight-only-int8",
                    "fixed",
                ),
            )
        )
        if include_flexible_coreml_candidate:
            artifacts.append(
                ArtifactSpec(
                    "coreml-flexible-fp16-candidate",
                    "coreml/all-MiniLM-L6-v2-flexible-fp16.mlpackage",
                    "coreml",
                    "float16",
                    "none",
                    "flexible",
                    candidate=True,
                )
            )
    return tuple(artifacts)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Export the pinned all-MiniLM-L6-v2 model to RetrievalKit's ONNX "
            "and Core ML artifact contract."
        )
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help="artifact directory; must be within the repository target directory",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=DEFAULT_CACHE_DIR,
        help="Hugging Face cache directory; must be within the repository target directory",
    )
    parser.add_argument(
        "--offline",
        action="store_true",
        help="use only an already-cached copy of the pinned source revision",
    )
    parser.add_argument(
        "--skip-onnx",
        action="store_true",
        help="skip all ONNX artifacts (intended for conversion diagnostics)",
    )
    parser.add_argument(
        "--skip-coreml",
        action="store_true",
        help="skip all Core ML artifacts (useful on non-macOS builders)",
    )
    parser.add_argument(
        "--include-flexible-coreml-candidate",
        action="store_true",
        help=(
            "also export an FP16 Core ML candidate with sequence length 1...256; "
            "the three fixed-length Core ML artifacts remain canonical"
        ),
    )
    parser.add_argument(
        "--validate-manifest",
        type=Path,
        help="validate an existing manifest and exit without loading conversion dependencies",
    )
    return parser


def parse_options(argv: Sequence[str] | None = None) -> tuple[ExportOptions | None, Path | None]:
    args = build_parser().parse_args(argv)
    if args.validate_manifest is not None:
        return None, args.validate_manifest.resolve()

    output_dir = require_target_path(args.output_dir, "output directory")
    cache_dir = require_target_path(args.cache_dir, "cache directory")
    if args.skip_onnx and args.skip_coreml:
        raise SystemExit("--skip-onnx and --skip-coreml cannot be used together")
    if args.include_flexible_coreml_candidate and args.skip_coreml:
        raise SystemExit(
            "--include-flexible-coreml-candidate cannot be used with --skip-coreml"
        )
    return (
        ExportOptions(
            output_dir=output_dir,
            cache_dir=cache_dir,
            offline=args.offline,
            include_onnx=not args.skip_onnx,
            include_coreml=not args.skip_coreml,
            include_flexible_coreml_candidate=args.include_flexible_coreml_candidate,
        ),
        None,
    )


def require_target_path(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    target = TARGET_DIR.resolve()
    if resolved != target and target not in resolved.parents:
        raise SystemExit(f"{label} must be inside {target}: {resolved}")
    return resolved


def main(argv: Sequence[str] | None = None) -> int:
    options, validation_path = parse_options(argv)
    if validation_path is not None:
        validate_manifest(validation_path)
        print(f"Validated artifact manifest: {validation_path}")
        return 0
    assert options is not None

    specs = artifact_plan(
        include_onnx=options.include_onnx,
        include_coreml=options.include_coreml,
        include_flexible_coreml_candidate=options.include_flexible_coreml_candidate,
    )
    export_artifacts(options, specs)
    copy_repository_metadata(options.output_dir)
    manifest_path = options.output_dir / "manifest-v1.json"
    write_manifest(options.output_dir, specs, manifest_path)
    validate_manifest(manifest_path)
    shutil.copyfile(manifest_path, options.output_dir / "manifest.json")
    print(f"Wrote and validated artifact manifest: {manifest_path}")
    return 0


def export_artifacts(options: ExportOptions, specs: Sequence[ArtifactSpec]) -> None:
    snapshot = download_source(options.cache_dir, options.offline)
    tokenizer_dir = options.output_dir / "tokenizer"
    copy_tokenizer(snapshot, tokenizer_dir)

    model = load_model(snapshot)
    wrapper = build_embedding_wrapper(model)
    by_name = {spec.name: spec for spec in specs}

    if "onnx-fp32" in by_name:
        export_onnx_set(wrapper, options.output_dir, by_name)
    if "coreml-fp32" in by_name:
        export_coreml_set(wrapper, options.output_dir, by_name)


def download_source(cache_dir: Path, offline: bool) -> Path:
    try:
        from huggingface_hub import snapshot_download
    except ImportError as error:
        raise SystemExit(
            "Missing export dependency: huggingface_hub. Install the model "
            "conversion requirements in a target-local virtual environment."
        ) from error

    cache_dir.mkdir(parents=True, exist_ok=True)
    snapshot = snapshot_download(
        repo_id=MODEL_ID,
        revision=MODEL_REVISION,
        cache_dir=cache_dir,
        local_files_only=offline,
        allow_patterns=list(SOURCE_FILES),
    )
    return Path(snapshot)


def copy_tokenizer(snapshot: Path, tokenizer_dir: Path) -> None:
    tokenizer_dir.mkdir(parents=True, exist_ok=True)
    for filename in TOKENIZER_FILES:
        source = snapshot / filename
        if not source.is_file():
            raise SystemExit(f"pinned source snapshot is missing tokenizer file: {filename}")
        shutil.copyfile(source, tokenizer_dir / filename)


def copy_repository_metadata(output_dir: Path) -> None:
    sources = {
        MODEL_CARD_SOURCE: output_dir / "README.md",
        ROOT_DIR / "LICENSE": output_dir / "LICENSE",
        ROOT_DIR / "NOTICE": output_dir / "NOTICE",
    }
    for source, destination in sources.items():
        if not source.is_file():
            raise SystemExit(f"required model repository metadata is missing: {source}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


def load_model(snapshot: Path) -> Any:
    try:
        from transformers import AutoModel
    except ImportError as error:
        raise SystemExit("Missing export dependency: transformers.") from error

    model = AutoModel.from_pretrained(snapshot, local_files_only=True)
    model.float()
    model.eval()
    return model


def build_embedding_wrapper(model: Any) -> Any:
    try:
        import torch
    except ImportError as error:
        raise SystemExit("Missing export dependency: torch.") from error

    class MiniLMEmbedding(torch.nn.Module):
        def __init__(self, base_model: Any) -> None:
            super().__init__()
            self.model = base_model

        def forward(
            self,
            input_ids: Any,
            attention_mask: Any,
            token_type_ids: Any,
        ) -> Any:
            outputs = self.model(
                input_ids=input_ids,
                attention_mask=attention_mask,
                token_type_ids=token_type_ids,
                return_dict=False,
            )
            token_embeddings = outputs[0]
            mask = attention_mask.to(token_embeddings.dtype).unsqueeze(-1)
            summed = (token_embeddings * mask).sum(dim=1)
            counts = mask.sum(dim=1).clamp(min=1e-9)
            return torch.nn.functional.normalize(summed / counts, p=2, dim=1)

    return MiniLMEmbedding(model).eval()


def example_inputs(dtype: Any) -> tuple[Any, Any, Any]:
    import torch

    input_ids = torch.zeros((1, MAX_SEQUENCE_LENGTH), dtype=dtype)
    attention_mask = torch.ones((1, MAX_SEQUENCE_LENGTH), dtype=dtype)
    token_type_ids = torch.zeros((1, MAX_SEQUENCE_LENGTH), dtype=dtype)
    return input_ids, attention_mask, token_type_ids


def export_onnx_set(
    wrapper: Any,
    output_dir: Path,
    specs: dict[str, ArtifactSpec],
) -> None:
    try:
        import onnx
        import torch
        from onnxruntime.quantization import QuantType, quantize_dynamic
        from onnxruntime.transformers.float16 import convert_float_to_float16
    except ImportError as error:
        raise SystemExit(
            "Missing ONNX export dependencies. Install torch, onnx, and onnxruntime."
        ) from error

    fp32_path = output_dir / specs["onnx-fp32"].relative_path
    fp16_path = output_dir / specs["onnx-fp16"].relative_path
    q8_path = output_dir / specs["onnx-dynamic-q8"].relative_path
    fp32_path.parent.mkdir(parents=True, exist_ok=True)

    inputs = example_inputs(torch.int64)
    with torch.no_grad():
        output = wrapper(*inputs)
    if tuple(output.shape) != (1, MODEL_DIMENSION):
        raise SystemExit(
            f"model produced shape {tuple(output.shape)}, expected (1, {MODEL_DIMENSION})"
        )

    torch.onnx.export(
        wrapper,
        inputs,
        str(fp32_path),
        export_params=True,
        do_constant_folding=True,
        input_names=list(INPUT_NAMES),
        output_names=[OUTPUT_NAME],
        dynamic_axes={
            name: {0: "batch", 1: "sequence"} for name in INPUT_NAMES
        }
        | {OUTPUT_NAME: {0: "batch"}},
        opset_version=ONNX_OPSET,
    )
    add_onnx_contract_metadata(onnx, fp32_path)

    fp32_model = onnx.load(str(fp32_path))
    fp16_model = convert_float_to_float16(
        fp32_model,
        keep_io_types=True,
        disable_shape_infer=False,
    )
    onnx.save_model(fp16_model, str(fp16_path))
    quantize_dynamic(
        model_input=str(fp32_path),
        model_output=str(q8_path),
        weight_type=QuantType.QInt8,
        per_channel=True,
        reduce_range=False,
        nodes_to_exclude=list(ONNX_Q8_NODES_TO_EXCLUDE),
    )


def add_onnx_contract_metadata(onnx: Any, model_path: Path) -> None:
    model = onnx.load(str(model_path))
    values = {
        "retrievalkit.model_id": MODEL_ID,
        "retrievalkit.model_revision": MODEL_REVISION,
        "retrievalkit.max_sequence_length": str(MAX_SEQUENCE_LENGTH),
        "retrievalkit.pooling": "mean",
        "retrievalkit.normalized": "true",
    }
    del model.metadata_props[:]
    for key, value in sorted(values.items()):
        item = model.metadata_props.add()
        item.key = key
        item.value = value
    onnx.save_model(model, str(model_path))


def export_coreml_set(
    wrapper: Any,
    output_dir: Path,
    specs: dict[str, ArtifactSpec],
) -> None:
    if sys.platform != "darwin":
        raise SystemExit("Core ML export requires macOS.")
    if sys.version_info >= (3, 14):
        raise SystemExit(
            "Core ML export requires a coremltools-supported Python, preferably 3.11 or 3.12."
        )
    try:
        import coremltools as ct
        import numpy as np
        import torch
    except ImportError as error:
        raise SystemExit(
            "Missing Core ML export dependencies. Install torch, numpy, and coremltools."
        ) from error

    inputs = example_inputs(torch.int32)
    with torch.no_grad():
        traced = torch.jit.trace(wrapper, inputs, strict=False).eval()

    fp32_path = output_dir / specs["coreml-fp32"].relative_path
    fp16_path = output_dir / specs["coreml-fp16"].relative_path
    q8_path = output_dir / specs["coreml-weight-only-q8"].relative_path
    fp32_path.parent.mkdir(parents=True, exist_ok=True)
    fixed_inputs = coreml_inputs(ct, np, flexible=False)

    fp32 = ct.convert(
        traced,
        convert_to="mlprogram",
        minimum_deployment_target=ct.target.macOS13,
        compute_precision=ct.precision.FLOAT32,
        inputs=fixed_inputs,
        outputs=[ct.TensorType(name=OUTPUT_NAME)],
    )
    save_coreml(fp32, fp32_path)
    fp16 = ct.convert(
        traced,
        convert_to="mlprogram",
        minimum_deployment_target=ct.target.macOS13,
        compute_precision=ct.precision.FLOAT16,
        inputs=fixed_inputs,
        outputs=[ct.TensorType(name=OUTPUT_NAME)],
    )
    save_coreml(fp16, fp16_path)
    save_coreml(quantize_coreml_weights(ct, fp16), q8_path)

    candidate = specs.get("coreml-flexible-fp16-candidate")
    if candidate is not None:
        flexible = ct.convert(
            traced,
            convert_to="mlprogram",
            minimum_deployment_target=ct.target.macOS13,
            compute_precision=ct.precision.FLOAT16,
            inputs=coreml_inputs(ct, np, flexible=True),
            outputs=[ct.TensorType(name=OUTPUT_NAME)],
        )
        save_coreml(flexible, output_dir / candidate.relative_path)


def coreml_inputs(ct: Any, np: Any, *, flexible: bool) -> list[Any]:
    sequence: int | Any
    if flexible:
        sequence = ct.RangeDim(
            lower_bound=1,
            upper_bound=MAX_SEQUENCE_LENGTH,
            default=MAX_SEQUENCE_LENGTH,
            symbol="sequence",
        )
    else:
        sequence = MAX_SEQUENCE_LENGTH
    return [
        ct.TensorType(name=name, shape=(1, sequence), dtype=np.int32)
        for name in INPUT_NAMES
    ]


def quantize_coreml_weights(ct: Any, model: Any) -> Any:
    try:
        config = ct.optimize.coreml.OptimizationConfig(
            global_config=ct.optimize.coreml.OpLinearQuantizerConfig(
                mode="linear_symmetric",
                dtype="int8",
                granularity="per_channel",
            )
        )
        return ct.optimize.coreml.linear_quantize_weights(model, config=config)
    except AttributeError as error:
        raise SystemExit(
            "Installed coremltools does not support ML Program weight-only int8 "
            "quantization; install a current coremltools release."
        ) from error


def save_coreml(model: Any, path: Path) -> None:
    if path.exists():
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
    model.user_defined_metadata.update(
        {
            "com.retrievalkit.model-id": MODEL_ID,
            "com.retrievalkit.model-revision": MODEL_REVISION,
            "com.retrievalkit.max-sequence-length": str(MAX_SEQUENCE_LENGTH),
            "com.retrievalkit.pooling": "mean",
            "com.retrievalkit.normalized": "true",
        }
    )
    model.save(str(path))
    canonicalize_coreml_package(path)


def canonicalize_coreml_package(path: Path) -> None:
    """Remove nondeterministic protobuf ordering and package UUIDs."""
    try:
        from coremltools.proto import Model_pb2
    except ImportError as error:
        raise SystemExit("coremltools protobuf support is unavailable.") from error

    for model_path in sorted((path / "Data").rglob("*.mlmodel")):
        model = Model_pb2.Model()
        model.ParseFromString(model_path.read_bytes())
        model_path.write_bytes(model.SerializeToString(deterministic=True))

    manifest_path = path / "Manifest.json"
    manifest = json.loads(manifest_path.read_text())
    canonical = canonicalize_coreml_manifest(manifest)
    manifest_path.write_text(json.dumps(canonical, indent=2, sort_keys=True) + "\n")


def canonicalize_coreml_manifest(manifest: dict[str, Any]) -> dict[str, Any]:
    entries = manifest.get("itemInfoEntries")
    if not isinstance(entries, dict):
        raise ValueError("Core ML package manifest has no itemInfoEntries object")
    old_root = manifest.get("rootModelIdentifier")
    replacements: dict[str, str] = {}
    canonical_entries: dict[str, Any] = {}
    for old_identifier, entry in entries.items():
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str):
            raise ValueError("Core ML package manifest entry has no path")
        identifier = str(
            uuid.uuid5(
                uuid.NAMESPACE_URL,
                f"{MODEL_ID}@{MODEL_REVISION}:{entry['path']}",
            )
        ).upper()
        replacements[old_identifier] = identifier
        canonical_entries[identifier] = entry
    if old_root not in replacements:
        raise ValueError("Core ML package root identifier is not an item")
    return {
        **manifest,
        "itemInfoEntries": canonical_entries,
        "rootModelIdentifier": replacements[old_root],
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def path_stats(path: Path) -> tuple[int, str, str]:
    if path.is_file():
        return path.stat().st_size, sha256_file(path), "file"
    if not path.is_dir():
        raise FileNotFoundError(path)

    total = 0
    digest = hashlib.sha256()
    files = sorted(item for item in path.rglob("*") if item.is_file())
    if not files:
        raise ValueError(f"artifact directory is empty: {path}")
    for item in files:
        relative = item.relative_to(path).as_posix()
        size = item.stat().st_size
        item_hash = sha256_file(item)
        total += size
        digest.update(f"{relative}\0{size}\0{item_hash}\n".encode())
    return total, digest.hexdigest(), "canonical-tree-v1"


def tokenizer_manifest(output_dir: Path) -> dict[str, Any]:
    tokenizer_dir = output_dir / "tokenizer"
    files: list[dict[str, Any]] = []
    aggregate = hashlib.sha256()
    for filename in TOKENIZER_FILES:
        path = tokenizer_dir / filename
        size, digest, _ = path_stats(path)
        files.append({"path": f"tokenizer/{filename}", "bytes": size, "sha256": digest})
        aggregate.update(f"{filename}\0{size}\0{digest}\n".encode())
    return {
        "path": "tokenizer",
        "sha256": aggregate.hexdigest(),
        "digest_kind": "canonical-tokenizer-v1",
        "files": files,
    }


def manifest_document(
    output_dir: Path,
    specs: Sequence[ArtifactSpec],
) -> dict[str, Any]:
    artifacts: list[dict[str, Any]] = []
    for spec in specs:
        size, digest, digest_kind = path_stats(output_dir / spec.relative_path)
        sequence_contract: dict[str, Any] = {
            "shape": spec.sequence_shape,
            "maximum": MAX_SEQUENCE_LENGTH,
        }
        if spec.sequence_shape == "fixed":
            sequence_contract["value"] = MAX_SEQUENCE_LENGTH
        else:
            sequence_contract["minimum"] = 1
        artifacts.append(
            {
                **asdict(spec),
                "bytes": size,
                "sha256": digest,
                "digest_kind": digest_kind,
                "sequence_length": sequence_contract,
            }
        )

    return {
        "schema": SCHEMA_NAME,
        "schema_version": SCHEMA_VERSION,
        "model": {
            "id": MODEL_ID,
            "revision": MODEL_REVISION,
            "dimension": MODEL_DIMENSION,
            "pooling": "mean",
            "normalized": True,
            "maximum_sequence_length": MAX_SEQUENCE_LENGTH,
        },
        "license": {
            "spdx_id": "Apache-2.0",
            "source": (
                f"https://huggingface.co/{MODEL_ID}/blob/{MODEL_REVISION}/README.md"
            ),
            "source_model_card_declared": True,
        },
        "tokenizer": tokenizer_manifest(output_dir),
        "export_contract": {
            "inputs": list(INPUT_NAMES),
            "output": OUTPUT_NAME,
            "onnx_opset": ONNX_OPSET,
            "onnx_sequence_length": {
                "shape": "dynamic",
                "minimum": 1,
                "maximum": MAX_SEQUENCE_LENGTH,
                "enforcement": "tokenizer-and-runtime-contract",
            },
        },
        "artifacts": artifacts,
    }


def write_manifest(
    output_dir: Path,
    specs: Sequence[ArtifactSpec],
    manifest_path: Path,
) -> None:
    document = manifest_document(output_dir, specs)
    manifest_path.write_text(
        json.dumps(document, indent=2, sort_keys=True, ensure_ascii=True) + "\n"
    )


def validate_manifest(manifest_path: Path) -> None:
    document = json.loads(manifest_path.read_text())
    base = manifest_path.parent
    require(document.get("schema") == SCHEMA_NAME, "unexpected manifest schema")
    require(document.get("schema_version") == SCHEMA_VERSION, "unexpected schema version")
    model = document.get("model")
    require(isinstance(model, dict), "model metadata is missing")
    require(model.get("id") == MODEL_ID, "model id is not pinned")
    require(model.get("revision") == MODEL_REVISION, "model revision is not pinned")
    require(
        model.get("maximum_sequence_length") == MAX_SEQUENCE_LENGTH,
        "maximum sequence length is not 256",
    )
    license_metadata = document.get("license")
    require(isinstance(license_metadata, dict), "license metadata is missing")
    require(license_metadata.get("spdx_id") == "Apache-2.0", "license is not Apache-2.0")

    tokenizer = document.get("tokenizer")
    require(isinstance(tokenizer, dict), "tokenizer metadata is missing")
    expected_tokenizer = tokenizer_manifest(base)
    require(tokenizer == expected_tokenizer, "tokenizer hashes do not match")

    artifacts = document.get("artifacts")
    require(isinstance(artifacts, list) and artifacts, "artifact list is empty")
    names: set[str] = set()
    for artifact in artifacts:
        require(isinstance(artifact, dict), "artifact entry is not an object")
        name = artifact.get("name")
        require(isinstance(name, str) and name not in names, "artifact name is invalid or duplicated")
        names.add(name)
        relative = artifact.get("relative_path")
        require(isinstance(relative, str), f"{name}: relative path is missing")
        path = safe_manifest_path(base, relative)
        size, digest, digest_kind = path_stats(path)
        require(artifact.get("bytes") == size, f"{name}: byte size does not match")
        require(artifact.get("sha256") == digest, f"{name}: SHA-256 does not match")
        require(
            artifact.get("digest_kind") == digest_kind,
            f"{name}: digest kind does not match artifact type",
        )
        sequence = artifact.get("sequence_length")
        require(isinstance(sequence, dict), f"{name}: sequence contract is missing")
        require(
            sequence.get("maximum") == MAX_SEQUENCE_LENGTH,
            f"{name}: sequence contract exceeds 256",
        )


def safe_manifest_path(base: Path, relative: str) -> Path:
    candidate = (base / relative).resolve()
    resolved_base = base.resolve()
    require(
        candidate != resolved_base and resolved_base in candidate.parents,
        f"artifact path escapes manifest directory: {relative}",
    )
    return candidate


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


if __name__ == "__main__":
    raise SystemExit(main())
