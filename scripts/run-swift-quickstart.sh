#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_DIR="${RETRIEVALKIT_APPLE_ARTIFACT_DIR:-$ROOT_DIR/target/apple}"

usage() {
  cat <<'EOF'
usage:
  scripts/run-swift-quickstart.sh <example>

Checked Swift quickstart entrypoint. It verifies the required local XCFramework
before invoking SwiftPM.

Examples:
  base-semantic    RetrievalKitDatabaseQuickstart
  base-retrieval   RetrievalKitRetrievalQuickstart
  graph            RetrievalKitGraphQuickstart
  graph-retrieval  RetrievalKitGraphRetrievalQuickstart
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 2
fi

case "$1" in
  base-semantic)
    artifact="RetrievalKitFFI.xcframework"
    build_command="scripts/build-xcframework.sh --macos-only"
    package_path="wrappers/swift/RetrievalKit"
    executable="RetrievalKitDatabaseQuickstart"
    ;;
  base-retrieval)
    artifact="RetrievalKitFFI.xcframework"
    build_command="scripts/build-xcframework.sh --macos-only"
    package_path="wrappers/swift/RetrievalKit"
    executable="RetrievalKitRetrievalQuickstart"
    ;;
  graph)
    artifact="RetrievalKitGraphFFI.xcframework"
    build_command="scripts/build-xcframework.sh --macos-only --graph"
    package_path="wrappers/swift/RetrievalKitGraph"
    executable="RetrievalKitGraphQuickstart"
    ;;
  graph-retrieval)
    artifact="RetrievalKitGraphFFI.xcframework"
    build_command="scripts/build-xcframework.sh --macos-only --graph"
    package_path="wrappers/swift/RetrievalKitGraph"
    executable="RetrievalKitGraphRetrievalQuickstart"
    ;;
  --help|-h)
    usage
    exit 0
    ;;
  *)
    echo "Swift wrapper entrypoint failed: unknown example '$1'." >&2
    usage >&2
    exit 2
    ;;
esac

artifact_path="$ARTIFACT_DIR/$artifact"
if [[ ! -d "$artifact_path" ]]; then
  cat >&2 <<EOF
Swift wrapper preflight failed: required local artifact is missing:
  $artifact_path

From the repository root, build it before invoking SwiftPM:
  $build_command

Then retry:
  scripts/run-swift-quickstart.sh $1
EOF
  exit 1
fi

if ! command -v swift >/dev/null 2>&1; then
  echo "Swift wrapper preflight failed: 'swift' is required on PATH; install Xcode Command Line Tools and retry." >&2
  exit 1
fi

cd "$ROOT_DIR"
exec swift run --package-path "$package_path" "$executable"
