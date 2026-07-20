#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_BIN="$ROOT_DIR/target/phase5-external-venv/bin/python"
PROFILE="${1:-smoke}"

case "$PROFILE" in
  smoke)
    config="smoke-v1.json"
    output="smoke-v1"
    ;;
  development)
    config="mac-development-v1.json"
    output="mac-development-v1"
    ;;
  comparison)
    config="mac-comparison-v1.json"
    output="mac-comparison-v1"
    ;;
  *)
    echo "usage: $0 [smoke|development|comparison]" >&2
    exit 2
    ;;
esac

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "missing Phase 5 environment; run scripts/benchmarks/setup-phase5-external.sh" >&2
  exit 1
fi

"$PYTHON_BIN" "$ROOT_DIR/benchmarks/external-reference/run_phase5.py" \
  --config "$ROOT_DIR/benchmarks/external-reference/configs/$config" \
  --output "$ROOT_DIR/target/benchmarks/phase5/$output" \
  --python "$PYTHON_BIN"

"$PYTHON_BIN" "$ROOT_DIR/benchmarks/external-reference/validate_artifacts.py" \
  --root "$ROOT_DIR/target/benchmarks/phase5/$output" \
  --output "$ROOT_DIR/target/benchmarks/phase5/$output-validation.json"

