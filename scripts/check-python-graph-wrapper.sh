#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRAPPER_DIR="$ROOT_DIR/wrappers/python-graph"
PYTHON_BIN="${PYTHON_BIN:-python3}"
SKIP_WHEEL=0

if [[ "${1:-}" == "--skip-wheel" ]]; then
  SKIP_WHEEL=1
elif [[ $# -ne 0 ]]; then
  echo "usage: $0 [--skip-wheel]" >&2
  exit 2
fi

PYTHON_TAG="$($PYTHON_BIN -c 'import sys; print(f"py{sys.version_info.major}{sys.version_info.minor}")')"
VENV_DIR="$ROOT_DIR/target/python-graph-wrapper-check-venv-$PYTHON_TAG"
BUILD_DIR="$ROOT_DIR/target/python-graph-wrapper-wheel-$PYTHON_TAG"
SMOKE_DIR="$ROOT_DIR/target/python-graph-wrapper-smoke-$PYTHON_TAG"

cargo test -p retrievalkit-python --features graph
"$PYTHON_BIN" -m venv "$VENV_DIR"
"$VENV_DIR/bin/python" -m pip install --disable-pip-version-check 'maturin==1.14.1' pytest mypy ruff
(
  cd "$WRAPPER_DIR"
  VIRTUAL_ENV="$VENV_DIR" "$VENV_DIR/bin/python" -m maturin develop
  "$VENV_DIR/bin/python" -m ruff check .
  "$VENV_DIR/bin/python" -m mypy
  "$VENV_DIR/bin/python" -m pytest
)

if [[ "$SKIP_WHEEL" == "0" ]]; then
  mkdir -p "$BUILD_DIR"
  (
    cd "$WRAPPER_DIR"
    "$VENV_DIR/bin/python" -m maturin build --locked --release --interpreter "$VENV_DIR/bin/python" --out "$BUILD_DIR"
  )
  WHEEL="$(find "$BUILD_DIR" -maxdepth 1 -name 'retrievalkit_graph-*.whl' -print -quit)"
  [[ -n "$WHEEL" ]] || { echo "graph wheel was not produced" >&2; exit 1; }
  "$PYTHON_BIN" -m venv "$SMOKE_DIR"
  "$SMOKE_DIR/bin/python" -m pip install --disable-pip-version-check --force-reinstall "$WHEEL"
  "$SMOKE_DIR/bin/python" "$WRAPPER_DIR/tests/smoke_installed.py"
fi
