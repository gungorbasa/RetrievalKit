// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "VectorKitShared",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [.library(name: "VectorKitShared", targets: ["VectorKitShared"])],
    targets: [
        .target(name: "VectorKitShared"),
        .testTarget(name: "VectorKitSharedTests", dependencies: ["VectorKitShared"]),
    ],
    swiftLanguageModes: [.v6]
)
