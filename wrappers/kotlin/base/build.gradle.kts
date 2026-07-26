import org.gradle.api.tasks.bundling.Jar
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    kotlin("jvm")
    `java-library`
    `maven-publish`
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_11)
        allWarningsAsErrors.set(true)
    }
    sourceSets {
        main {
            kotlin.srcDir("../shared/src/main/kotlin")
            kotlin.srcDir("src/main/kotlin")
            resources.srcDir("build/generated/resources")
        }
        test {
            kotlin.srcDir("../shared/src/test/kotlin")
        }
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
    testImplementation("com.fasterxml.jackson.core:jackson-databind:2.18.2")
}

tasks.test {
    useJUnitPlatform()
    systemProperty(
        "retrievalkit.native.path",
        providers.gradleProperty("retrievalkitBaseNative").orNull ?: ""
    )
    systemProperty("retrievalkit.repo.root", rootProject.file("../..").absolutePath)
}

tasks.withType<Jar>().configureEach {
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    from(rootProject.file("../../LICENSE"))
    from(rootProject.file("../../NOTICE"))
}

tasks.register("inspectBaseArtifact") {
    dependsOn("jar")
    doLast {
        val jar = tasks.named<Jar>("jar").get().archiveFile.get().asFile
        val names = zipTree(jar).files.map { it.invariantSeparatorsPath }
        check(names.none { it.contains("GraphDatabase") || it.contains("retrievalkit_jni_graph") }) {
            "base JAR unexpectedly contains graph classes or native code"
        }
    }
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            artifactId = "retrievalkit"
            pom {
                name.set("RetrievalKit Kotlin/JVM")
                description.set("Local-first exact, BM25, and hybrid retrieval for Kotlin/JVM on macOS arm64")
            }
        }
    }
}
