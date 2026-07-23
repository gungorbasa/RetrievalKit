"""High-level document ingestion and text-search orchestration."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import Protocol

from ._native import Index
from .ingest import RustTextChunker
from .types import AddDocumentResult, ChunkInput, Filter, HybridHit, Metadata, TextChunk

EmbeddingProvider = Callable[[Sequence[str]], Sequence[Sequence[float]]]
TokenCounter = Callable[[str], int]


class PipelineError(ValueError):
    """Base class for pipeline validation and configuration failures."""


class PipelineConfigurationError(PipelineError):
    """The pipeline was configured with incompatible options."""


class EmptyDocumentError(PipelineError):
    """A document produced no indexable chunks."""


class InvalidChunkError(PipelineError):
    """A custom chunk is not an exact, ordered source slice."""


class TokenLimitError(PipelineError):
    """Source text cannot be made to fit the embedding token budget."""


class EmbeddingCountMismatchError(PipelineError):
    """The provider did not return one embedding per input text."""


class EmbeddingDimensionMismatchError(PipelineError):
    """An embedding does not match the index dimension."""


class DocumentChunker(Protocol):
    """Application-defined chunking policy consumed and validated by Pipeline."""

    def chunks(self, text: str) -> list[TextChunk]: ...


class TokenAwareDocumentChunker:
    """Subdivide chunks until each fits an exact model token budget."""

    def __init__(
        self,
        base_chunker: DocumentChunker,
        *,
        count_tokens: TokenCounter,
        max_tokens: int,
    ) -> None:
        if max_tokens <= 0:
            raise PipelineConfigurationError("max_tokens must be greater than zero")
        self._base_chunker = base_chunker
        self._count_tokens = count_tokens
        self._max_tokens = max_tokens

    def chunks(self, text: str) -> list[TextChunk]:
        return [
            piece
            for chunk in self._base_chunker.chunks(text)
            for piece in self._fit(chunk)
        ]

    def _fit(self, chunk: TextChunk) -> list[TextChunk]:
        if self._count_tokens(chunk["text"]) <= self._max_tokens:
            return [chunk]
        if len(chunk["text"]) <= 1:
            raise TokenLimitError(
                f"Token limit {self._max_tokens} cannot fit source text "
                f"{chunk['text']!r}. Use a model with a larger input limit or "
                "a tokenizer with fewer special tokens."
            )

        splitter = RustTextChunker(
            max_characters=max(1, len(chunk["text"]) // 2),
            overlap_characters=0,
            strategy="sentence",
        )
        pieces = splitter.chunks(chunk["text"])
        adjusted: list[TextChunk] = [
            {
                "text": piece["text"],
                "start_byte": chunk["start_byte"] + piece["start_byte"],
                "end_byte": chunk["start_byte"] + piece["end_byte"],
            }
            for piece in pieces
        ]
        return [result for piece in adjusted for result in self._fit(piece)]


class Pipeline:
    """Compose shared chunking, caller-provided embeddings, and RetrievalKit."""

    def __init__(
        self,
        index: Index,
        *,
        embed: EmbeddingProvider,
        chunker: DocumentChunker | None = None,
        count_tokens: TokenCounter | None = None,
        max_tokens: int | None = None,
    ) -> None:
        self._index = index
        self._embed = embed
        selected_chunker: DocumentChunker = chunker or RustTextChunker(
            max_characters=500,
            overlap_characters=50,
            strategy="sentence",
        )
        if (count_tokens is None) != (max_tokens is None):
            raise PipelineConfigurationError(
                "count_tokens and max_tokens must be provided together"
            )
        if count_tokens is not None and max_tokens is not None:
            selected_chunker = TokenAwareDocumentChunker(
                selected_chunker,
                count_tokens=count_tokens,
                max_tokens=max_tokens,
            )
        self._chunker = selected_chunker

    def add(
        self,
        document_id: str,
        text: str,
        *,
        metadata: Metadata | None = None,
    ) -> AddDocumentResult:
        """Chunk and embed a document before replacing its indexed chunks."""

        chunks = self._chunker.chunks(text)
        if not chunks:
            raise EmptyDocumentError(
                f"Document '{document_id}' produced no chunks. "
                "Return at least one non-empty source slice."
            )
        self._validate_chunks(chunks, source=text)

        embeddings = self._embed([chunk["text"] for chunk in chunks])
        if len(embeddings) != len(chunks):
            raise EmbeddingCountMismatchError(
                "Embedding provider returned "
                f"{len(embeddings)} embeddings for {len(chunks)} chunks. "
                "Return exactly one embedding per input text."
            )

        chunk_inputs: list[ChunkInput] = []
        for position, (chunk, embedding) in enumerate(zip(chunks, embeddings)):
            self._validate_embedding(embedding)
            chunk_inputs.append(
                {
                    "text": chunk["text"],
                    "embedding": embedding,
                    "metadata": {
                        "retrievalkit.chunk.index": position,
                        "retrievalkit.chunk.start_byte": chunk["start_byte"],
                        "retrievalkit.chunk.end_byte": chunk["end_byte"],
                    },
                }
            )

        results = self._index.add(
            documents=[
                {
                    "id": document_id,
                    "metadata": metadata or {},
                    "chunks": chunk_inputs,
                }
            ]
        )
        return results[0]

    def search(
        self,
        text: str,
        *,
        limit: int = 10,
        where: Filter | None = None,
        vector_candidates: int | None = None,
        keyword_candidates: int | None = None,
        alpha: float = 0.6,
    ) -> list[HybridHit]:
        """Embed text and run hybrid vector plus BM25 search."""

        embeddings = self._embed([text])
        if len(embeddings) != 1:
            raise EmbeddingCountMismatchError(
                "Embedding provider returned "
                f"{len(embeddings)} embeddings for one query. "
                "Return exactly one embedding per input text."
            )
        embedding = embeddings[0]
        self._validate_embedding(embedding)
        return self._index.hybrid_search(
            text,
            embedding,
            limit=limit,
            where=where,
            vector_candidates=vector_candidates,
            keyword_candidates=keyword_candidates,
            alpha=alpha,
        )

    def _validate_embedding(self, embedding: Sequence[float]) -> None:
        if len(embedding) != self._index.dimension:
            raise EmbeddingDimensionMismatchError(
                "Embedding dimension mismatch: "
                f"expected {self._index.dimension}, got {len(embedding)}. "
                "Use the same embedding model for indexing and queries."
            )

    @staticmethod
    def _validate_chunks(chunks: Sequence[TextChunk], *, source: str) -> None:
        source_bytes = source.encode("utf-8")
        previous_start: int | None = None

        for position, chunk in enumerate(chunks):
            chunk_text_value = chunk["text"]
            start = chunk["start_byte"]
            end = chunk["end_byte"]
            if not chunk_text_value.strip():
                raise InvalidChunkError(
                    f"Chunk {position} is empty. Return non-whitespace source text."
                )
            if start < 0 or end < start or end > len(source_bytes):
                raise InvalidChunkError(
                    f"Chunk {position} has invalid UTF-8 range {start}..<{end}; "
                    f"expected 0...{len(source_bytes)} with start_byte <= end_byte."
                )
            if previous_start is not None and start < previous_start:
                raise InvalidChunkError(
                    f"Chunk {position} starts before chunk {position - 1}. "
                    "Return chunks in source order."
                )
            try:
                source_slice = source_bytes[start:end].decode("utf-8")
            except UnicodeDecodeError as error:
                raise InvalidChunkError(
                    f"Chunk {position} range cuts through a UTF-8 character. "
                    "Return offsets on character boundaries."
                ) from error
            if source_slice != chunk_text_value:
                raise InvalidChunkError(
                    f"Chunk {position} text does not match its source byte range. "
                    "Return the exact source slice or correct the offsets."
                )
            previous_start = start


__all__ = [
    "DocumentChunker",
    "EmbeddingCountMismatchError",
    "EmbeddingDimensionMismatchError",
    "EmbeddingProvider",
    "EmptyDocumentError",
    "InvalidChunkError",
    "Pipeline",
    "PipelineConfigurationError",
    "PipelineError",
    "TokenAwareDocumentChunker",
    "TokenCounter",
    "TokenLimitError",
]
