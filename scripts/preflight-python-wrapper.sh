#!/usr/bin/env bash
set -euo pipefail

PYTHON_BIN="${PYTHON_BIN:-python3}"

fail() {
  echo "Python wrapper preflight failed: $*" >&2
  exit 1
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail "required tool '$1' was not found on PATH."
  fi
}

require_tool "$PYTHON_BIN"
require_tool cargo

PYTHON_VERSION="$("$PYTHON_BIN" -c 'import platform; print(platform.python_version())')"
PYTHON_COMPATIBLE="$("$PYTHON_BIN" -c 'import sys; print("yes" if (3, 10) <= sys.version_info[:2] <= (3, 14) else "no")')"
if [[ "$PYTHON_COMPATIBLE" != "yes" ]]; then
  fail "Python 3.10-3.14 is required; detected Python $PYTHON_VERSION from '$PYTHON_BIN'."
fi

if ! "$PYTHON_BIN" -c 'import venv' >/dev/null 2>&1; then
  fail "Python $PYTHON_VERSION cannot import venv; install this interpreter's venv support."
fi

HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
CARGO_VERSION="$(cargo --version)"

echo "Python wrapper preflight passed"
echo "  Python: required 3.10-3.14; detected $PYTHON_VERSION ($PYTHON_BIN)"
echo "  Rust: required cargo on PATH; detected $CARGO_VERSION"
echo "  Host: detected $HOST_OS $HOST_ARCH"
if [[ "$HOST_OS" != "Darwin" || "$HOST_ARCH" != "arm64" ]]; then
  echo "  Support: source validation may work here, but the initial public wheel target remains macOS arm64."
else
  echo "  Support: matches the initial macOS arm64 wheel target."
fi
