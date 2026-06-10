#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRAPPER_DIR="$ROOT_DIR/wrappers/python"
WHEEL_DIR="$ROOT_DIR/target/wheels"
PYTHON_BIN="${PYTHON_BIN:-python3}"
BUILD_MODE="--release"
SMOKE_TEST=1

usage() {
  cat <<'EOF'
usage:
  scripts/build-python-wheel.sh [--debug] [--skip-smoke-test]

Builds a local Python wheel for the VectorKit Python wrapper.

The wheel is written to:
  target/wheels/

By default, the script installs the wheel it just built into a clean virtual
environment and runs wrappers/python/tests/smoke_installed.py.

Options:
  --debug            build an unoptimized debug wheel
  --skip-smoke-test  skip installing and smoke-testing the built wheel
  --help, -h         show this help

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
    --skip-smoke-test)
      SMOKE_TEST=0
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

  local python_tag
  python_tag="$("$PYTHON_BIN" - <<'PY'
import sys
print(f"py{sys.version_info.major}{sys.version_info.minor}")
PY
  )"
  local venv_dir="$ROOT_DIR/target/python-wheel-venv-$python_tag"
  local build_wheel_dir="$ROOT_DIR/target/python-wheel-build-$python_tag"
  local smoke_venv_dir="$ROOT_DIR/target/python-wheel-smoke-venv-$python_tag"

  if [[ ! -f "$WRAPPER_DIR/pyproject.toml" ]]; then
    echo "missing Python wrapper pyproject: $WRAPPER_DIR/pyproject.toml" >&2
    exit 1
  fi

  "$PYTHON_BIN" -m venv "$venv_dir"
  "$venv_dir/bin/python" -m pip install --upgrade pip maturin

  rm -rf "$build_wheel_dir"
  mkdir -p "$build_wheel_dir"
  mkdir -p "$WHEEL_DIR"
  (
    cd "$WRAPPER_DIR"
    "$venv_dir/bin/maturin" build $BUILD_MODE --interpreter "$venv_dir/bin/python" --out "$build_wheel_dir"
  )

  local built_wheels=()
  while IFS= read -r wheel; do
    built_wheels+=("$wheel")
  done < <(find "$build_wheel_dir" -maxdepth 1 -name 'vectorkit-*.whl' -print | sort)

  if [[ ${#built_wheels[@]} -eq 0 ]]; then
    echo "maturin completed but no vectorkit wheel was produced in $build_wheel_dir" >&2
    exit 1
  fi

  if [[ "$SMOKE_TEST" -eq 1 ]]; then
    rm -rf "$smoke_venv_dir"
    "$PYTHON_BIN" -m venv "$smoke_venv_dir"
    "$smoke_venv_dir/bin/python" -m pip install --upgrade pip
    "$smoke_venv_dir/bin/python" -m pip install --force-reinstall "${built_wheels[0]}"
    "$smoke_venv_dir/bin/python" "$ROOT_DIR/wrappers/python/tests/smoke_installed.py"
    echo "Smoke-tested installed wheel with $("$smoke_venv_dir/bin/python" --version)"
  fi

  for wheel in "${built_wheels[@]}"; do
    cp "$wheel" "$WHEEL_DIR/"
  done

  echo "Built Python wheel(s):"
  for wheel in "${built_wheels[@]}"; do
    echo "$WHEEL_DIR/$(basename "$wheel")"
  done
}

main "$@"
