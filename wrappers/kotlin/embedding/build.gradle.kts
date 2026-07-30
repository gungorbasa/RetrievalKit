import org.gradle.api.tasks.bundling.Jar
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm")
    `java-library`
    `maven-publish`
}

group = providers.gradleProperty("retrievalkitMavenGroup").orElse("io.github.gungorbasa").get()
version = providers.gradleProperty("retrievalkitVersion").orElse("0.1.0").get()

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_11)
        allWarningsAsErrors.set(true)
    }
}

sourceSets {
    main {
        resources.srcDir("build/generated/resources")
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
    withSourcesJar()
    withJavadocJar()
}

dependencies {
    testImplementation(kotlin("test"))
}

tasks.test {
    useJUnitPlatform()
}

tasks.withType<Jar>().configureEach {
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    isPreserveFileTimestamps = false
    isReproducibleFileOrder = true
    from(project.file("../LICENSE"))
    from(project.file("../NOTICE"))
}

tasks.register("inspectEmbeddingArtifact") {
    dependsOn("jar")
    doLast {
        val jar = tasks.named<Jar>("jar").get().archiveFile.get().asFile
        val names = zipTree(jar).files.map { it.invariantSeparatorsPath }
        val required = setOf(
            "LICENSE",
            "NOTICE",
            "ONNX-Runtime-LICENSE",
            "ONNX-Runtime-ThirdPartyNotices.txt",
            "runtime-identity.txt",
            "native/macos-aarch64/libretrievalkit_embedding_jni.dylib",
            "native/macos-aarch64/libonnxruntime.1.24.3.dylib",
        )
        check(required.all { expected -> names.any { it.endsWith(expected) } }) {
            "embedding JAR is missing required native/runtime/legal content"
        }
        check(names.none { it.contains("RetrievalDatabase") || it.contains("GraphDatabase") }) {
            "embedding JAR unexpectedly contains retrieval or graph classes"
        }
    }
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "retrievalkit-embedding"
            pom {
                name.set("RetrievalKit Embedding for Kotlin/JVM")
                description.set("Verified FP32 all-MiniLM-L6-v2 embeddings for Kotlin/JVM")
                url.set("https://retrievalkit-docs.gungorbasa.chatgpt.site")
                licenses {
                    license {
                        name.set("Apache License, Version 2.0")
                        url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                        distribution.set("repo")
                    }
                }
            }
        }
    }
}
