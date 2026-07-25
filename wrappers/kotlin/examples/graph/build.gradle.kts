import org.gradle.api.tasks.JavaExec

plugins {
    kotlin("jvm")
    application
}

dependencies {
    implementation(project(":graph"))
}

application {
    mainClass.set("examples.graph.GraphOnlyKt")
}

kotlin {
    jvmToolchain(17)
}

tasks.register<JavaExec>("runCombined") {
    group = "application"
    classpath = sourceSets["main"].runtimeClasspath
    mainClass.set("examples.graph.GraphAndRetrievalKt")
}
