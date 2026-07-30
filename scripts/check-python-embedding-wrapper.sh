#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WRAPPER_DIR="$ROOT_DIR/wrappers/python-embedding"
PYTHON_BIN="${PYTHON_BIN:-python3}"
SKIP_WHEEL=0

if [[ "${1:-}" == "--skip-wheel" ]]; then
  SKIP_WHEEL=1
elif [[ $# -ne 0 ]]; then
  echo "usage: $0 [--skip-wheel]" >&2
  exit 2
fi

PYTHON_BIN="$PYTHON_BIN" "$ROOT_DIR/scripts/preflight-python-wrapper.sh"

PYTHON_TAG="$("$PYTHON_BIN" -c \
  'import sys; print(f"py{sys.version_info.major}{sys.version_info.minor}")')"
VENV_DIR="$ROOT_DIR/target/python-embedding-check-venv-$PYTHON_TAG"
BUILD_DIR="$ROOT_DIR/target/python-embedding-wheel-$PYTHON_TAG"
SMOKE_DIR="$ROOT_DIR/target/python-embedding-smoke-$PYTHON_TAG"

cargo test --locked -p retrievalkit-python-embedding
"$PYTHON_BIN" -m venv "$VENV_DIR"
"$VENV_DIR/bin/python" -m pip install --disable-pip-version-check \
  'maturin==1.14.1' pytest mypy ruff

if [[ -n "${RETRIEVALKIT_ONNX_RUNTIME_LIBRARY:-}" ]]; then
  "$VENV_DIR/bin/python" "$WRAPPER_DIR/prepare_runtime.py"
fi

(
  cd "$WRAPPER_DIR"
  VIRTUAL_ENV="$VENV_DIR" "$VENV_DIR/bin/python" -m maturin develop --locked
  "$VENV_DIR/bin/python" -m ruff check .
  "$VENV_DIR/bin/python" -m mypy
  "$VENV_DIR/bin/python" -m pytest
)

if [[ "$SKIP_WHEEL" == "1" ]]; then
  exit 0
fi

if [[ -z "${RETRIEVALKIT_ONNX_RUNTIME_LIBRARY:-}" ]]; then
  echo "production wheel validation requires RETRIEVALKIT_ONNX_RUNTIME_LIBRARY" >&2
  exit 1
fi

mkdir -p "$BUILD_DIR"
(
  cd "$WRAPPER_DIR"
  "$VENV_DIR/bin/python" -m maturin build --locked --release \
    --interpreter "$VENV_DIR/bin/python" --out "$BUILD_DIR"
)
WHEEL="$(find "$BUILD_DIR" -maxdepth 1 \
  -name 'retrievalkit_embedding-*.whl' -print -quit)"
[[ -n "$WHEEL" ]] || {
  echo "Python embedding wheel was not produced" >&2
  exit 1
}

"$VENV_DIR/bin/python" - "$WHEEL" <<'PY'
import sys
import zipfile

required_suffixes = {
    "retrievalkit_embedding/runtime/libonnxruntime.1.24.3.dylib",
    "retrievalkit_embedding/runtime/LICENSE",
    "retrievalkit_embedding/runtime/ThirdPartyNotices.txt",
}
with zipfile.ZipFile(sys.argv[1]) as wheel:
    names = set(wheel.namelist())
missing = {
    required for required in required_suffixes
    if not any(name.endswith(required) for name in names)
}
if missing:
    raise SystemExit(f"embedding wheel is missing bundled runtime files: {sorted(missing)}")
PY

"$PYTHON_BIN" -m venv "$SMOKE_DIR"
"$SMOKE_DIR/bin/python" -m pip install --disable-pip-version-check \
  --force-reinstall "$WHEEL"
(
  cd "$ROOT_DIR"
  "$SMOKE_DIR/bin/python" "$WRAPPER_DIR/tests/smoke_installed.py"
)
