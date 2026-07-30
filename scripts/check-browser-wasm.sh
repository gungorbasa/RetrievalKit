#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wasm_target="wasm32-unknown-unknown"
wasm_binary="$repository_root/target/$wasm_target/release/retrievalkit_wasm.wasm"
smoke_test="$repository_root/crates/retrievalkit-wasm/tests/node-smoke.cjs"
simd_conformance_test="$repository_root/crates/retrievalkit-wasm/tests/node-simd-conformance.cjs"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen CLI is required; install the version recorded by Cargo.lock" >&2
  exit 1
fi

generated_directory="$(mktemp -d)"
portable_node_directory="$generated_directory/portable-node"
portable_web_directory="$generated_directory/portable-web"
simd_target_directory="$generated_directory/simd-target"
simd_binary="$simd_target_directory/$wasm_target/release/retrievalkit_wasm.wasm"
simd_node_directory="$generated_directory/simd-node"
simd_web_directory="$generated_directory/simd-web"
cleanup() {
  rm -rf "$generated_directory"
}
trap cleanup EXIT
mkdir -p \
  "$portable_node_directory" \
  "$portable_web_directory" \
  "$simd_node_directory" \
  "$simd_web_directory"

cargo build \
  --manifest-path "$repository_root/Cargo.toml" \
  --locked \
  --release \
  --target "$wasm_target" \
  -p retrievalkit-wasm

wasm-bindgen \
  "$wasm_binary" \
  --target nodejs \
  --out-dir "$portable_node_directory"

wasm-bindgen \
  "$wasm_binary" \
  --target web \
  --typescript \
  --out-dir "$portable_web_directory"

test -s "$portable_web_directory/retrievalkit_wasm_bg.wasm"
for export_name in \
  "RetrievalDatabase" \
  "GraphDatabase" \
  "GraphRetrievalDatabase" \
  "buildCapabilities"
do
  grep -q "$export_name" "$portable_web_directory/retrievalkit_wasm.d.ts"
done

node "$smoke_test" "$portable_node_directory/retrievalkit_wasm.js" portable

CARGO_TARGET_DIR="$simd_target_directory" cargo build \
  --manifest-path "$repository_root/Cargo.toml" \
  --locked \
  --release \
  --target "$wasm_target" \
  -p retrievalkit-wasm \
  --features wasm-simd128

wasm-bindgen \
  "$simd_binary" \
  --target nodejs \
  --out-dir "$simd_node_directory"

wasm-bindgen \
  "$simd_binary" \
  --target web \
  --typescript \
  --out-dir "$simd_web_directory"

test -s "$simd_web_directory/retrievalkit_wasm_bg.wasm"
node "$smoke_test" "$simd_node_directory/retrievalkit_wasm.js" simd128
node \
  "$simd_conformance_test" \
  "$portable_node_directory/retrievalkit_wasm.js" \
  "$simd_node_directory/retrievalkit_wasm.js"
