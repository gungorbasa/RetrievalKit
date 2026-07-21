# Phase 6 benchmark publication

This directory generates and independently validates the frozen,
repository-local Phase 6 benchmark publication package.

The checked-in package is under `artifacts/phase6-publication-v1`. Large raw
Phase 3 and Phase 4b evidence remains untracked under `target/`; Phase 5's
frozen compact artifacts remain under `benchmarks/external-reference`.

Generate:

```sh
python3 benchmarks/publication/generate_publication.py \
  --repo . \
  --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting \
  --phase4-root target/phase4b/device-results-v3-02b8971 \
  --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 \
  --output /tmp/phase6-publication-v1
```

Validate:

```sh
python3 benchmarks/publication/validate_publication.py \
  --repo . \
  --phase3-root target/benchmarks/hotpotqa-phase-3b/locked-reporting \
  --phase4-root target/phase4b/device-results-v3-02b8971 \
  --phase5-root benchmarks/external-reference/artifacts/mac-comparison-v1 \
  --root benchmarks/publication/artifacts/phase6-publication-v1
```

Neither command performs network access, physical-device work, or benchmark
retuning. Generation is a pure transformation of frozen evidence.
