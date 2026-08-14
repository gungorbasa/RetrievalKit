#!/usr/bin/env python3
"""Acquire and fail-closed verify the two pinned Core ML benchmark profiles."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
from pathlib import Path, PurePosixPath


REPOSITORY = "gungorbasa/retrievalkit-minilm"
FP32_COMMIT = "405818d6afef1aaf2fc8da67da6caf20b55f0a28"
FP32_ARCHIVE = "all-MiniLM-L6-v2-coreml-fp32-v1.tar"
FP32_BYTES = 90_664_960
FP32_SHA256 = "e54611cc957f38fe82f5d82715a8043fff308a022c55b5471d4602c723540b6f"
FP32_MANIFEST_SHA256 = "085ebd344abdbc944568636d12ea10309e7b7457730b8be65a92c5da53091b60"
FP32_MODEL_TREE_SHA256 = "6de733c8906b816a310c2735712022ad2093edcd1b17566b86553a2c730b9ec7"
Q8_COMMIT = "617ce926c1f9e0289365d3e999474cc28b1645d4"
Q8_MANIFEST_SHA256 = "b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2"
Q8_MODEL_TREE_SHA256 = "72c82477ad518acdf88f95727f1af695702a9e3da7ae48799902bac3adc55281"
Q8_MODEL_TREE_BYTES = 22_724_832
Q8_PUBLISHED_TREE_SHA256 = "f9f78284766a1dd8352d85e7663fb366a938304c76204828badd7e52c2f05292"
Q8_PUBLISHED_TREE_BYTES = 22_724_760
Q8_FILES = (
    "coreml/all-MiniLM-L6-v2-q8.mlpackage/Manifest.json",
    "coreml/all-MiniLM-L6-v2-q8.mlpackage/Data/com.apple.CoreML/model.mlmodel",
    "coreml/all-MiniLM-L6-v2-q8.mlpackage/Data/com.apple.CoreML/weights/weight.bin",
)
TOKENIZER_FILES = (
    "tokenizer/special_tokens_map.json",
    "tokenizer/tokenizer.json",
    "tokenizer/tokenizer_config.json",
    "tokenizer/vocab.txt",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_tree(path: Path) -> tuple[int, str]:
    records: list[dict[str, object]] = []
    for item in sorted(path.rglob("*")):
        if item.is_symlink():
            raise ValueError(f"symbolic links are forbidden: {item}")
        if item.is_file():
            records.append({
                "path": item.relative_to(path).as_posix(),
                "sha256": sha256_file(item),
                "size": item.stat().st_size,
            })
    digest = hashlib.sha256()
    for record in records:
        digest.update(
            f"{record['path']}\0{record['size']}\0{record['sha256']}\n".encode("utf-8")
        )
    return sum(int(record["size"]) for record in records), digest.hexdigest()


def resolve_url(commit: str, path: str) -> str:
    return f"https://huggingface.co/{REPOSITORY}/resolve/{commit}/{path}"


def download(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "RetrievalKit-Apple-E2E/1"})
    with urllib.request.urlopen(request, timeout=120) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)


def safe_extract(archive: Path, destination: Path) -> None:
    with tarfile.open(archive, mode="r:") as bundle:
        for member in bundle.getmembers():
            relative = PurePosixPath(member.name)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError(f"unsafe archive path: {member.name}")
            if not member.isfile():
                raise ValueError(f"only regular archive files are allowed: {member.name}")
            source = bundle.extractfile(member)
            if source is None:
                raise ValueError(f"archive payload unavailable: {member.name}")
            target = destination.joinpath(*relative.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("wb") as output:
                shutil.copyfileobj(source, output, length=1024 * 1024)


def prepare_fp32(root: Path) -> dict[str, object]:
    archive = root / FP32_ARCHIVE
    manifest = root / "archive-manifest-v1.json"
    download(resolve_url(FP32_COMMIT, FP32_ARCHIVE), archive)
    download(resolve_url(FP32_COMMIT, "archive-manifest-v1.json"), manifest)
    if archive.stat().st_size != FP32_BYTES or sha256_file(archive) != FP32_SHA256:
        raise ValueError("FP32 archive size or SHA-256 mismatch")
    if sha256_file(manifest) != FP32_MANIFEST_SHA256:
        raise ValueError("FP32 archive manifest SHA-256 mismatch")
    extracted = root / "extracted"
    safe_extract(archive, extracted)
    model = extracted / "coreml/all-MiniLM-L6-v2-fp32.mlpackage"
    model_bytes, model_sha = canonical_tree(model)
    if model_sha != FP32_MODEL_TREE_SHA256:
        raise ValueError("FP32 model package canonical-tree SHA-256 mismatch")
    return {
        "archive_bytes": archive.stat().st_size,
        "archive_sha256": sha256_file(archive),
        "model_bytes": model_bytes,
        "model_path": str(model.relative_to(root)),
        "model_sha256": model_sha,
        "tokenizer_path": "extracted/tokenizer/tokenizer.json",
    }


def prepare_q8(root: Path) -> dict[str, object]:
    manifest = root / "manifest-v1.json"
    download(resolve_url(Q8_COMMIT, "manifest-v1.json"), manifest)
    if sha256_file(manifest) != Q8_MANIFEST_SHA256:
        raise ValueError("Q8 artifact manifest SHA-256 mismatch")
    document = json.loads(manifest.read_text(encoding="utf-8"))
    tokenizer_records = {item["path"]: item for item in document["tokenizer"]["files"]}
    for relative in (*Q8_FILES, *TOKENIZER_FILES):
        target = root / relative
        download(resolve_url(Q8_COMMIT, relative), target)
        if relative in tokenizer_records:
            expected = tokenizer_records[relative]
            if target.stat().st_size != expected["bytes"] or sha256_file(target) != expected["sha256"]:
                raise ValueError(f"Q8 tokenizer file mismatch: {relative}")
    model = root / "coreml/all-MiniLM-L6-v2-q8.mlpackage"
    model_bytes, model_sha = canonical_tree(model)
    if model_bytes != Q8_MODEL_TREE_BYTES or model_sha != Q8_MODEL_TREE_SHA256:
        raise ValueError("Q8 model package canonical-tree size or SHA-256 mismatch")
    return {
        "manifest_sha256": sha256_file(manifest),
        "model_bytes": model_bytes,
        "model_path": str(model.relative_to(root)),
        "model_sha256": model_sha,
        "published_manifest_model_bytes": Q8_PUBLISHED_TREE_BYTES,
        "published_manifest_model_sha256": Q8_PUBLISHED_TREE_SHA256,
        "published_manifest_matches_pinned_tree": False,
        "tokenizer_path": "tokenizer/tokenizer.json",
    }


def compile_model(package: Path, output: Path) -> str:
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["xcrun", "coremlcompiler", "compile", str(package), str(output)],
        check=True,
    )
    compiled = sorted(output.glob("*.mlmodelc"))
    if len(compiled) != 1:
        raise ValueError(f"expected one compiled Core ML model under {output}")
    return str(compiled[0].relative_to(output.parent))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("--output must not already exist")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{output.name}-", dir=output.parent))
    try:
        fp32 = prepare_fp32(temporary / "coreml-fp32-production-v1")
        q8 = prepare_q8(temporary / "coreml-weight-only-q8-experimental-v1")
        fp32_root = temporary / "coreml-fp32-production-v1"
        q8_root = temporary / "coreml-weight-only-q8-experimental-v1"
        fp32["compiled_model_path"] = compile_model(
            fp32_root / str(fp32["model_path"]), fp32_root / "compiled"
        )
        q8["compiled_model_path"] = compile_model(
            q8_root / str(q8["model_path"]), q8_root / "compiled"
        )
        result = {
            "profiles": {
                "coreml-fp32-production-v1": fp32,
                "coreml-weight-only-q8-experimental-v1": q8,
            },
            "schema_version": 1,
        }
        (temporary / "models-manifest.json").write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        os.replace(temporary, output)
        print(json.dumps(result, sort_keys=True))
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
