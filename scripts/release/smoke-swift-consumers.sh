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
  dependencies: [.package(name: "RetrievalKit", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [.product(name: "$product", package: "RetrievalKit")]
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

conflict_dir="$TEMP_ROOT/ConflictingAggregates"
mkdir -p "$conflict_dir/Sources/Consumer"
cat >"$conflict_dir/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "ConflictingAggregates",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "RetrievalKit", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [
        .product(name: "RetrievalKit", package: "RetrievalKit"),
        .product(name: "RetrievalKitGraph", package: "RetrievalKit"),
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
grep -Eiq 'multiple commands produce|duplicate symbol|framework.*collision' "$TEMP_ROOT/conflict.log" || {
  echo "aggregate conflict did not produce the expected linker diagnostic" >&2
  cat "$TEMP_ROOT/conflict.log" >&2
  exit 1
}
echo "isolated Swift consumer smoke tests passed"
