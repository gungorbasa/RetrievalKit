# Phase 7 regression gates

Run the deterministic PR gate from the repository root:

```bash
scripts/benchmarks/run-phase7-pr.sh
```

The command runs the production-backed Rust smoke test, validates the static
contract/registry/fixture and frozen Phase 4–6 identities, generates two fresh
result roots, and proves byte identity.

Scheduled and release tiers consume a canonical observation JSON with
`inputs`, `metrics`, and `platform` objects. `inputs.provisioned` must contain
every required input token listed for the tier in `gate-registry-v1.json`.
Scheduled observations must also bind `inputs.frozen_inputs` to every identity
in `baselines-v1.json`. Release observations must name device identifier, OS,
toolchain, source revision, and sample count.

```bash
python3 benchmarks/regression/run_gates.py \
  --tier scheduled_full \
  --observation /controlled/path/full-observation.json \
  --output target/phase7-full-result
```

```bash
python3 benchmarks/regression/run_gates.py \
  --tier release \
  --observation /controlled/path/release-observation.json \
  --output target/phase7-release-result
```

If a scheduled or release observation is unavailable, omit `--observation`.
The runner writes a deterministic `not_provisioned` result and exits 2. It
never converts missing licensed data or controlled hardware into a pass.

The manual release workflow separately requires an authorization JSON with
the exact fields enforced by `validate_release_authorization.py`. It binds the
observation SHA-256, permits evidence validation only, authorizes no device
commands, and lists exactly 10K/25K/50K and F32/I8. There is no 100K option.
