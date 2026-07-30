from __future__ import annotations

from pathlib import Path

import pytest

import retrievalkit_embedding as embedding
from retrievalkit_embedding import _native


def test_public_contract() -> None:
    assert embedding.EMBEDDING_DIMENSION == 384
    assert embedding.ONNX_RUNTIME_VERSION == "1.24.3"
    assert issubclass(embedding.ModelUnavailableError, embedding.EmbeddingError)
    assert issubclass(embedding.DownloadError, embedding.ArtifactError)
    assert issubclass(embedding.EmbeddingInputError, ValueError)


def test_local_only_prefetch_is_network_free(tmp_path: Path) -> None:
    with pytest.raises(embedding.ModelUnavailableError):
        embedding.OnnxEmbedder.prefetch(
            cache_directory=tmp_path,
            local_only=True,
        )


def test_unqualified_runtime_is_a_typed_error(tmp_path: Path) -> None:
    runtime = tmp_path / "libonnxruntime.1.24.3.dylib"
    runtime.write_bytes(b"not a qualified runtime")
    with pytest.raises(embedding.EmbeddingRuntimeError, match="ONNX Runtime"):
        _native._verify_package_runtime(runtime)
