// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "VectorKitGraph",
    platforms: [.macOS(.v14), .iOS(.v15)],
    products: [.library(name: "VectorKitGraph", targets: ["VectorKitGraph"])],
    targets: [
        .systemLibrary(name: "VectorKitGraphFFI", path: "Sources/CVectorKitGraphFFI"),
        .target(name: "VectorKitGraph", dependencies: ["VectorKitGraphFFI"], linkerSettings: [.unsafeFlags(["../../../target/debug/libvectorkit_ffi.a"])]),
        .testTarget(name: "VectorKitGraphTests", dependencies: ["VectorKitGraph"]),
    ],
    swiftLanguageModes: [.v6]
)
