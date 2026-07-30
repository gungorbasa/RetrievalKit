# RetrievalKit Embedding for Android

This AAR reuses the Kotlin API from the sibling `embedding` module and packages
the independent `retrievalkit_embedding_jni` aggregate for Android
`arm64-v8a`. It has no dependency on RetrievalKit's database AAR.

Use `AndroidOnnxEmbedder.load(context)` or
`AndroidOnnxEmbedder.prefetch(context)` so verified model artifacts are placed
under the application cache. The returned `OnnxEmbedder` has the same blocking,
FP32-only API and deterministic `close()` behavior as Kotlin/JVM.

The AAR supports minSdk 24 and only `arm64-v8a`. It packages the official ONNX
Runtime 1.24.3 arm64 library after exact AAR and selected-library verification;
no runtime or model binary is committed as source.

The native library must be placed at:

```text
build/generated/jniLibs/arm64-v8a/libretrievalkit_embedding_jni.so
```

Run `inspectEmbeddingAar` after producing the native aggregate to verify the
ABI, legal resources, and retrieval/graph exclusion. Coordinates remain
repository-local and provisional; this module does not authorize publication.
