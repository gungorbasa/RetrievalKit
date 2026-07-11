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
        .binaryTarget(
            name: "VectorKitFFI",
            path: "../../../target/apple/VectorKitFFI.xcframework"
        ),
        .binaryTarget(
            name: "VectorKitGraphFFI",
            path: "../../../target/apple/VectorKitGraphFFI.xcframework"
        ),
        .target(
            name: "VectorKit",
            dependencies: ["VectorKitFFI"]
        ),
        .target(
            name: "VectorKitIngest",
            dependencies: ["VectorKitFFI"]
        ),
        .target(
            name: "VectorKitGraph",
            dependencies: ["VectorKitGraphFFI"]
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
