from pathlib import Path

from retrievalkit_embedding import ModelInfo, OnnxEmbedder


def typed_api(cache: Path, runtime: Path) -> None:
    OnnxEmbedder.prefetch(cache_directory=cache)
    embedder = OnnxEmbedder.load(
        cache_directory=cache,
        runtime_library_path=runtime,
        local_only=True,
    )
    info: ModelInfo = embedder.model_info
    vector: list[float] = embedder.embed("local retrieval")
    batch: list[list[float]] = embedder.embed_batch(["Unicode: İstanbul", "search"])
    assert info.dimension == len(vector) == len(batch[0])
