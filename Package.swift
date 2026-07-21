// swift-tools-version: 6.2

import Foundation
import PackageDescription

let version = "0.1.0"
let releaseBase = "https://github.com/gungorbasa/VectorKit/releases/download/v\(version)"
let useLocalArtifacts = ProcessInfo.processInfo.environment["VECTORKIT_USE_LOCAL_ARTIFACTS"] == "1"

let baseBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "VectorKitFFI", path: "target/apple/VectorKitFFI.xcframework")
  : .binaryTarget(
    name: "VectorKitFFI",
    url: "\(releaseBase)/VectorKitFFI.xcframework.zip",
    checksum: "c5fd96725f3991f6a6770bdaa2affaff7ccbb8015fb05731f44e24368967869d"
  )

let graphBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "VectorKitGraphFFI", path: "target/apple/VectorKitGraphFFI.xcframework")
  : .binaryTarget(
    name: "VectorKitGraphFFI",
    url: "\(releaseBase)/VectorKitGraphFFI.xcframework.zip",
    checksum: "e94e6079c781fbeb5cdad740f14746869748242bf81b3955c1e9493abe321ec4"
  )

let package = Package(
  name: "VectorKit",
  platforms: [.macOS(.v14), .iOS(.v15)],
  products: [
    .library(name: "VectorKit", targets: ["VectorKit"]),
    .library(name: "VectorKitIngest", targets: ["VectorKitIngest"]),
    .library(name: "VectorKitGraph", targets: ["VectorKitGraph"]),
    .library(name: "EmbeddingKit", targets: ["EmbeddingKit"]),
    .library(name: "VectorKitPipeline", targets: ["VectorKitPipeline"]),
  ],
  targets: [
    baseBinary,
    graphBinary,
    .target(
      name: "VectorKitShared",
      path: "wrappers/swift/VectorKitShared/Sources/VectorKitShared"
    ),
    .target(
      name: "VectorKit",
      dependencies: ["VectorKitFFI", "VectorKitShared"],
      path: "wrappers/swift/VectorKit/Sources/VectorKit"
    ),
    .target(
      name: "VectorKitIngest",
      dependencies: ["VectorKitFFI"],
      path: "wrappers/swift/VectorKit/Sources/VectorKitIngest"
    ),
    .target(
      name: "VectorKitGraph",
      dependencies: ["VectorKitGraphFFI", "VectorKitShared"],
      path: "wrappers/swift/VectorKitGraph/Sources/VectorKitGraph"
    ),
    .target(
      name: "EmbeddingKit",
      path: "wrappers/swift/EmbeddingKit/Sources/EmbeddingKit"
    ),
    .target(
      name: "VectorKitPipeline",
      dependencies: ["VectorKit", "VectorKitIngest", "EmbeddingKit"],
      path: "wrappers/swift/VectorKitPipeline/Sources/VectorKitPipeline"
    ),
  ],
  swiftLanguageModes: [.v6]
)
