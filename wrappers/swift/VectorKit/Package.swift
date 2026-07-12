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
        .executable(name: "VectorKitRetrievalQuickstart", targets: ["VectorKitRetrievalQuickstart"])
    ],
    dependencies: [.package(path: "../VectorKitShared")],
    targets: [
        .binaryTarget(
            name: "VectorKitFFI",
            path: "../../../target/apple/VectorKitFFI.xcframework"
        ),
        .target(
            name: "VectorKit",
            dependencies: [
                "VectorKitFFI",
                .product(name: "VectorKitShared", package: "VectorKitShared")
            ]
        ),
        .target(
            name: "VectorKitIngest",
            dependencies: ["VectorKitFFI"]
        ),
        .executableTarget(
            name: "VectorKitRetrievalQuickstart",
            dependencies: ["VectorKit"]
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
