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
        .library(name: "RetrievalKitIngest", targets: ["RetrievalKitIngest"]),
        .executable(name: "RetrievalKitDatabaseQuickstart", targets: ["RetrievalKitDatabaseQuickstart"]),
        .executable(name: "RetrievalKitRetrievalQuickstart", targets: ["RetrievalKitRetrievalQuickstart"])
    ],
    dependencies: [.package(path: "../RetrievalKitShared")],
    targets: [
        .systemLibrary(
            name: "RetrievalKitFFI",
            path: "Sources/CRetrievalKitFFI"
        ),
        .target(
            name: "RetrievalKit",
            dependencies: ["RetrievalKitFFI", .product(name: "RetrievalKitShared", package: "RetrievalKitShared")],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/debug/libretrievalkit_ffi.a"
                ])
            ]
        ),
        .target(
            name: "RetrievalKitIngest",
            dependencies: ["RetrievalKitFFI"],
            linkerSettings: [
                .unsafeFlags([
                    "../../../target/debug/libretrievalkit_ffi.a"
                ])
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
        .testTarget(
            name: "RetrievalKitIngestTests",
            dependencies: ["RetrievalKitIngest"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
