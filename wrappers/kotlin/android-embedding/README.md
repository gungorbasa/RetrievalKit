# RetrievalKit Embedding for Android

> [RetrievalKit](../../../README.md) › SDKs › Android embedding

```kotlin
implementation("io.github.gungorbasa:retrievalkit-embedding-android:0.1.0")
```

This AAR reuses the Kotlin API from the sibling `embedding` module and packages
the independent `retrievalkit_embedding_jni` aggregate for Android
`arm64-v8a`. It has no dependency on RetrievalKit's database AAR.

This is an explicit v0.1.0 preview. Cross-compilation, AAR packaging, closed
inventory, ABI/architecture, the JVM/JNI contract, and fresh consumer
resolution/compilation remain required and can run without a device.
Live-device model acquisition, inference, lifecycle, compatibility, and
performance are unqualified and deferred; they are not a v0.1.0 publication
blocker. Do not infer production or device compatibility from a successful
package check.

## Quickstart and cache ownership

Use `AndroidOnnxEmbedder.load(context)` or
`AndroidOnnxEmbedder.prefetch(context)` so verified model artifacts are placed
under the application cache. The returned `OnnxEmbedder` has the same blocking,
FP32-only API and deterministic `close()` behavior as Kotlin/JVM.

## Supported package surface

The AAR supports minSdk 24 and only `arm64-v8a`. It packages the official ONNX
Runtime 1.24.3 arm64 library after exact AAR and selected-library verification;
no runtime or model binary is committed as source.

## Build and inspect

For a source build, the native library must be placed at:

```text
build/generated/jniLibs/arm64-v8a/libretrievalkit_embedding_jni.so
```

Run `inspectEmbeddingAar` after producing the native aggregate to verify the
ABI, legal resources, and retrieval/graph exclusion. The
`io.github.gungorbasa:retrievalkit-embedding-android:0.1.0` preview is available
from Maven Central. Publication does not change the live-device qualification
limits above.
