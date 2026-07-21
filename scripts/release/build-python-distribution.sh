#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_BIN="${PYTHON_BIN:-python3}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/target/release-python-wheels}"
DISTRIBUTION="${1:-}"

case "$DISTRIBUTION" in
  vectorkit)
    WRAPPER_DIR="$ROOT_DIR/wrappers/python"
    SMOKE="$ROOT_DIR/wrappers/python/tests/smoke_installed.py"
    ;;
  vectorkit-graph)
    WRAPPER_DIR="$ROOT_DIR/wrappers/python-graph"
    SMOKE="$ROOT_DIR/wrappers/python-graph/tests/smoke_installed.py"
    ;;
  *)
    echo "usage: $0 {vectorkit|vectorkit-graph}" >&2
    exit 2
    ;;
esac

PYTHON_TAG="$($PYTHON_BIN -c 'import sys; print(f"cp{sys.version_info.major}{sys.version_info.minor}")')"
case "$PYTHON_TAG" in
  cp310|cp311|cp312|cp313|cp314) ;;
  *) echo "release wheel requires CPython 3.10 through 3.14, got $PYTHON_TAG" >&2; exit 1 ;;
esac

BUILD_VENV="$ROOT_DIR/target/release-wheel-build-$DISTRIBUTION-$PYTHON_TAG"
SMOKE_VENV="$ROOT_DIR/target/release-wheel-smoke-$DISTRIBUTION-$PYTHON_TAG"
BUILD_DIR="$ROOT_DIR/target/release-wheel-output-$DISTRIBUTION-$PYTHON_TAG"

"$PYTHON_BIN" -m venv --clear "$BUILD_VENV"
"$BUILD_VENV/bin/python" -m pip install --disable-pip-version-check 'maturin==1.14.1'
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR" "$OUTPUT_DIR"
(
  cd "$WRAPPER_DIR"
  "$BUILD_VENV/bin/maturin" build --locked --release --interpreter "$BUILD_VENV/bin/python" --out "$BUILD_DIR"
)

WHEELS=()
while IFS= read -r wheel; do
  WHEELS+=("$wheel")
done < <(find "$BUILD_DIR" -maxdepth 1 -name '*.whl' -type f | sort)
[[ "${#WHEELS[@]}" == "1" ]] || { echo "expected exactly one wheel for $DISTRIBUTION $PYTHON_TAG" >&2; exit 1; }
WHEEL="${WHEELS[0]}"
[[ "$(basename "$WHEEL")" == *"-$PYTHON_TAG-"* ]] || { echo "wheel has unexpected Python tag: $WHEEL" >&2; exit 1; }
[[ "$(basename "$WHEEL")" == *"macosx"*"arm64.whl" ]] || { echo "wheel is not macOS arm64: $WHEEL" >&2; exit 1; }

"$PYTHON_BIN" -m venv --clear "$SMOKE_VENV"
"$SMOKE_VENV/bin/python" -m pip install --disable-pip-version-check "$WHEEL"
"$SMOKE_VENV/bin/python" "$SMOKE"
cp "$WHEEL" "$OUTPUT_DIR/"
printf '%s\n' "$OUTPUT_DIR/$(basename "$WHEEL")"
