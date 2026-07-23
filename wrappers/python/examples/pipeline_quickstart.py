"""Runnable RetrievalKit Pipeline example with a deterministic demo embedder."""

from collections.abc import Sequence

from retrievalkit import Index
from retrievalkit.pipeline import Pipeline


def embed(texts: Sequence[str]) -> list[list[float]]:
    """Replace this deterministic function with your embedding provider."""
    return [
        [
            float("rust" in text.lower()),
            float("swift" in text.lower()),
            float("python" in text.lower()),
            1.0,
        ]
        for text in texts
    ]


index = Index(dimension=4)
pipeline = Pipeline(index, embed=embed)
pipeline.add("quickstart", "RetrievalKit connects Rust retrieval to Swift and Python.")

for hit in pipeline.search("Rust retrieval", limit=3):
    print(hit["document_id"], hit["score"], hit["text"])
