from __future__ import annotations

import math
import os
from pathlib import Path

import pytest

import retrievalkit_embedding as embedding


@pytest.mark.skipif(
    os.environ.get("RETRIEVALKIT_EMBEDDING_LIVE_TEST") != "1",
    reason="requires explicit RETRIEVALKIT_EMBEDDING_LIVE_TEST=1 opt-in",
)
def test_live_fp32_embedding_contract(tmp_path: Path) -> None:
    embedding.OnnxEmbedder.prefetch(cache_directory=tmp_path)
    embedder = embedding.OnnxEmbedder.load(
        cache_directory=tmp_path,
        local_only=True,
    )
    assert embedder.model_info.profile == "fp32"
    assert embedder.model_info.dimension == 384
    assert embedder.model_info.max_input_tokens == 256

    for text in ["local retrieval", "Unicode: İstanbul — 你好"]:
        vector = embedder.embed(text)
        assert len(vector) == 384
        assert all(math.isfinite(value) for value in vector)
        assert math.sqrt(sum(value * value for value in vector)) == pytest.approx(
            1.0, abs=1e-4
        )

    batch = embedder.embed_batch(["alpha", "beta"])
    assert len(batch) == 2
    with pytest.raises(embedding.EmbeddingInputError):
        embedder.embed("")
    with pytest.raises(embedding.EmbeddingInputError):
        embedder.embed_batch([])
