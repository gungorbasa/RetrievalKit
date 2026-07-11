// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "VectorKitGraph",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [.library(name: "VectorKitGraph", targets: ["VectorKitGraph"])],
    targets: [
        .binaryTarget(name: "VectorKitGraphFFI", path: "../../../target/apple/VectorKitGraphFFI.xcframework"),
        .target(name: "VectorKitGraph", dependencies: ["VectorKitGraphFFI"]),
        .testTarget(name: "VectorKitGraphTests", dependencies: ["VectorKitGraph"]),
    ],
    swiftLanguageModes: [.v6]
)
