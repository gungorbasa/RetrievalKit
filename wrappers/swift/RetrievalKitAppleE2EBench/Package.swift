// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "RetrievalKitAppleE2EBench",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [
        .library(
            name: "RetrievalKitAppleE2EBenchmarkCore",
            targets: ["RetrievalKitAppleE2EBenchmarkCore"]
        ),
        .executable(name: "retrievalkit-apple-e2e", targets: ["RetrievalKitAppleE2EBench"]),
    ],
    dependencies: [
        .package(path: "../EmbeddingKit"),
        .package(path: "../RetrievalKit"),
    ],
    targets: [
        .target(
            name: "RetrievalKitAppleE2EBenchmarkCore",
            dependencies: [
                .product(name: "EmbeddingKit", package: "EmbeddingKit"),
                .product(name: "RetrievalKit", package: "RetrievalKit"),
                .product(name: "RetrievalKitRuntimeDiagnostics", package: "RetrievalKit"),
            ]
        ),
        .executableTarget(
            name: "RetrievalKitAppleE2EBench",
            dependencies: ["RetrievalKitAppleE2EBenchmarkCore"]
        ),
        .testTarget(
            name: "RetrievalKitAppleE2EBenchmarkCoreTests",
            dependencies: ["RetrievalKitAppleE2EBenchmarkCore"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
