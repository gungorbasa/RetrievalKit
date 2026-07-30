package ai.retrievalkit.embedding.internal

import ai.retrievalkit.embedding.NativeLibraryException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.attribute.PosixFilePermission
import java.security.MessageDigest

internal object NativeLibraryLoader {
    private val loaded = mutableSetOf<String>()
    private var extractedDirectory: Path? = null

    @Synchronized
    fun load(name: String) {
        if (name in loaded) return
        try {
            val explicit = System.getProperty("retrievalkit.embedding.native.path")
            if (isAndroid()) {
                System.loadLibrary(ANDROID_RUNTIME_LIBRARY_NAME)
                if (explicit.isNullOrBlank()) {
                    System.loadLibrary(name)
                } else {
                    loadExplicit(Path.of(explicit))
                }
            } else if (!explicit.isNullOrBlank()) {
                loadExplicit(Path.of(explicit))
            } else {
                loadPackaged(name)
            }
            loaded.add(name)
        } catch (error: NativeLibraryException) {
            throw error
        } catch (error: Throwable) {
            throw NativeLibraryException(
                "could not load embedding native library '$name': ${error.message}",
                error,
            )
        }
    }

    @Synchronized
    fun resolveRuntimeLibrary(explicitPath: String?): String {
        if (!explicitPath.isNullOrBlank()) {
            val runtime = Path.of(explicitPath).toAbsolutePath().normalize()
            if (!Files.isRegularFile(runtime)) {
                throw NativeLibraryException(
                    "runtimeLibrary must identify a regular file: $runtime",
                )
            }
            if (isAndroid()) {
                verifyRuntime(
                    runtime,
                    ANDROID_RUNTIME_SIZE,
                    ANDROID_RUNTIME_SHA256,
                    deleteOnFailure = false,
                )
            } else {
                verifyRuntime(
                    runtime,
                    MACOS_RUNTIME_SIZE,
                    MACOS_RUNTIME_SHA256,
                    deleteOnFailure = false,
                )
            }
            return runtime.toString()
        }
        if (isAndroid()) {
            // The app native-library directory is on Android's linker search
            // path after System.loadLibrary above. ort accepts this soname.
            return ANDROID_RUNTIME_FILENAME
        }
        if (extractedDirectory == null) {
            load("retrievalkit_embedding_jni")
        }
        val directory = extractedDirectory ?: Files.createTempDirectory("retrievalkit-embedding-runtime-")
            .also {
                it.toFile().deleteOnExit()
                extractedDirectory = it
            }
        val runtime = directory.resolve(MACOS_RUNTIME_FILENAME)
        if (Files.exists(runtime)) {
            verifyRuntime(
                runtime,
                MACOS_RUNTIME_SIZE,
                MACOS_RUNTIME_SHA256,
                deleteOnFailure = true,
            )
            return runtime.toAbsolutePath().toString()
        }
        val platform = platform()
        val resource = "/native/$platform/$MACOS_RUNTIME_FILENAME"
        val stream = NativeLibraryLoader::class.java.getResourceAsStream(resource)
            ?: throw NativeLibraryException(
                "ONNX Runtime resource $resource is missing; rebuild the embedding artifact " +
                    "or pass runtimeLibrary explicitly",
            )
        stream.use { Files.copy(it, runtime, StandardCopyOption.REPLACE_EXISTING) }
        runtime.toFile().deleteOnExit()
        setOwnerOnlyPermissions(runtime)
        verifyRuntime(
            runtime,
            MACOS_RUNTIME_SIZE,
            MACOS_RUNTIME_SHA256,
            deleteOnFailure = true,
        )
        return runtime.toAbsolutePath().toString()
    }

    private fun loadExplicit(path: Path) {
        val absolute = path.toAbsolutePath().normalize()
        if (!absolute.isAbsolute || !Files.isRegularFile(absolute)) {
            throw NativeLibraryException(
                "retrievalkit.embedding.native.path must identify a regular file: $absolute",
            )
        }
        System.load(absolute.toString())
    }

