# RetrievalKit for Kotlin

RetrievalKit exposes the Rust-owned local retrieval and graph databases through
an idiomatic, blocking Kotlin/JVM API and a thin typed JNI boundary. The initial
Android package contains an `arm64-v8a` native library. Kotlin Multiplatform,
browsers, servers, and other Android ABIs are not claimed.

The repository-local Maven coordinates are provisional:

```text
local.retrievalkit:retrievalkit:0.1.0
local.retrievalkit:retrievalkit-graph:0.1.0
local.retrievalkit:retrievalkit-android:0.1.0
local.retrievalkit:retrievalkit-graph-android:0.1.0
```

They do not imply that artifacts are available from a public Maven registry.

## API

`RetrievalDatabase`, `GraphDatabase`, and `GraphRetrievalDatabase` implement
`AutoCloseable`, so use Kotlin `use` to release native state deterministically.
Builders infer vector dimension from the first non-empty `FloatArray`. The
overloaded `search` family supports embedding-only exact search, text-only BM25,
and text-plus-embedding fusion:

```kotlin
RetrievalDatabase.Builder("notes").use { builder ->
    builder.upsert(Document("one", "Local search"), floatArrayOf(1f, 0f))
    builder.build().use { database ->
        database.search(floatArrayOf(1f, 0f))
        database.search("local") // alpha = 0, no embedding required
        database.search("local", floatArrayOf(1f, 0f), alpha = 0.6f)
    }
}
```

Base builders accept `Iterable<EmbeddedDocument>`, graph-only builders accept
`Iterable<Record>`, and combined builders accept
`Iterable<GraphRecordInput>` for bulk ingestion without keyed embedding maps.

`alpha = 1` uses vector-only candidate generation, `alpha = 0` uses BM25-only
candidate generation, and intermediate values use both. Rust owns alpha
validation, filtering, ranking, stable identities, graph traversal, generation
checks, candidate projection, and persistence. Search, filters, graph queries,
projection, and results cross JNI as typed values; they do not use JSON,
reflection, or serialization.

## Lifecycle and threading

Methods are synchronous and may perform CPU or disk work. RetrievalKit does not
add a coroutine dependency; Android callers choose their own dispatcher or
executor and must not call build, search, save, or load on the UI thread.

Each database, builder, and selection owns one private native handle. Native
operations on the same handle are serialized. Independent databases can run in
parallel. `close()` removes the handle, waits for an active operation on that
resource, and is idempotent through the Kotlin owner. A later call fails with
`ClosedResourceException`. A graph selection is immutable and can safely scope
a combined database query until it is closed.

Built Kotlin capability databases are immutable, so the public API cannot
mutate a generation beneath an existing selection. Stale-generation validation
remains Rust-owned and is exercised by native/core conformance; Kotlin
projection tests additionally prove cross-corpus rejection. Projection lexical
ordering and metadata intersection are verified through the shared graph
fixture.

Rust error categories map to typed Kotlin exceptions, including
`InvalidIdentityException`, `InvalidDimensionException`,
`MissingEmbeddingException`, `InvalidQueryException`,
`PersistenceException`, `CorruptIndexException`,
`InvalidGraphSchemaException`, and `StaleSelectionException`. Messages retain
the actual value, expected value, and corrective action supplied by Rust.

## Capability-separated artifacts

The base JAR/AAR loads only `retrievalkit_jni`. The graph-capable JAR/AAR loads
only `retrievalkit_jni_graph`, which contains graph plus retrieval entry points.
An application must choose one aggregate and must not load both in one process.
The base native crate is built without the Cargo `graph` feature and has no
`retrievalkit-graph` dependency.

## Build and test

The checked-in Gradle wrapper pins Gradle 8.10.2. Use JDK 17 for the build
toolchain; the produced JVM bytecode targets Java 11. On macOS, select JDK 17
and run the preflight before compiling:

```bash
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
export PATH="$JAVA_HOME/bin:$PATH"
./scripts/preflight.sh jvm
./scripts/build-native.sh jvm
./gradlew :base:test :graph:test
./gradlew :example-retrieval:run
./gradlew :example-graph:run
./gradlew :example-graph:runCombined
```

The three examples cover retrieval-only, graph-only, and graph-scoped
retrieval. They use progressive Rust builders and do not expose native handles,
internal chunk IDs, or keyed embedding maps.

The preflight prints required and detected Java, Rust, and host values.
`build-native.sh` invokes it again and stops with a corrective JDK message
instead of passing an unsupported Java version to Gradle.

Android requires Rust's `aarch64-linux-android` standard library plus Android
NDK 26. The script defaults to the standard macOS SDK location and accepts
`ANDROID_NDK_HOME` when the NDK is elsewhere:

```bash
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/26.1.10909125"
./scripts/preflight.sh android
./scripts/build-native.sh android
./gradlew :android-base:assembleRelease :android-graph:assembleRelease
./gradlew :android-base:inspectBaseAar :android-graph:inspectGraphAar
./gradlew :base:inspectBaseArtifact
```

Produced AARs are:

```text
android-base/build/outputs/aar/android-base-release.aar
android-graph/build/outputs/aar/android-graph-release.aar
```

The inspection tasks fail unless each AAR has exactly its intended
`jni/arm64-v8a` aggregate, and the base artifact inspection fails if graph
classes or the graph native library are present. `LICENSE` and `NOTICE` are
included in JARs and generated native-resource trees.
