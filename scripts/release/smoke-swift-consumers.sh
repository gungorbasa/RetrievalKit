#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEMP_ROOT"' EXIT

smoke_product() {
  local product="$1"
  local directory="$TEMP_ROOT/$product"
  mkdir -p "$directory/Sources/Consumer"
  cat >"$directory/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "Consumer",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "VectorKit", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [.product(name: "$product", package: "VectorKit")]
    )
  ]
)
EOF
  cat >"$directory/Sources/Consumer/main.swift" <<EOF
import $product
print("$product=ok")
EOF
  local output
  output="$(VECTORKIT_USE_LOCAL_ARTIFACTS=1 swift run --package-path "$directory" Consumer)"
  [[ "$output" == "$product=ok" ]] || { echo "unexpected $product consumer output: $output" >&2; exit 1; }
}

smoke_product VectorKit
smoke_product VectorKitGraph
smoke_product EmbeddingKit
smoke_product VectorKitPipeline

conflict_dir="$TEMP_ROOT/ConflictingAggregates"
mkdir -p "$conflict_dir/Sources/Consumer"
cat >"$conflict_dir/Package.swift" <<EOF
// swift-tools-version: 6.2
import PackageDescription
let package = Package(
  name: "ConflictingAggregates",
  platforms: [.macOS(.v14)],
  dependencies: [.package(name: "VectorKit", path: "$ROOT_DIR")],
  targets: [
    .executableTarget(
      name: "Consumer",
      dependencies: [
        .product(name: "VectorKit", package: "VectorKit"),
        .product(name: "VectorKitGraph", package: "VectorKit"),
      ]
    )
  ]
)
EOF
cat >"$conflict_dir/Sources/Consumer/main.swift" <<'EOF'
import VectorKit
import VectorKitGraph
let _: VectorIndex? = nil
let _: GraphDatabase? = nil
EOF
if VECTORKIT_USE_LOCAL_ARTIFACTS=1 swift build --package-path "$conflict_dir" >"$TEMP_ROOT/conflict.log" 2>&1; then
  echo "base and graph native aggregates unexpectedly linked together" >&2
  exit 1
fi
grep -Eiq 'multiple commands produce|duplicate symbol|framework.*collision' "$TEMP_ROOT/conflict.log" || {
  echo "aggregate conflict did not produce the expected linker diagnostic" >&2
  cat "$TEMP_ROOT/conflict.log" >&2
  exit 1
}
echo "isolated Swift consumer smoke tests passed"