    private fun loadPackaged(name: String) {
        val platform = platform()
        val mapped = System.mapLibraryName(name)
        val resource = "/native/$platform/$mapped"
        val stream = NativeLibraryLoader::class.java.getResourceAsStream(resource)
            ?: throw NativeLibraryException(
                "native library resource $resource is missing; build the embedding native aggregate " +
                    "or supply -Dretrievalkit.embedding.native.path",
            )
        val directory = Files.createTempDirectory("retrievalkit-embedding-")
        directory.toFile().deleteOnExit()
        extractedDirectory = directory
        val temporary = directory.resolve(mapped)
        stream.use { Files.copy(it, temporary, StandardCopyOption.REPLACE_EXISTING) }
        temporary.toFile().deleteOnExit()
        setOwnerOnlyPermissions(temporary)
        System.load(temporary.toAbsolutePath().toString())
    }

    private fun platform(): String {
        val osName = System.getProperty("os.name").orEmpty()
        val osArch = System.getProperty("os.arch").orEmpty()
        return when {
            osName.contains("mac", ignoreCase = true) &&
                osArch in setOf("aarch64", "arm64") -> "macos-aarch64"
            else -> throw NativeLibraryException(
                "unsupported JVM platform $osName $osArch; use macOS arm64, Android arm64-v8a, " +
                    "or supply -Dretrievalkit.embedding.native.path=/absolute/path/to/library",
            )
        }
    }

    private fun verifyRuntime(
        path: Path,
        expectedSize: Long,
        expectedSha256: String,
        deleteOnFailure: Boolean,
    ) {
        val size = Files.size(path)
        if (size != expectedSize) {
            if (deleteOnFailure) Files.deleteIfExists(path)
            throw NativeLibraryException(
                "ONNX Runtime size mismatch: expected $expectedSize bytes, found $size",
            )
        }
        val digest = MessageDigest.getInstance("SHA-256")
        Files.newInputStream(path).use { input ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        }
        val sha256 = digest.digest().joinToString("") {
            (it.toInt() and 0xff).toString(16).padStart(2, '0')
        }
        if (sha256 != expectedSha256) {
            if (deleteOnFailure) Files.deleteIfExists(path)
            throw NativeLibraryException(
                "ONNX Runtime SHA-256 mismatch: expected $expectedSha256, found $sha256",
            )
        }
    }

    private fun setOwnerOnlyPermissions(path: Path) {
        try {
            Files.setPosixFilePermissions(
                path,
                setOf(
                    PosixFilePermission.OWNER_READ,
                    PosixFilePermission.OWNER_WRITE,
                    PosixFilePermission.OWNER_EXECUTE,
                ),
            )
        } catch (_: UnsupportedOperationException) {
            // Non-POSIX filesystems still get the securely-created temporary path.
        }
    }

    private fun isAndroid(): Boolean =
        System.getProperty("java.vm.name")?.contains("Dalvik", ignoreCase = true) == true ||
            System.getProperty("java.runtime.name")?.contains("Android", ignoreCase = true) == true

    private const val MACOS_RUNTIME_FILENAME = "libonnxruntime.1.24.3.dylib"
    private const val MACOS_RUNTIME_SIZE = 27_724_968L
    private const val MACOS_RUNTIME_SHA256 =
        "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"
    private const val ANDROID_RUNTIME_LIBRARY_NAME = "onnxruntime"
    private const val ANDROID_RUNTIME_FILENAME = "libonnxruntime.so"
    private const val ANDROID_RUNTIME_SIZE = 25_831_632L
    private const val ANDROID_RUNTIME_SHA256 =
        "4d2318b3849abb8862133d3068fc7e807ed8b2671cc6d83657fff2fcb9e1caad"
}
