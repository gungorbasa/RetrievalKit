#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/target/release-apple-artifacts}"
export CARGO_INCREMENTAL=0
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$ROOT_DIR=/workspace"
unset RUSTFLAGS

"$ROOT_DIR/scripts/build-xcframework.sh"
"$ROOT_DIR/scripts/build-xcframework.sh" --graph
python3 "$ROOT_DIR/scripts/release/canonicalize_xcframework.py" \
  "$ROOT_DIR/target/apple/RetrievalKitFFI.xcframework"
python3 "$ROOT_DIR/scripts/release/canonicalize_xcframework.py" \
  "$ROOT_DIR/target/apple/RetrievalKitGraphFFI.xcframework"
mkdir -p "$OUTPUT_DIR"
python3 "$ROOT_DIR/scripts/release/canonical_zip.py" \
  "$ROOT_DIR/target/apple/RetrievalKitFFI.xcframework" \
  "$OUTPUT_DIR/RetrievalKitFFI.xcframework.zip"
python3 "$ROOT_DIR/scripts/release/canonical_zip.py" \
  "$ROOT_DIR/target/apple/RetrievalKitGraphFFI.xcframework" \
  "$OUTPUT_DIR/RetrievalKitGraphFFI.xcframework.zip"

for archive in "$OUTPUT_DIR"/*.zip; do
  swift package compute-checksum "$archive"
done
