#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" ]]; then
  SKIP_BUILD=true
elif [[ $# -ne 0 ]]; then
  echo "usage: $0 [--skip-build]" >&2
  exit 2
fi

if [[ "$SKIP_BUILD" == false ]]; then
  "$ROOT_DIR/scripts/build-xcframework.sh" --macos-only
  "$ROOT_DIR/scripts/build-xcframework.sh" --macos-only --graph
fi

BASE_BINARY="$ROOT_DIR/target/apple/VectorKitFFI.xcframework/macos-arm64/VectorKitFFI.framework/VectorKitFFI"
GRAPH_BINARY="$ROOT_DIR/target/apple/VectorKitGraphFFI.xcframework/macos-arm64/VectorKitGraphFFI.framework/VectorKitGraphFFI"
for binary in "$BASE_BINARY" "$GRAPH_BINARY"; do
  [[ -f "$binary" ]] || { echo "missing native artifact: $binary" >&2; exit 1; }
done

if nm -g "$BASE_BINARY" 2>/dev/null | grep -F '_vectorkit_graph_ffi_abi_version' >/dev/null; then
  echo "base VectorKitFFI unexpectedly exports graph symbols" >&2
  exit 1
fi
nm -g "$BASE_BINARY" 2>/dev/null | grep -F '_vectorkit_index_new' >/dev/null || {
  echo "base VectorKitFFI does not export the core API" >&2
  exit 1
}
nm -g "$GRAPH_BINARY" 2>/dev/null | grep -F '_vectorkit_index_new' >/dev/null || {
  echo "graph aggregate does not export the core API" >&2
  exit 1
}
nm -g "$GRAPH_BINARY" 2>/dev/null | grep -F '_vectorkit_graph_ffi_abi_version' >/dev/null || {
  echo "graph aggregate does not export the graph API" >&2
  exit 1
}

swift test --package-path "$ROOT_DIR/wrappers/swift/VectorKitShared"
swift test --package-path "$ROOT_DIR/wrappers/swift/VectorKit"
swift test --package-path "$ROOT_DIR/wrappers/swift/VectorKitGraph"

check_quickstart() {
  local package_path="$1"
  local executable="$2"
  local expected="$3"
  local actual
  actual="$(swift run --package-path "$package_path" "$executable")"
  if [[ "$actual" != "$expected" ]]; then
    echo "$executable output mismatch" >&2
    diff -u <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") >&2 || true
    exit 1
  fi
  printf '%s\n' "$actual"
}

check_quickstart "$ROOT_DIR/wrappers/swift/VectorKit" VectorKitRetrievalQuickstart 'retrieval=rust'
check_quickstart "$ROOT_DIR/wrappers/swift/VectorKitGraph" VectorKitGraphQuickstart 'graph=rust'
check_quickstart "$ROOT_DIR/wrappers/swift/VectorKitGraph" VectorKitGraphRetrievalQuickstart 'combined=rust'
echo "Swift base/graph linkage and conformance verification passed"
