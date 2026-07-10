// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "VectorKitBench",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "vectorkit-bench", targets: ["VectorKitBench"])
    ],
    dependencies: [
        .package(path: "../EmbeddingKit")
    ],
    targets: [
        .systemLibrary(
            name: "CVectorKitFFI",
            path: "Sources/CVectorKitFFI"
        ),
        .executableTarget(
            name: "VectorKitBench",
            dependencies: ["CVectorKitFFI", "EmbeddingKit"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/release/libvectorkit_ffi.a"
                ])
            ]
        ),
    ]
)
