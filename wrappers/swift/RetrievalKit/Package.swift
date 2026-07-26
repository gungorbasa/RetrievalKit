// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "RetrievalKit",
    platforms: [
        .macOS(.v14),
        .iOS(.v15)
    ],
    products: [
        .library(name: "RetrievalKit", targets: ["RetrievalKit"]),
        .executable(name: "RetrievalKitDatabaseQuickstart", targets: ["RetrievalKitDatabaseQuickstart"]),
        .executable(name: "RetrievalKitRetrievalQuickstart", targets: ["RetrievalKitRetrievalQuickstart"])
    ],
    dependencies: [.package(path: "../RetrievalKitShared")],
    targets: [
        .binaryTarget(
            name: "RetrievalKitFFI",
            path: "../../../target/apple/RetrievalKitFFI.xcframework"
        ),
        .target(
            name: "RetrievalKit",
            dependencies: [
                "RetrievalKitFFI",
                .product(name: "RetrievalKitShared", package: "RetrievalKitShared")
            ]
        ),
        .executableTarget(
            name: "RetrievalKitDatabaseQuickstart",
            dependencies: ["RetrievalKit"]
        ),
        .executableTarget(
            name: "RetrievalKitRetrievalQuickstart",
            dependencies: ["RetrievalKit"]
        ),
        .testTarget(
            name: "RetrievalKitTests",
            dependencies: ["RetrievalKit"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
