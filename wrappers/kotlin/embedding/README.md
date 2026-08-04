# RetrievalKit Embedding for Kotlin/JVM

> [RetrievalKit](../../../README.md) › SDKs › Kotlin embedding

```kotlin
implementation("io.github.gungorbasa:retrievalkit-embedding:0.1.0")
```

This optional package provides blocking FP32 `all-MiniLM-L6-v2` text
embeddings. It is separate from the RetrievalKit database package: embedding
produces 384-value, finite, L2-normalized `FloatArray` values, while database
storage may independently use RetrievalKit's signed-I8 encoding.

## Quickstart

```kotlin
OnnxEmbedder.prefetch()

OnnxEmbedder.load(localOnly = true).use { embedder ->
    val query: FloatArray = embedder.embed("local semantic search")
}
```

## Acquisition and offline behavior

Only `load(...)` and `prefetch(...)` may acquire model files. `localOnly = true`
prohibits network acquisition. An application may pass `cacheDirectory` for
its own cache placement. The macOS arm64 JAR includes the verified official
ONNX Runtime 1.24.3 dynamic library; `runtimeLibrary` remains available for an
explicit application-managed copy with the same exact size and SHA-256.
Inference never performs model download or cache resolution.

The package pins `gungorbasa/retrievalkit-minilm` commit
`617ce926c1f9e0289365d3e999474cc28b1645d4` and manifest SHA-256
`b81e0e9393a25630eda184cfa373f2f28eed08c2ed92ae3d4097504e5f7ab4b2`.
Empty or blank text and empty batches fail with typed input errors.

## Lifecycle and runtime

Calls on one embedder are serialized. The API deliberately has no coroutine
dependency; applications choose the appropriate dispatcher for blocking
loading and inference. `close()` is deterministic and idempotent, and later
operations throw `ClosedEmbedderException`.

The JVM artifact expects the `retrievalkit_embedding_jni` native aggregate as a
packaged macOS arm64 resource or through
`-Dretrievalkit.embedding.native.path=/absolute/path/to/library`.

The `io.github.gungorbasa:retrievalkit-embedding:0.1.0` preview is available
from Maven Central for the qualified macOS arm64 target.
