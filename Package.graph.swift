// swift-tools-version: 6.2

import Foundation
import PackageDescription

// Publication manifest for the standalone RetrievalKitGraph Swift package.
// Release tooling stages this file as Package.swift in the graph package
// repository so SwiftPM never resolves the base and graph binaries together.
let version = "0.1.0"
let releaseBase = "https://github.com/gungorbasa/RetrievalKit/releases/download/v\(version)"
let useLocalArtifacts = ProcessInfo.processInfo.environment["RETRIEVALKIT_USE_LOCAL_ARTIFACTS"] == "1"

let graphBinary: Target = useLocalArtifacts
  ? .binaryTarget(name: "RetrievalKitGraphFFI", path: "target/apple/RetrievalKitGraphFFI.xcframework")
  : .binaryTarget(
    name: "RetrievalKitGraphFFI",
    url: "\(releaseBase)/RetrievalKitGraphFFI.xcframework.zip",
    checksum: "5cac89628b3296aaedda0006049283d87261d157c09d7f537b05a93e8b1f4468"
  )

let package = Package(
  name: "RetrievalKitGraph",
  platforms: [.macOS(.v14), .iOS(.v15)],
  products: [
    .library(name: "RetrievalKitGraph", targets: ["RetrievalKitGraph"]),
  ],
  targets: [
    graphBinary,
    .target(
      name: "RetrievalKitShared",
      path: "wrappers/swift/RetrievalKitShared/Sources/RetrievalKitShared"
    ),
    .target(
      name: "RetrievalKitGraph",
      dependencies: ["RetrievalKitGraphFFI", "RetrievalKitShared"],
      path: "wrappers/swift/RetrievalKitGraph/Sources/RetrievalKitGraph"
    ),
  ],
  swiftLanguageModes: [.v6]
)
