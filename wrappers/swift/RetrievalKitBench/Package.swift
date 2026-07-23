// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "RetrievalKitBench",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "retrievalkit-bench", targets: ["RetrievalKitBench"])
    ],
    dependencies: [
        .package(path: "../EmbeddingKit")
    ],
    targets: [
        .systemLibrary(
            name: "CRetrievalKitFFI",
            path: "Sources/CRetrievalKitFFI"
        ),
        .executableTarget(
            name: "RetrievalKitBench",
            dependencies: ["CRetrievalKitFFI", "EmbeddingKit"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/release/libretrievalkit_ffi.a"
                ])
            ]
        ),
    ]
)
