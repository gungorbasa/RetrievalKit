#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wasm_target="wasm32-unknown-unknown"
wasm_binary="$repository_root/target/$wasm_target/release/retrievalkit_wasm.wasm"
benchmark="$repository_root/crates/retrievalkit-wasm/tests/node-benchmark.cjs"
count="${1:-10000}"
dimension="${2:-384}"
iterations="${3:-30}"
encoding="${4:-f32}"
tier="${5:-portable}"
warmups="${6:-5}"

case "$tier" in
  portable)
    feature_arguments=()
    ;;
  simd128)
    feature_arguments=(--features wasm-simd128)
    ;;
  *)
    echo "tier must be 'portable' or 'simd128'" >&2
    exit 1
    ;;
esac

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen CLI is required; install the version recorded by Cargo.lock" >&2
  exit 1
fi

generated_directory="$(mktemp -d)"
cleanup() {
  rm -rf "$generated_directory"
}
trap cleanup EXIT

cargo build \
  --manifest-path "$repository_root/Cargo.toml" \
  --locked \
  --release \
  --target "$wasm_target" \
  -p retrievalkit-wasm \
  "${feature_arguments[@]}"

wasm-bindgen \
  "$wasm_binary" \
  --target nodejs \
  --out-dir "$generated_directory"

node \
  "$benchmark" \
  "$generated_directory/retrievalkit_wasm.js" \
  "$count" \
  "$dimension" \
  "$iterations" \
  "$encoding" \
  "$warmups"
