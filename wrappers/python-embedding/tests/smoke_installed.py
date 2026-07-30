"""Installed-wheel smoke check; run outside the source tree."""

from __future__ import annotations

from tempfile import TemporaryDirectory

import retrievalkit_embedding as embedding


def main() -> None:
    assert embedding.EMBEDDING_DIMENSION == 384
    assert embedding.ONNX_RUNTIME_VERSION == "1.24.3"
    with TemporaryDirectory() as cache:
        try:
            embedding.OnnxEmbedder.prefetch(
                cache_directory=cache,
                local_only=True,
            )
        except embedding.ModelUnavailableError:
            pass
        else:
            raise AssertionError("empty local-only cache unexpectedly resolved a model")


if __name__ == "__main__":
    main()
