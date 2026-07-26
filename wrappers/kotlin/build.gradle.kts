import org.gradle.api.publish.PublishingExtension
import org.gradle.api.publish.maven.MavenPublication
import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    kotlin("jvm") version "1.9.22" apply false
    id("com.android.library") version "8.7.0" apply false
}

val publicationGroup = providers.gradleProperty("retrievalkitMavenGroup")
    .orElse("local.retrievalkit")
val publicationVersion = providers.gradleProperty("retrievalkitVersion")
    .orElse("0.1.0")

allprojects {
    group = publicationGroup.get()
    version = publicationVersion.get()
}

subprojects {
    tasks.withType<AbstractArchiveTask>().configureEach {
        isPreserveFileTimestamps = false
        isReproducibleFileOrder = true
    }

    pluginManager.withPlugin("maven-publish") {
        extensions.configure<PublishingExtension> {
            publications.withType(MavenPublication::class.java).configureEach {
                pom {
                    url.set("https://retrievalkit-docs.gungorbasa.chatgpt.site")
                    licenses {
                        license {
                            name.set("Apache License, Version 2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                            distribution.set("repo")
                        }
                    }
                    developers {
                        developer {
                            id.set("eggyolk-yazilim")
                            name.set("EGGYOLK YAZILIM TİCARET LİMİTED ŞİRKETİ")
                            organization.set("EGGYOLK YAZILIM TİCARET LİMİTED ŞİRKETİ")
                        }
                    }
                    scm {
                        connection.set("scm:git:https://github.com/gungorbasa/RetrievalKit.git")
                        developerConnection.set("scm:git:ssh://git@github.com/gungorbasa/RetrievalKit.git")
                        url.set("https://github.com/gungorbasa/RetrievalKit")
                    }
                }
            }

            providers.gradleProperty("retrievalkitMavenRepository").orNull?.let { repository ->
                repositories {
                    maven {
                        name = "release"
                        url = uri(repository)
                    }
                }
            }
        }
    }
}
