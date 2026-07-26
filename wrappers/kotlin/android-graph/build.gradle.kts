import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    kotlin("android")
    `maven-publish`
}

android {
    namespace = "ai.retrievalkit.graph"
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
        java.srcDir("../shared/src/main/kotlin")
        java.srcDir("../graph/src/main/kotlin")
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

tasks.register("inspectGraphAar") {
    dependsOn("assembleRelease")
    doLast {
        val aar = layout.buildDirectory.file("outputs/aar/android-graph-release.aar").get().asFile
        val names = zipTree(aar).files.map { it.invariantSeparatorsPath }
        check(names.any { it.endsWith("jni/arm64-v8a/libretrievalkit_jni_graph.so") }) {
            "graph AAR is missing Android arm64-v8a JNI library"
        }
        check(names.none { it.endsWith("jni/arm64-v8a/libretrievalkit_jni.so") }) {
            "graph AAR unexpectedly contains the base native aggregate"
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_11)
        allWarningsAsErrors.set(true)
    }
}

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("release") {
                from(components["release"])
                artifactId = "retrievalkit-graph-android"
                pom {
                    name.set("RetrievalKit Graph Android arm64-v8a")
                    description.set("Optional local graph and graph-scoped retrieval for Android arm64-v8a")
                }
            }
        }
    }
}
