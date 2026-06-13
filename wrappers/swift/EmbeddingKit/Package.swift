// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "EmbeddingKit",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "EmbeddingKit", targets: ["EmbeddingKit"])
    ],
    targets: [
        .target(name: "EmbeddingKit"),
        .testTarget(
            name: "EmbeddingKitTests",
            dependencies: ["EmbeddingKit"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
