// swift-tools-version: 6.2

import Foundation
import PackageDescription

let version = "0.1.0"
let releaseBase = "https://github.com/gungorbasa/RetrievalKit/releases/download/v\(version)"
let useLocalArtifacts = ProcessInfo.processInfo.environment["RETRIEVALKIT_USE_LOCAL_ARTIFACTS"] == "1"

let nativeBinary: Target = useLocalArtifacts
  ? .binaryTarget(
    name: "RetrievalKitGraphFFI",
    path: "target/apple/RetrievalKitGraphFFI.xcframework"
  )
  : .binaryTarget(
    name: "RetrievalKitGraphFFI",
    url: "\(releaseBase)/RetrievalKitGraphFFI.xcframework.zip",
    checksum: "5cac49a81d352eb5a50e588bfed108b7c0ab356e2284ff079e41f58685fd288a"
  )

let package = Package(
  name: "RetrievalKit",
  platforms: [.macOS(.v14), .iOS(.v15)],
  products: [
    .library(name: "RetrievalKit", targets: ["RetrievalKit"]),
    .library(name: "RetrievalKitGraph", targets: ["RetrievalKitGraph"]),
    .library(name: "EmbeddingKit", targets: ["EmbeddingKit"]),
    .library(name: "RetrievalKitPipeline", targets: ["RetrievalKitPipeline"]),
  ],
  targets: [
    nativeBinary,
    .target(
      name: "RetrievalKitShared",
      path: "wrappers/swift/RetrievalKitShared/Sources/RetrievalKitShared"
    ),
    .target(
      name: "RetrievalKit",
      dependencies: ["RetrievalKitGraphFFI", "RetrievalKitShared"],
      path: "wrappers/swift/RetrievalKit/Sources/RetrievalKit"
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
      dependencies: ["RetrievalKit", "EmbeddingKit"],
      path: "wrappers/swift/RetrievalKitPipeline/Sources/RetrievalKitPipeline"
    ),
  ],
  swiftLanguageModes: [.v6]
)
