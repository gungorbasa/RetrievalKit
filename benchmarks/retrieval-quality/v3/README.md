# RetrievalKit V3 Retrieval-Quality Fixture

> [RetrievalKit](../../../README.md) › Benchmarks › Retrieval quality › V3

**Status:** frozen synthetic conformance collection. Treat every checked-in
identity and expected result as immutable qualification input.

Graph-aware evaluation-artifact Phase 1 is complete. The Phase 1.1 and Phase
1.2a-c qualification artifacts remain intentionally partial, while a separate
release-context publication pipeline now verifies official `trec_eval` and
assembles the closed public layout.

This directory is the checked-in synthetic A-J collection defined by
`docs/product/graph-retrieval-evaluation-contract-v3.md`. Its seven records,
eight chunks, graph schema, embeddings, judgments, expected paths, exclusions,
and manifests are immutable qualification inputs. Do not edit them to make an
implementation pass. `collection.json` has frozen SHA-256
`0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65`.
Its `vectorkit-v3-*` collection, corpus, tool, and archive identifiers are
legacy fixture identities covered by that digest; the RetrievalKit product
rename does not rewrite frozen qualification inputs.

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

## Generate qualification evidence

Generate the complete A-G qualification into a fresh ignored directory:

```bash
ARTIFACTS=target/benchmarks/v3/phase-1.2c-qualification

cargo run -p retrievalkit-cli -- bench quality-v3 \
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

## Failure semantics

The evaluator always emits one result and metric row per declared query.
Successful executions retain their deterministic results. A classified
query-local contract failure emits an `invalid_execution` row only for that
query. Generation, stale-selection, persistence, reload, nondeterministic
ranking, and run-wide contract failures invalidate every attempted row in the
affected run while preserving `excluded_pre_freeze` rows. Invalid rows have no
hits or projected documents, contribute no metric value, and emit no TREC,
selection, or path rows. Unrelated runs remain valid.

## Publication boundary

The finalizer requires exactly 56 files before its own index, rejects missing
or unexpected paths, and verifies an existing stored index against a fresh
rebuild. The valid frozen set has artifact SHA-256
`ee264e919ab5872fd400354f5aa332993fd55fdedcaab400e6f5ba41619f631c`.
The qualification marker remains `partial=true` and
`publication_ready=false`; no final `manifest.json` is emitted into this
qualification directory. That is intentional: the release-context public
artifact is assembled into a fresh root only after every gate passes:

```bash
python3 scripts/quality/bootstrap_v3_trec_eval.py

uv run --python 3.13 --with ir_measures==0.4.3 \
  python scripts/quality/assemble_v3_publication.py \
  --collection benchmarks/retrieval-quality/v3 \
  --executable target/release/retrievalkit \
  --qualification-output target/benchmarks/v3/release-qualification \
  --output target/benchmarks/v3/publication \
  --gate-report-root target/benchmarks/v3/publication-gates
```

The resulting public root has exactly 44 files, including a 43-entry
`manifest.json`, and contains no qualification markers, cross-check reports,
or intermediate files. See
`docs/product/reports/graph-retrieval-phase-1-publication-report.md`. Phase 2
and all public quality, performance, device, and marketing claims remain out of
scope.
