#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRAPPER_DIR="$ROOT_DIR/wrappers/python"
VENV_DIR="$ROOT_DIR/target/python-wheel-venv"
WHEEL_DIR="$ROOT_DIR/target/wheels"
PYTHON_BIN="${PYTHON_BIN:-python3}"
BUILD_MODE="--release"

usage() {
  cat <<'EOF'
usage:
  scripts/build-python-wheel.sh [--debug]

Builds a local Python wheel for the VectorKit Python wrapper.

The wheel is written to:
  target/wheels/

Options:
  --debug     build an unoptimized debug wheel
  --help, -h  show this help

Environment:
  PYTHON_BIN  Python interpreter used to create the build virtualenv; default python3

Install the produced wheel with:
  python -m pip install target/wheels/vectorkit-*.whl

Note: wheels with native Rust extensions are platform and Python ABI specific.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)
      BUILD_MODE=""
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required tool not found: $1" >&2
    exit 1
  fi
}

main() {
  require_tool cargo
  require_tool "$PYTHON_BIN"

  if [[ ! -f "$WRAPPER_DIR/pyproject.toml" ]]; then
    echo "missing Python wrapper pyproject: $WRAPPER_DIR/pyproject.toml" >&2
    exit 1
  fi

  "$PYTHON_BIN" -m venv "$VENV_DIR"
  "$VENV_DIR/bin/python" -m pip install --upgrade pip maturin

  mkdir -p "$WHEEL_DIR"
  (
    cd "$WRAPPER_DIR"
    "$VENV_DIR/bin/maturin" build $BUILD_MODE --out "$WHEEL_DIR"
  )

  echo "Built Python wheel(s):"
  find "$WHEEL_DIR" -maxdepth 1 -name 'vectorkit-*.whl' -print | sort
}

main "$@"
