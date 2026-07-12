// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "VectorKitGraph",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [
        .library(name: "VectorKitGraph", targets: ["VectorKitGraph"]),
        .executable(name: "VectorKitGraphQuickstart", targets: ["VectorKitGraphQuickstart"]),
        .executable(name: "VectorKitGraphRetrievalQuickstart", targets: ["VectorKitGraphRetrievalQuickstart"]),
    ],
    dependencies: [.package(path: "../VectorKitShared")],
    targets: [
        .systemLibrary(name: "VectorKitGraphFFI", path: "Sources/CVectorKitGraphFFI"),
        .target(name: "VectorKitGraph", dependencies: ["VectorKitGraphFFI", .product(name: "VectorKitShared", package: "VectorKitShared")], linkerSettings: [.unsafeFlags(["../../../target/debug/libvectorkit_ffi.a"])]),
        .executableTarget(name: "VectorKitGraphQuickstart", dependencies: ["VectorKitGraph"]),
        .executableTarget(name: "VectorKitGraphRetrievalQuickstart", dependencies: ["VectorKitGraph"]),
        .testTarget(name: "VectorKitGraphTests", dependencies: ["VectorKitGraph"]),
    ],
    swiftLanguageModes: [.v6]
)
