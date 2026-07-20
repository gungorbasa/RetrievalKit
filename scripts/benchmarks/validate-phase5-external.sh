#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_BIN="$ROOT_DIR/target/phase5-external-venv/bin/python"

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <phase5-artifact-root>" >&2
  exit 2
fi

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "missing Phase 5 environment; run scripts/benchmarks/setup-phase5-external.sh" >&2
  exit 1
fi

"$PYTHON_BIN" "$ROOT_DIR/benchmarks/external-reference/validate_artifacts.py" \
  --root "$1"

