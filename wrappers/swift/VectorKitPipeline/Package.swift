// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "VectorKitPipeline",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "VectorKitPipeline", targets: ["VectorKitPipeline"]),
        .executable(name: "vectorkit-pipeline-example", targets: ["VectorKitPipelineExample"])
    ],
    dependencies: [
        .package(path: "../VectorKit"),
        .package(path: "../EmbeddingKit")
    ],
    targets: [
        .target(
            name: "VectorKitPipeline",
            dependencies: [
                .product(name: "VectorKit", package: "VectorKit"),
                .product(name: "VectorKitIngest", package: "VectorKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        ),
        .executableTarget(
            name: "VectorKitPipelineExample",
            dependencies: [
                "VectorKitPipeline",
                .product(name: "VectorKit", package: "VectorKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        ),
        .testTarget(
            name: "VectorKitPipelineTests",
            dependencies: [
                "VectorKitPipeline",
                .product(name: "VectorKit", package: "VectorKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        )
    ],
    swiftLanguageModes: [.v6]
)
