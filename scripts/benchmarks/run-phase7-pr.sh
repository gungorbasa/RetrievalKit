#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
result_parent="$(mktemp -d)"
trap 'rm -rf "$result_parent"' EXIT

cd "$repo_root"
cargo test --locked -p retrievalkit-graph --test regression_gates
python3 benchmarks/regression/validate_gates.py --repo "$repo_root"
python3 benchmarks/regression/run_gates.py \
  --tier pull_request \
  --output "$result_parent/root-a"
python3 benchmarks/regression/run_gates.py \
  --tier pull_request \
  --output "$result_parent/root-b"
python3 benchmarks/regression/validate_gates.py \
  --repo "$repo_root" \
  --result-root "$result_parent/root-a" \
  --compare-root "$result_parent/root-b"
