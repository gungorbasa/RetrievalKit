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
        .library(name: "VectorKitIngest", targets: ["VectorKitIngest"]),
        .library(name: "VectorKitGraph", targets: ["VectorKitGraph"])
    ],
    targets: [
        .systemLibrary(
            name: "VectorKitFFI",
            path: "Sources/CVectorKitFFI"
        ),
        .systemLibrary(
            name: "VectorKitGraphFFI",
            path: "Sources/CVectorKitGraphFFI"
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
        .target(
            name: "VectorKitGraph",
            dependencies: ["VectorKitGraphFFI"],
            linkerSettings: [.unsafeFlags(["../../../target/debug/libvectorkit_ffi.a"])]
        ),
        .testTarget(
            name: "VectorKitTests",
            dependencies: ["VectorKit"]
        ),
        .testTarget(
            name: "VectorKitIngestTests",
            dependencies: ["VectorKitIngest"]
        ),
        .testTarget(
            name: "VectorKitGraphTests",
            dependencies: ["VectorKitGraph"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
