package ai.retrievalkit.internal

import ai.retrievalkit.NativeLibraryException
import java.nio.file.Files
import java.nio.file.StandardCopyOption

internal object NativeLibraryLoader {
    private val loaded = mutableSetOf<String>()

    @Synchronized
    fun load(name: String) {
        if (!loaded.add(name)) return
        try {
            val explicit = System.getProperty("retrievalkit.native.path")
            if (!explicit.isNullOrBlank()) {
                System.load(explicit)
                return
            }
            if (isAndroid()) {
                System.loadLibrary(name)
                return
            }
            val osName = System.getProperty("os.name").orEmpty()
            val osArch = System.getProperty("os.arch").orEmpty()
            val platform = when {
                osName.lowercase().contains("mac") &&
                    osArch in setOf("aarch64", "arm64") -> "macos-aarch64"
                osName.lowercase().contains("linux") &&
                    osArch in setOf("aarch64", "arm64") -> "linux-aarch64"
                else -> throw NativeLibraryException(
                    "unsupported JVM platform $osName $osArch; use Android arm64-v8a or supply " +
                        "-Dretrievalkit.native.path=/absolute/path/to/library",
                )
            }
            val mapped = System.mapLibraryName(name)
            val resource = "/native/$platform/$mapped"
            val stream = NativeLibraryLoader::class.java.getResourceAsStream(resource)
                ?: throw NativeLibraryException(
                    "native library resource $resource is missing; build the native aggregate " +
                        "or supply -Dretrievalkit.native.path",
                )
            val temporary = Files.createTempFile("retrievalkit-", mapped.substringAfterLast('.'))
            temporary.toFile().deleteOnExit()
            stream.use { Files.copy(it, temporary, StandardCopyOption.REPLACE_EXISTING) }
            System.load(temporary.toAbsolutePath().toString())
        } catch (error: NativeLibraryException) {
            loaded.remove(name)
            throw error
        } catch (error: Throwable) {
            loaded.remove(name)
            throw NativeLibraryException("could not load native library '$name': ${error.message}", error)
        }
    }

    private fun isAndroid(): Boolean =
        System.getProperty("java.vm.name")?.contains("Dalvik", ignoreCase = true) == true ||
            System.getProperty("java.runtime.name")?.contains("Android", ignoreCase = true) == true
}
