#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT="$ROOT_DIR/wrappers/swift/VectorKitIOSBench/VectorKitIOSBench.xcodeproj"
BASE_DERIVED="$ROOT_DIR/target/xcode-phase4-base"
GRAPH_DERIVED="$ROOT_DIR/target/xcode-phase4-graph"

if [[ "${1:-}" != "--skip-build" ]]; then
  "$ROOT_DIR/scripts/build-xcframework.sh"
  "$ROOT_DIR/scripts/build-xcframework.sh" --graph
  xcodebuild -project "$PROJECT" -scheme VectorKitIOSBench -configuration Release \
    -sdk iphoneos -derivedDataPath "$BASE_DERIVED" CODE_SIGNING_ALLOWED=NO build
  xcodebuild -project "$PROJECT" -scheme VectorKitIOSGraphBench -configuration Release \
    -sdk iphoneos -derivedDataPath "$GRAPH_DERIVED" CODE_SIGNING_ALLOWED=NO build
elif [[ $# -ne 1 ]]; then
  echo "usage: $0 [--skip-build]" >&2
  exit 2
fi

BASE_BINARY="$BASE_DERIVED/Build/Products/Release-iphoneos/VectorKitIOSBench.app/VectorKitIOSBench"
GRAPH_BINARY="$GRAPH_DERIVED/Build/Products/Release-iphoneos/VectorKitIOSGraphBench.app/VectorKitIOSGraphBench"
for binary in "$BASE_BINARY" "$GRAPH_BINARY"; do
  [[ -f "$binary" ]] || { echo "missing release iOS binary: $binary" >&2; exit 1; }
  file "$binary" | grep -F 'Mach-O 64-bit executable arm64' >/dev/null || {
    echo "release iOS binary is not arm64: $binary" >&2
    exit 1
  }
done

if nm -g "$BASE_BINARY" | grep -F '_vectorkit_graph_' >/dev/null; then
  echo "graph-free iOS binary unexpectedly contains a graph symbol" >&2
  exit 1
fi
nm -g "$BASE_BINARY" | grep -F '_vectorkit_bench_memory_json' >/dev/null || {
  echo "graph-free iOS binary does not contain the base benchmark API" >&2
  exit 1
}
nm -g "$GRAPH_BINARY" | grep -F '_vectorkit_graph_ffi_abi_version' >/dev/null || {
  echo "graph-capable iOS binary does not contain the graph API" >&2
  exit 1
}
for counter in graph_state_creations graph_file_opens graph_dispatches; do
  strings "$BASE_BINARY" | grep -Fx "$counter" >/dev/null || {
    echo "graph-free zero-instrumentation field is missing: $counter" >&2
    exit 1
  }
done

echo "isolated release iOS benchmark linkage verification passed"
