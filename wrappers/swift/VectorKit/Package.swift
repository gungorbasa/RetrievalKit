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
        .binaryTarget(
            name: "VectorKitFFI",
            path: "../../../target/apple/VectorKitFFI.xcframework"
        ),
        .target(
            name: "VectorKit",
            dependencies: ["VectorKitFFI"]
        ),
        .target(
            name: "VectorKitIngest",
            dependencies: ["VectorKitFFI"]
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
