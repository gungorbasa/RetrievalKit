#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRAPPER_DIR="$ROOT_DIR/wrappers/python"
PYTHON_BIN="${PYTHON_BIN:-python3}"
SKIP_WHEEL=0

usage() {
  cat <<'EOF'
usage:
  scripts/check-python-wrapper.sh [--skip-wheel]

Runs the local Python wrapper quality checks in an isolated developer
environment. This does not add any runtime package dependencies.

Checks:
  cargo test -p vectorkit-python
  maturin develop
  ruff check .
  mypy
  pytest
  scripts/build-python-wheel.sh --debug

Options:
  --skip-wheel  skip the final wheel build and installed-wheel smoke test
  --help, -h    show this help

Environment:
  PYTHON_BIN  Python interpreter used to create the check virtualenv; default python3
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-wheel)
      SKIP_WHEEL=1
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

venv_python() {
  local venv_dir="$1"
  if [[ -x "$venv_dir/bin/python" ]]; then
    echo "$venv_dir/bin/python"
  elif [[ -x "$venv_dir/Scripts/python.exe" ]]; then
    echo "$venv_dir/Scripts/python.exe"
  else
    echo "could not find python executable in virtualenv: $venv_dir" >&2
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

  local python_tag
  python_tag="$("$PYTHON_BIN" - <<'PY'
import sys
print(f"py{sys.version_info.major}{sys.version_info.minor}")
PY
  )"
  local venv_dir="$ROOT_DIR/target/python-wrapper-check-venv-$python_tag"

  cargo test -p vectorkit-python

  "$PYTHON_BIN" -m venv "$venv_dir"
  local check_python
  check_python="$(venv_python "$venv_dir")"
  "$check_python" -m pip install --upgrade pip maturin pytest mypy ruff

  (
    cd "$WRAPPER_DIR"
    VIRTUAL_ENV="$venv_dir" "$check_python" -m maturin develop
    "$check_python" -m ruff check .
    "$check_python" -m mypy
    "$check_python" -m pytest
    "$check_python" examples/database_quickstart.py
  )

  if [[ "$SKIP_WHEEL" -eq 0 ]]; then
    PYTHON_BIN="$check_python" "$ROOT_DIR/scripts/build-python-wheel.sh" --debug
  fi
}

main "$@"
