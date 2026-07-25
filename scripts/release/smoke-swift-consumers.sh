#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEMP_ROOT"' EXIT

GRAPH_PACKAGE_ROOT="$TEMP_ROOT/RetrievalKitGraphPackage"
mkdir -p \
  "$GRAPH_PACKAGE_ROOT/target/apple" \
  "$GRAPH_PACKAGE_ROOT/wrappers/swift/RetrievalKitGraph/Sources" \
  "$GRAPH_PACKAGE_ROOT/wrappers/swift/RetrievalKitShared/Sources"
cp "$ROOT_DIR/Package.graph.swift" "$GRAPH_PACKAGE_ROOT/Package.swift"
cp -R \
  "$ROOT_DIR/target/apple/RetrievalKitGraphFFI.xcframework" \
  "$GRAPH_PACKAGE_ROOT/target/apple/RetrievalKitGraphFFI.xcframework"
cp -R \
  "$ROOT_DIR/wrappers/swift/RetrievalKitGraph/Sources/RetrievalKitGraph" \
  "$GRAPH_PACKAGE_ROOT/wrappers/swift/RetrievalKitGraph/Sources/RetrievalKitGraph"
cp -R \
  "$ROOT_DIR/wrappers/swift/RetrievalKitShared/Sources/RetrievalKitShared" \
  "$GRAPH_PACKAGE_ROOT/wrappers/swift/RetrievalKitShared/Sources/RetrievalKitShared"

smoke_product() {
  local package_root="$1"
  local package_name="$2"
  local product="$3"
  local directory="$TEMP_ROOT/Consumer-$product"
  mkdir -p "$directory/Sources/Consumer"
  cat >"$directory/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "Consumer",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "$package_name", path: "$package_root")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [.product(name: "$product", package: "$package_name")]
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

smoke_product "$ROOT_DIR" RetrievalKitBase RetrievalKit
smoke_product "$GRAPH_PACKAGE_ROOT" RetrievalKitGraphPackage RetrievalKitGraph
smoke_product "$ROOT_DIR" RetrievalKitBase EmbeddingKit
smoke_product "$ROOT_DIR" RetrievalKitBase RetrievalKitPipeline

conflict_dir="$TEMP_ROOT/ConflictingAggregates"
mkdir -p "$conflict_dir/Sources/Consumer"
cat >"$conflict_dir/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "ConflictingAggregates",
  platforms: [.macOS(.v14)],
  dependencies: [
    .package(name: "RetrievalKitBase", path: "$ROOT_DIR"),
    .package(name: "RetrievalKitGraphPackage", path: "$GRAPH_PACKAGE_ROOT"),
  ],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [
        .product(name: "RetrievalKit", package: "RetrievalKitBase"),
        .product(name: "RetrievalKitGraph", package: "RetrievalKitGraphPackage"),
      ],
      linkerSettings: [.unsafeFlags(["-Xlinker", "-all_load"])]
    )
  ]
)
EOF
cat >"$conflict_dir/Sources/Consumer/main.swift" <<'EOF'
import RetrievalKit
import RetrievalKitGraph

_ = try VectorIndex(dimension: 2, encoding: .f32)
_ = try GraphDatabase.Builder(
  corpusID: "conflict",
  schema: GraphSchema(
    recordNodes: [
      GraphRecordNodeSchema(recordType: "Probe", nodeType: "Probe")
    ]
  )
)
EOF
if RETRIEVALKIT_USE_LOCAL_ARTIFACTS=1 swift build --package-path "$conflict_dir" >"$TEMP_ROOT/conflict.log" 2>&1; then
  echo "base and graph native aggregates unexpectedly linked together" >&2
  exit 1
fi
grep -Eiq 'multiple commands produce|duplicate symbol|framework.*collision|targets with a conflicting name' "$TEMP_ROOT/conflict.log" || {
  echo "aggregate conflict did not produce the expected linker diagnostic" >&2
  cat "$TEMP_ROOT/conflict.log" >&2
  exit 1
}
echo "isolated Swift consumer smoke tests passed"
