import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.io.ByteArrayInputStream
import java.util.zip.ZipFile
import java.util.zip.ZipInputStream

plugins {
    id("com.android.library")
    kotlin("android")
    `maven-publish`
}

group = providers.gradleProperty("retrievalkitMavenGroup").orElse("io.github.gungorbasa").get()
version = providers.gradleProperty("retrievalkitVersion").orElse("0.1.0").get()

android {
    namespace = "ai.retrievalkit.embedding"
    compileSdk = 34
    defaultConfig {
        minSdk = 24
        ndk {
            abiFilters += "arm64-v8a"
        }
        consumerProguardFiles("consumer-rules.pro")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
    sourceSets["main"].apply {
        java.srcDir("../embedding/src/main/kotlin")
        jniLibs.srcDir("build/generated/jniLibs")
        resources.srcDir("build/generated/resources")
    }
    publishing {
        singleVariant("release") {
            withSourcesJar()
            withJavadocJar()
        }
    }
    packaging {
        resources.pickFirsts += setOf("LICENSE", "NOTICE")
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_11)
        allWarningsAsErrors.set(true)
    }
}

tasks.register("inspectEmbeddingAar") {
    dependsOn("assembleRelease")
    doLast {
        val aar = layout.buildDirectory.file("outputs/aar/android-embedding-release.aar").get().asFile
        val (aarEntries, classEntries) = ZipFile(aar).use { archive ->
            val outer = archive.entries().asSequence().map { it.name }.toSet()
            val classes = archive.getEntry("classes.jar")
                ?: error("embedding AAR is missing classes.jar")
            val nestedEntries = ZipInputStream(
                ByteArrayInputStream(archive.getInputStream(classes).readBytes()),
            ).use { nested ->
                buildSet {
                    while (true) {
                        val entry = nested.nextEntry ?: break
                        add(entry.name)
                    }
                }
            }
            outer to nestedEntries
        }
        check("jni/arm64-v8a/libretrievalkit_embedding_jni.so" in aarEntries) {
            "embedding AAR is missing the Android arm64-v8a JNI library"
        }
        check("jni/arm64-v8a/libonnxruntime.so" in aarEntries) {
            "embedding AAR is missing ONNX Runtime for Android arm64-v8a"
        }
        val requiredClassResources = setOf(
            "LICENSE",
            "NOTICE",
            "ONNX-Runtime-LICENSE",
            "ONNX-Runtime-ThirdPartyNotices.txt",
            "runtime-identity.txt",
        )
        check(requiredClassResources.all(classEntries::contains)) {
            "embedding AAR classes.jar is missing required legal/runtime resources: " +
                (requiredClassResources - classEntries)
        }
        check(classEntries.none { it.contains("RetrievalDatabase") || it.contains("GraphDatabase") }) {
            "embedding AAR unexpectedly contains retrieval or graph classes"
        }
    }
}

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                artifactId = "retrievalkit-embedding-android"
                pom {
                    name.set("RetrievalKit Embedding for Android arm64-v8a")
                    description.set("Preview FP32 all-MiniLM-L6-v2 embedding AAR for Android arm64-v8a")
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
}
