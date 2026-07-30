pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
        google()
    }
    resolutionStrategy {
        eachPlugin {
            when (requested.id.id) {
                "com.android.library" ->
                    useModule("com.android.tools.build:gradle:${requested.version ?: "8.7.0"}")
                "org.jetbrains.kotlin.android" ->
                    useModule("org.jetbrains.kotlin:kotlin-gradle-plugin:${requested.version ?: "1.9.22"}")
            }
        }
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        mavenCentral()
        google()
    }
}

rootProject.name = "retrievalkit-kotlin"
include(":base", ":graph", ":embedding", ":android-base", ":android-graph", ":android-embedding")
include(":example-retrieval", ":example-graph")
project(":example-retrieval").projectDir = file("examples/base")
project(":example-graph").projectDir = file("examples/graph")
