#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 PYTHON BASE_WHEEL GRAPH_WHEEL" >&2
  exit 2
fi

PYTHON_BIN="$1"
BASE_WHEEL="$2"
GRAPH_WHEEL="$3"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEMP_ROOT"' EXIT

"$PYTHON_BIN" -m venv "$TEMP_ROOT/venv"
"$TEMP_ROOT/venv/bin/python" -m pip install --disable-pip-version-check "$BASE_WHEEL" "$GRAPH_WHEEL"

check_order() {
  local first="$1"
  local second="$2"
  "$TEMP_ROOT/venv/bin/python" - "$first" "$second" <<'PY'
import importlib
import sys

first, second = sys.argv[1:]
importlib.import_module(first)
try:
    importlib.import_module(second)
except ImportError as error:
    message = str(error)
    assert "mutually exclusive" in message and "one capability package per process" in message
else:
    raise SystemExit("base and graph imports unexpectedly coexisted")
PY
}

check_order retrievalkit retrievalkit_graph
check_order retrievalkit_graph retrievalkit
echo "Python base/graph co-installation diagnostic passed"
