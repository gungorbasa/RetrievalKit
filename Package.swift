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
    checksum: "9c4a05595c9872907f53a34f602bc2d7008d3d4679768bd33c094bae8aaa1c06"
  )

let graphBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "VectorKitGraphFFI", path: "target/apple/VectorKitGraphFFI.xcframework")
  : .binaryTarget(
    name: "VectorKitGraphFFI",
    url: "\(releaseBase)/VectorKitGraphFFI.xcframework.zip",
    checksum: "941f5724f94c181a89e26ad5bd285a234a24c0771d8e5531ebc4f88c3eaeaa3f"
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
