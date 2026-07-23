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
    checksum: "fcc3c94144ce26104c92abb9227a1e95a45395e1db44265e70e585ead915266f"
  )

let graphBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "RetrievalKitGraphFFI", path: "target/apple/RetrievalKitGraphFFI.xcframework")
  : .binaryTarget(
    name: "RetrievalKitGraphFFI",
    url: "\(releaseBase)/RetrievalKitGraphFFI.xcframework.zip",
    checksum: "5cac89628b3296aaedda0006049283d87261d157c09d7f537b05a93e8b1f4468"
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
