// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "RetrievalKitGraph",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [
        .library(name: "RetrievalKitGraph", targets: ["RetrievalKitGraph"]),
        .executable(name: "RetrievalKitGraphQuickstart", targets: ["RetrievalKitGraphQuickstart"]),
        .executable(name: "RetrievalKitGraphRetrievalQuickstart", targets: ["RetrievalKitGraphRetrievalQuickstart"]),
    ],
    dependencies: [.package(path: "../RetrievalKitShared")],
    targets: [
        .binaryTarget(name: "RetrievalKitGraphFFI", path: "../../../target/apple/RetrievalKitGraphFFI.xcframework"),
        .target(name: "RetrievalKitGraph", dependencies: [
            "RetrievalKitGraphFFI",
            .product(name: "RetrievalKitShared", package: "RetrievalKitShared"),
        ]),
        .executableTarget(name: "RetrievalKitGraphQuickstart", dependencies: ["RetrievalKitGraph"]),
        .executableTarget(name: "RetrievalKitGraphRetrievalQuickstart", dependencies: ["RetrievalKitGraph"]),
        .testTarget(name: "RetrievalKitGraphTests", dependencies: ["RetrievalKitGraph"]),
    ],
    swiftLanguageModes: [.v6]
)
