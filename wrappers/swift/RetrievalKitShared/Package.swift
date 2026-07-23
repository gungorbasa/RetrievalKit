// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "RetrievalKitShared",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [.library(name: "RetrievalKitShared", targets: ["RetrievalKitShared"])],
    targets: [
        .target(name: "RetrievalKitShared"),
        .testTarget(name: "RetrievalKitSharedTests", dependencies: ["RetrievalKitShared"]),
    ],
    swiftLanguageModes: [.v6]
)
