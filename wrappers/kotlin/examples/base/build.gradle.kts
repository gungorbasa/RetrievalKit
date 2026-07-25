plugins {
    kotlin("jvm")
    application
}

dependencies {
    implementation(project(":base"))
}

application {
    mainClass.set("examples.retrieval.RetrievalOnlyKt")
}

kotlin {
    jvmToolchain(17)
}
