// swift-tools-version: 6.2

import Foundation
import PackageDescription

let version = "0.1.0"
let releaseBase = "https://github.com/gungorbasa/RetrievalKit/releases/download/v\(version)"
let useLocalArtifacts = ProcessInfo.processInfo.environment["RETRIEVALKIT_USE_LOCAL_ARTIFACTS"] == "1"

let baseBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "RetrievalKitFFI", path: "target/apple/RetrievalKitFFI.xcframework")
  : .binaryTarget(
    name: "RetrievalKitFFI",
    url: "\(releaseBase)/RetrievalKitFFI.xcframework.zip",
    checksum: "c5fd96725f3991f6a6770bdaa2affaff7ccbb8015fb05731f44e24368967869d"
  )

let graphBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "RetrievalKitGraphFFI", path: "target/apple/RetrievalKitGraphFFI.xcframework")
  : .binaryTarget(
    name: "RetrievalKitGraphFFI",
    url: "\(releaseBase)/RetrievalKitGraphFFI.xcframework.zip",
    checksum: "e94e6079c781fbeb5cdad740f14746869748242bf81b3955c1e9493abe321ec4"
  )

let package = Package(
  name: "RetrievalKit",
  platforms: [.macOS(.v14), .iOS(.v15)],
  products: [
    .library(name: "RetrievalKit", targets: ["RetrievalKit"]),
    .library(name: "RetrievalKitIngest", targets: ["RetrievalKitIngest"]),
    .library(name: "RetrievalKitGraph", targets: ["RetrievalKitGraph"]),
    .library(name: "EmbeddingKit", targets: ["EmbeddingKit"]),
    .library(name: "RetrievalKitPipeline", targets: ["RetrievalKitPipeline"]),
  ],
  targets: [
    baseBinary,
    graphBinary,
    .target(
      name: "RetrievalKitShared",
      path: "wrappers/swift/RetrievalKitShared/Sources/RetrievalKitShared"
    ),
    .target(
      name: "RetrievalKit",
      dependencies: ["RetrievalKitFFI", "RetrievalKitShared"],
      path: "wrappers/swift/RetrievalKit/Sources/RetrievalKit"
    ),
    .target(
      name: "RetrievalKitIngest",
      dependencies: ["RetrievalKitFFI"],
      path: "wrappers/swift/RetrievalKit/Sources/RetrievalKitIngest"
    ),
    .target(
      name: "RetrievalKitGraph",
      dependencies: ["RetrievalKitGraphFFI", "RetrievalKitShared"],
      path: "wrappers/swift/RetrievalKitGraph/Sources/RetrievalKitGraph"
    ),
    .target(
      name: "EmbeddingKit",
      path: "wrappers/swift/EmbeddingKit/Sources/EmbeddingKit"
    ),
    .target(
      name: "RetrievalKitPipeline",
      dependencies: ["RetrievalKit", "RetrievalKitIngest", "EmbeddingKit"],
      path: "wrappers/swift/RetrievalKitPipeline/Sources/RetrievalKitPipeline"
    ),
  ],
  swiftLanguageModes: [.v6]
)
