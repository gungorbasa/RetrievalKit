#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEMP_ROOT"' EXIT

smoke_product() {
  local product="$1"
  local directory="$TEMP_ROOT/Consumer-$product"
  mkdir -p "$directory/Sources/Consumer"
  cat >"$directory/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "Consumer",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "RetrievalKitPackage", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [.product(name: "$product", package: "RetrievalKitPackage")]
    )
  ]
)
EOF
  cat >"$directory/Sources/Consumer/main.swift" <<EOF
import $product
print("$product=ok")
EOF
  local output
  output="$(RETRIEVALKIT_USE_LOCAL_ARTIFACTS=1 swift run --package-path "$directory" Consumer)"
  [[ "$output" == "$product=ok" ]] || { echo "unexpected $product consumer output: $output" >&2; exit 1; }
}

smoke_product RetrievalKit
smoke_product RetrievalKitGraph
smoke_product EmbeddingKit
smoke_product RetrievalKitPipeline

combined_dir="$TEMP_ROOT/CombinedProducts"
mkdir -p "$combined_dir/Sources/Consumer"
cat >"$combined_dir/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "CombinedProducts",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "RetrievalKitPackage", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [
        .product(name: "RetrievalKit", package: "RetrievalKitPackage"),
        .product(name: "RetrievalKitGraph", package: "RetrievalKitPackage"),
      ]
    )
  ]
)
EOF
cat >"$combined_dir/Sources/Consumer/main.swift" <<'EOF'
import RetrievalKit
import RetrievalKitGraph

_ = try VectorIndex(dimension: 2, encoding: .f32)
let document = Document(id: "shared-document", text: "shared type")
guard document.id.rawValue == "shared-document" else {
  fatalError("unexpected shared document identity")
}
let chunks = try TextChunker(strategy: .fixed, maxCharacters: 4).chunks(for: "chunk me")
guard chunks.map(\.text) == ["chun", "k me"] else {
  fatalError("unexpected text chunking result")
}
_ = try GraphDatabase.Builder(
  corpusID: "combined",
  schema: GraphSchema(
    recordNodes: [
      GraphRecordNodeSchema(recordType: "Probe", nodeType: "Probe")
    ]
  )
)
print("combined=ok")
EOF
combined_output="$(RETRIEVALKIT_USE_LOCAL_ARTIFACTS=1 swift run --package-path "$combined_dir" Consumer)"
[[ "$combined_output" == "combined=ok" ]] || {
  echo "unexpected combined consumer output: $combined_output" >&2
  exit 1
}
echo "single-package Swift consumer smoke tests passed"
