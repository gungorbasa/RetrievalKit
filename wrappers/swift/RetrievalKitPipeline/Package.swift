// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "RetrievalKitPipeline",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "RetrievalKitPipeline", targets: ["RetrievalKitPipeline"]),
        .executable(name: "retrievalkit-pipeline-example", targets: ["RetrievalKitPipelineExample"])
    ],
    dependencies: [
        .package(path: "../RetrievalKit"),
        .package(path: "../EmbeddingKit")
    ],
    targets: [
        .target(
            name: "RetrievalKitPipeline",
            dependencies: [
                .product(name: "RetrievalKit", package: "RetrievalKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        ),
        .executableTarget(
            name: "RetrievalKitPipelineExample",
            dependencies: [
                "RetrievalKitPipeline",
                .product(name: "RetrievalKit", package: "RetrievalKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        ),
        .testTarget(
            name: "RetrievalKitPipelineTests",
            dependencies: [
                "RetrievalKitPipeline",
                .product(name: "RetrievalKit", package: "RetrievalKit"),
                .product(name: "EmbeddingKit", package: "EmbeddingKit")
            ]
        )
    ],
    swiftLanguageModes: [.v6]
)
