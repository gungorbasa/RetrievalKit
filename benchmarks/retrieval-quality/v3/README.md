# VectorKit V3 Retrieval-Quality Fixture

Status: frozen conformance collection; Phase 1.1, Phase 1.2a, and Phase 1.2b
Run D are complete. Graph-scoped retrieval E-G and the publication manifest
are not complete.

This directory is the checked-in synthetic A-J collection defined by
`docs/product/graph-retrieval-evaluation-contract-v3.md`. Its seven records,
eight chunks, graph schema, embeddings, judgments, expected paths, exclusions,
and manifests are immutable qualification inputs. Do not edit them to make an
implementation pass. `collection.json` has frozen SHA-256
`0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65`.

The completed qualification slices are:

- Phase 1.1: schema, population, run-matrix, serialization, and determinism
  conformance;
- Phase 1.2a: production whole-corpus A-C retrieval, persistence, metrics, and
  independent Python rankings; and
- Phase 1.2b: production graph-only D selection for explicit, topic, and team
  seeds, projection/filtering into stable chunk identities, graph metrics,
  persistence equivalence, and an independent Python graph oracle.

Generate the current partial A-D qualification into a fresh ignored directory:

```bash
cargo run -p vectorkit-cli -- bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --qualification-artifacts target/benchmarks/v3/phase-1.2b-qualification \
  --verify-rerun

python3 scripts/quality/validate_v3_phase_1_2a.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts target/benchmarks/v3/phase-1.2b-qualification

python3 scripts/quality/validate_v3_phase_1_2b.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts target/benchmarks/v3/phase-1.2b-qualification
```

The output is deliberately `partial=true` and `publication_ready=false`. It
must not contain the final A-G `manifest.json` until E-G and the external
`ir_measures`/`trec_eval` publication gate are complete.
