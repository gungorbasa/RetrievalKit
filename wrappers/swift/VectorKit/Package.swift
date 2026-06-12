// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "VectorKit",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "VectorKit", targets: ["VectorKit"])
    ],
    targets: [
        .systemLibrary(
            name: "CVectorKitFFI",
            path: "Sources/CVectorKitFFI"
        ),
        .target(
            name: "VectorKit",
            dependencies: ["CVectorKitFFI"],
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
    ]
)
