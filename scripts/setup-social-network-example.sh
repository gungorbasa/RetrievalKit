#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE_DIR="$ROOT_DIR/examples/python/social_network_search"
VENV_DIR="$ROOT_DIR/target/social-network-example-venv"
PYTHON_BIN="${PYTHON_BIN:-python3.11}"

usage() {
  cat <<'EOF'
usage:
  scripts/setup-social-network-example.sh

Creates target/social-network-example-venv and installs:
  - the local vectorkit wheel
  - FastEmbed example dependencies

Environment:
  PYTHON_BIN  Python interpreter for the example environment; default python3.11
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required tool not found: $1" >&2
    exit 1
  fi
}

main() {
  require_tool "$PYTHON_BIN"

  local cp_tag
  cp_tag="$("$PYTHON_BIN" - <<'PY'
import sys
print(f"cp{sys.version_info.major}{sys.version_info.minor}")
PY
)"

  PYTHON_BIN="$PYTHON_BIN" "$ROOT_DIR/scripts/build-python-wheel.sh"

  local wheel
  wheel="$(find "$ROOT_DIR/target/wheels" -maxdepth 1 -name "vectorkit-*-${cp_tag}-${cp_tag}-*.whl" | sort | tail -n 1)"
  if [[ -z "$wheel" ]]; then
    echo "could not find vectorkit wheel for $cp_tag in target/wheels" >&2
    exit 1
  fi

  "$PYTHON_BIN" -m venv "$VENV_DIR"
  "$VENV_DIR/bin/python" -m pip install --upgrade pip
  "$VENV_DIR/bin/python" -m pip install --force-reinstall "$wheel"
  "$VENV_DIR/bin/python" -m pip install -r "$EXAMPLE_DIR/requirements.txt"

  echo "Created example environment: $VENV_DIR"
  echo "Run:"
  echo "  $VENV_DIR/bin/python examples/python/social_network_search/social_network_search.py --rebuild"
}

main "$@"
