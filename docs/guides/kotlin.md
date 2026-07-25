# Kotlin/JVM and Android Guide

RetrievalKit has separate repository-local base and graph artifacts:

- Kotlin/JVM: `retrievalkit` and `retrievalkit-graph`.
- Android: `retrievalkit-android` and `retrievalkit-graph-android`.

The coordinates are provisional. Choose exactly one artifact per application;
the graph artifact already contains retrieval. The initial Android target is
API 24+ on `arm64-v8a`; Kotlin Multiplatform and other Android ABIs are not
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
it. Use JDK 17 with the pinned Gradle/Android plugin toolchain. The wrapper
README supplies the exact host and Android NDK commands. After the native
libraries are present, run:

```bash
cd wrappers/kotlin
./gradlew :base:test :graph:test
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
