#!/usr/bin/env bash
set -euo pipefail

PYTHON="${PYTHON:-target/embedding-conversion-venv/bin/python}"

if [[ ! -x "$PYTHON" ]]; then
  echo "Missing conversion Python at $PYTHON" >&2
  echo "Create it with: python3.11 -m venv target/embedding-conversion-venv" >&2
  exit 1
fi

for preset in \
  bge-small-en-v1.5 \
  all-MiniLM-L6-v2 \
  arctic-xs \
  arctic-s \
  e5-small-v2 \
  gte-small
do
  "$PYTHON" scripts/embedding/convert-embedding-coreml.py \
    --preset "$preset" \
    --compile \
    --verify
done
