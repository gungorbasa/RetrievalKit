# RetrievalKit for Kotlin

RetrievalKit exposes the Rust-owned local retrieval and graph databases through
an idiomatic, blocking Kotlin/JVM API and a thin typed JNI boundary. The initial
Android package contains an `arm64-v8a` native library and ships as an explicit
v0.1.0 preview. Its host-verifiable build, package, inventory, ABI/architecture,
JVM/JNI-contract, and fresh consumer resolution/compilation checks are
required and device-independent. Live-device inference, lifecycle,
compatibility, and performance are unqualified and deferred, and their absence
does not block v0.1.0. Kotlin
Multiplatform, browsers, servers, and other Android ABIs are not claimed.

The owner-approved Maven coordinates are:

```text
io.github.gungorbasa:retrievalkit:0.1.0
io.github.gungorbasa:retrievalkit-graph:0.1.0
io.github.gungorbasa:retrievalkit-android:0.1.0
io.github.gungorbasa:retrievalkit-graph-android:0.1.0
io.github.gungorbasa:retrievalkit-embedding:0.1.0
io.github.gungorbasa:retrievalkit-embedding-android:0.1.0
```

All six v0.1.0 preview artifacts are available from Maven Central.

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

The checked-in Gradle wrapper pins Gradle 8.10.2. These are two different Java
requirements:

- **Build toolchain:** JDK 17 is required to run Gradle and compile the wrapper.
- **Produced library:** JVM bytecode targets Java 11, so consuming applications
  may run the built JVM artifact on a Java 11+ runtime.

Having Java 11 or a newer non-LTS JDK such as Java 25 on `PATH` does not satisfy
the build requirement. On macOS, select an installed JDK 17, verify the actual
binary, and run the preflight before compiling:

```bash
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
export PATH="$JAVA_HOME/bin:$PATH"
"$JAVA_HOME/bin/java" -version
./scripts/preflight.test.sh
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

The preflight uses `$JAVA_HOME/bin/java` when `JAVA_HOME` is set, prints the
selected binary plus required and detected Java, Rust, and host values, and
explains the JDK 17 versus Java 11 distinction. `build-native.sh` invokes it
again and stops with installation, selection, and verification commands instead
of passing an unsupported Java version to Gradle.

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

## Assemble Maven release artifacts

The checked-in default is the owner-approved `io.github.gungorbasa` group.
Release assembly still requires the group explicitly and does not claim that
Central Portal has verified the namespace:

```bash
python3 ../../scripts/release/assemble_kotlin_packages.py \
  --group io.github.gungorbasa \
  --version 0.1.0 \
  --java-home "$JAVA_HOME" \
  --output ../../dist/release/kotlin
```

The assembler builds the macOS arm64 JVM and Android arm64-v8a native
aggregates, then creates six isolated publications:

```text
<group>:retrievalkit
<group>:retrievalkit-graph
<group>:retrievalkit-android
<group>:retrievalkit-graph-android
<group>:retrievalkit-embedding
<group>:retrievalkit-embedding-android
```

Every publication contains its main JAR or AAR, sources JAR, Javadoc JAR,
Apache-2.0 POM metadata, and MD5/SHA-1/SHA-256/SHA-512 checksum companions.
The output also includes an inventory and a deterministic Central Portal bundle.
Base artifacts are rejected if they contain graph classes or graph native code.

Android assembly and inspection do not claim physical-device qualification.
No live Android model acquisition, inference, lifecycle, memory, thermal,
offline-restart, compatibility, or performance pass exists for v0.1.0.

Central requires PGP signatures. Pass `--signing-key <gpg-key-id>` only from an
approved secret-bearing release environment, and pass `--namespace-verified`
only after the owner confirms control of the fixed namespace. Without those
assertions, the inventory records the exact blockers and reports
`publicationReady: false`.
Assembly never uploads or publishes. Run the deterministic package test with:

```bash
RETRIEVALKIT_JAVA_HOME="$JAVA_HOME" \
  python3 ../../scripts/release/test_assemble_kotlin_packages.py
```
