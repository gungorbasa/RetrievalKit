#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VENV_DIR="$ROOT_DIR/target/phase5-external-venv"
WHEEL_DIR="$ROOT_DIR/target/phase5-external-wheels"
PYTHON_VERSION="3.12.12"
MATURIN_VERSION="1.14.1"

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required tool not found: $1" >&2
    exit 1
  fi
}

require_tool uv
require_tool cargo

uv venv --clear --python "$PYTHON_VERSION" "$VENV_DIR"
uv pip sync \
  --python "$VENV_DIR/bin/python" \
  --require-hashes \
  "$ROOT_DIR/benchmarks/external-reference/requirements.lock.txt"

mkdir -p "$WHEEL_DIR"

(
  cd "$ROOT_DIR/wrappers/python"
  uv tool run --from "maturin==$MATURIN_VERSION" maturin build \
    --release \
    --interpreter "$VENV_DIR/bin/python" \
    --out "$WHEEL_DIR"
)

(
  cd "$ROOT_DIR/wrappers/python-graph"
  uv tool run --from "maturin==$MATURIN_VERSION" maturin build \
    --release \
    --interpreter "$VENV_DIR/bin/python" \
    --out "$WHEEL_DIR"
)

base_wheel=()
graph_wheel=()
while IFS= read -r wheel; do
  base_wheel+=("$wheel")
done < <(find "$WHEEL_DIR" -maxdepth 1 -name 'vectorkit-*.whl' ! -name 'vectorkit_graph-*' -print | sort)
while IFS= read -r wheel; do
  graph_wheel+=("$wheel")
done < <(find "$WHEEL_DIR" -maxdepth 1 -name 'vectorkit_graph-*.whl' -print | sort)

if [[ ${#base_wheel[@]} -ne 1 || ${#graph_wheel[@]} -ne 1 ]]; then
  echo "expected exactly one base wheel and one graph wheel" >&2
  exit 1
fi

uv pip install \
  --python "$VENV_DIR/bin/python" \
  --no-deps \
  "${base_wheel[0]}" \
  "${graph_wheel[0]}"

"$VENV_DIR/bin/python" - <<'PY'
import importlib.metadata
import numpy
import sqlite_vec
import usearch

print("numpy", numpy.__version__)
print("sqlite-vec", importlib.metadata.version("sqlite-vec"))
print("usearch", usearch.__version__)
print("vectorkit", importlib.metadata.version("vectorkit"))
print("vectorkit-graph", importlib.metadata.version("vectorkit-graph"))
PY

echo "Phase 5 environment ready: $VENV_DIR"
