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
    targets: [
        .systemLibrary(
            name: "CVectorKitFFI",
            path: "Sources/CVectorKitFFI"
        ),
        .executableTarget(
            name: "VectorKitBench",
            dependencies: ["CVectorKitFFI"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/release/libvectorkit_ffi.a"
                ])
            ]
        ),
    ]
)
