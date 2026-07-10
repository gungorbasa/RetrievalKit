// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "VectorKit",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "VectorKit", targets: ["VectorKit"]),
        .library(name: "VectorKitIngest", targets: ["VectorKitIngest"])
    ],
    targets: [
        .systemLibrary(
            name: "VectorKitFFI",
            path: "Sources/CVectorKitFFI"
        ),
        .target(
            name: "VectorKit",
            dependencies: ["VectorKitFFI"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/debug/libvectorkit_ffi.a"
                ])
            ]
        ),
        .target(
            name: "VectorKitIngest",
            dependencies: ["VectorKitFFI"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/debug/libvectorkit_ffi.a"
                ])
            ]
        ),
        .testTarget(
            name: "VectorKitTests",
            dependencies: ["VectorKit"]
        ),
        .testTarget(
            name: "VectorKitIngestTests",
            dependencies: ["VectorKitIngest"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
