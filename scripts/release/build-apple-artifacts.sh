#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/target/release-apple-artifacts}"

"$ROOT_DIR/scripts/build-xcframework.sh"
"$ROOT_DIR/scripts/build-xcframework.sh" --graph
mkdir -p "$OUTPUT_DIR"
python3 "$ROOT_DIR/scripts/release/canonical_zip.py" \
  "$ROOT_DIR/target/apple/VectorKitFFI.xcframework" \
  "$OUTPUT_DIR/VectorKitFFI.xcframework.zip"
python3 "$ROOT_DIR/scripts/release/canonical_zip.py" \
  "$ROOT_DIR/target/apple/VectorKitGraphFFI.xcframework" \
  "$OUTPUT_DIR/VectorKitGraphFFI.xcframework.zip"

for archive in "$OUTPUT_DIR"/*.zip; do
  swift package compute-checksum "$archive"
done
