# Kotlin/JVM and Android Guide

RetrievalKit has separate repository-local base and graph artifacts:

- Kotlin/JVM: `retrievalkit` and `retrievalkit-graph`.
- Android: `retrievalkit-android` and `retrievalkit-graph-android`.

The coordinates are provisional. Choose exactly one artifact per application;
the graph artifact already contains retrieval. The initial Android target is
API 24+ on `arm64-v8a`; Kotlin Multiplatform and other Android ABIs are not
claimed.

## Installation status

The eventual Gradle dependency will have this shape:

```kotlin
dependencies {
    // PENDING — do not paste the placeholder group literally.
    implementation("<approved-group>:retrievalkit-graph:0.1.0")
}
```

For Android, the graph artifact will use
`<approved-group>:retrievalkit-graph-android:0.1.0`. Base applications will use
`retrievalkit` on JVM or `retrievalkit-android` on Android. The group ID is not
approved and none of these artifacts is published to a public Maven repository.
Choose exactly one base or graph artifact; graph already includes retrieval.

The available JVM route is the repository source build:

```bash
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
export PATH="$JAVA_HOME/bin:$PATH"
cd wrappers/kotlin
./scripts/preflight.sh jvm
./scripts/build-native.sh jvm
./gradlew :example-retrieval:run
```

The initial Kotlin/JVM native library target is macOS arm64. JDK 17 builds the
wrapper, while applications consuming the compiled bytecode may run Java 11+.
The initial Android target is API 24+ on arm64-v8a. Windows, Linux, other
desktop architectures, Kotlin Multiplatform, and other Android ABIs are not
claimed.

## Retrieval-only quickstart

Kotlin uses blocking, typed JNI calls. Run build, persistence, and search work
on an application-selected background executor or coroutine dispatcher on
Android. Embeddings use `FloatArray`, and Rust infers dimension from the first
upsert.

```kotlin
import ai.retrievalkit.Document
import ai.retrievalkit.MetadataValue
import ai.retrievalkit.RetrievalDatabase

val database = RetrievalDatabase.Builder(corpusId = "apollo").use { builder ->
    builder.upsert(
        Document(
            id = "decision-swift",
            text = "Apollo chose Swift for native platform integration.",
            metadata = mapOf(
                "project" to MetadataValue.Text("apollo"),
            ),
        ),
        floatArrayOf(1f, 0f, 0f),
    )
    builder.build()
}

database.use {
    val hits = it.search(
        text = "Why did we choose Swift?",
        embedding = floatArrayOf(1f, 0f, 0f),
        alpha = 0.6f,
        limit = 5,
    )
    println(hits.firstOrNull()?.documentId)
}
```

The search overloads form one family:

- `search(embedding = ...)` is exact vector search.
- `search(text = ...)` is BM25-only search.
- `search(text = ..., embedding = ..., alpha = ...)` is hybrid search.

Metadata integers and timestamps use exact signed `Long` values. Results expose
stable document/record identities rather than native chunk IDs.

## Graph-only and graph-scoped retrieval

`GraphDatabase.Builder` accepts ordinary typed `Record` values and needs no
retrieval configuration. `GraphRetrievalDatabase.Builder` accepts the same
record with either one direct embedding or a list of embedded documents.

```kotlin
val database = GraphRetrievalDatabase.Builder(
    corpusId = "apollo",
    schema = schema,
).use { builder ->
    builder.upsert(
        record = decisionRecord,
        embedding = floatArrayOf(1f, 0f, 0f),
    )
    builder.build()
}

database.use { db ->
    db.query(apolloQuery).use { selection ->
        val candidates = db.projectCandidates(selection)
        val hits = db.search(
            text = "native integration",
            embedding = floatArrayOf(1f, 0f, 0f),
            alpha = 0.6f,
            within = selection,
        )
    }
}
```

Graph selections are opaque `AutoCloseable` resources. Rust owns schema
validation, traversal, full typed path edges and provenance, projection
filtering, lexical order, stale/cross-corpus rejection, and ranking.

## Build and verify from source

The JNI crate is compiled twice: base without the `graph` feature and graph with
it. **JDK 17 is the build toolchain requirement:** it runs Gradle and compiles
the wrapper. **Java 11 is the produced bytecode/runtime target:** applications
consuming the already-built JVM artifact may use Java 11 or newer. A Java 11
runtime—or a newer non-JDK-17 installation such as Java 25—does not replace the
JDK 17 required to build from source.

On macOS, select an installed JDK 17 and verify the selected binary before
running the preflight. When `JAVA_HOME` is set, the preflight checks that exact
`$JAVA_HOME/bin/java`, reports required and detected Java, Rust, host, and (for
Android) NDK values, then exits with installation and selection commands before
a build if they do not match:

```bash
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
export PATH="$JAVA_HOME/bin:$PATH"
"$JAVA_HOME/bin/java" -version
cd wrappers/kotlin
./scripts/preflight.test.sh
./scripts/preflight.sh jvm
./scripts/build-native.sh jvm
./gradlew :base:test :graph:test
./gradlew :example-retrieval:run
```

To build the Android arm64-v8a packages, install Rust's Android target and NDK
26, then run:

```bash
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/26.1.10909125"
./scripts/preflight.sh android
./scripts/build-native.sh android
./gradlew :android-base:assembleRelease :android-graph:assembleRelease
```

Inspect both AARs before consumption: the base artifact must contain only the
base aggregate, the graph artifact must contain only the graph aggregate, each
under `jni/arm64-v8a/`, and both must carry `LICENSE` and `NOTICE`.

See [`wrappers/kotlin/README.md`](../../wrappers/kotlin/README.md) for native
paths, Gradle properties, artifact inspection, and lifecycle details.

The three compiling source examples are in:

- `wrappers/kotlin/examples/base` for retrieval only.
- `wrappers/kotlin/examples/graph/.../GraphOnly.kt` for graph only.
- `wrappers/kotlin/examples/graph/.../GraphAndRetrieval.kt` for combined use.

The wrapper README gives the corresponding `:examples:*:run` commands.
