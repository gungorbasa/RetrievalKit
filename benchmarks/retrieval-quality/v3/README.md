# VectorKit V3 Retrieval-Quality Fixture

Status: frozen conformance collection; Phase 1.1 and the complete Phase 1.2a,
1.2b, and 1.2c A-G qualification are complete. The artifacts remain partial
and non-publication-ready until official `trec_eval` and final public
`manifest.json` assembly are completed.

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
  independent Python rankings;
- Phase 1.2b: production graph-only D selection for explicit, topic, and team
  seeds, projection/filtering, graph metrics, persistence equivalence, and an
  independent Python graph oracle; and
- Phase 1.2c: nine production graph-scoped E-G runs, paired comparisons,
  combined persistence, independent reconstruction, and pinned `ir_measures`.

Generate the complete A-G qualification into a fresh ignored directory:

```bash
ARTIFACTS=target/benchmarks/v3/phase-1.2c-qualification

cargo run -p vectorkit-cli -- bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --qualification-artifacts "$ARTIFACTS" \
  --verify-rerun
```

Run every current qualification and add only the two reports in the frozen
56-file inventory:

```bash
python3 scripts/quality/validate_v3_phase_1_2a.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts "$ARTIFACTS" --check-only

python3 scripts/quality/validate_v3_phase_1_2b.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts "$ARTIFACTS" --check-only

python3 scripts/quality/validate_v3_phase_1_2c.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts "$ARTIFACTS"

uv run --python 3.13 --with ir_measures==0.4.3 \
  python scripts/quality/validate_v3_ir_measures.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts "$ARTIFACTS"

python3 scripts/quality/finalize_v3_phase_1_2c_artifacts.py \
  --artifacts "$ARTIFACTS"
python3 scripts/quality/finalize_v3_phase_1_2c_artifacts.py \
  --artifacts "$ARTIFACTS" --check-only
```

The evaluator always emits one result and metric row per declared query.
Successful executions retain their deterministic results. A classified
query-local contract failure emits an `invalid_execution` row only for that
query. Generation, stale-selection, persistence, reload, nondeterministic
ranking, and run-wide contract failures invalidate every attempted row in the
affected run while preserving `excluded_pre_freeze` rows. Invalid rows have no
hits or projected documents, contribute no metric value, and emit no TREC,
selection, or path rows. Unrelated runs remain valid.

The finalizer requires exactly 56 files before its own index, rejects missing
or unexpected paths, and verifies an existing stored index against a fresh
rebuild. The valid frozen set has artifact SHA-256
`ee264e919ab5872fd400354f5aa332993fd55fdedcaab400e6f5ba41619f631c`.
The qualification marker remains `partial=true` and
`publication_ready=false`; no final `manifest.json` is emitted. Official
`trec_eval` and final public-manifest assembly are the next separate Phase 1
task. These synthetic results are not marketing claims.
