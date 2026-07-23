# Graph Retrieval Phase 1 Retrieval Baselines Report

Status: Phase 1.2a complete; overall Phase 1 remains active

Date: 2026-07-16

## Scope and outcome

Phase 1.2a qualifies only the frozen whole-corpus retrieval baselines:

- A: production F32 semantic retrieval;
- B: production I8 semantic retrieval; and
- C: production I8 weighted-hybrid retrieval.

All three runs passed. The evaluation-only adapter feeds the existing
`CorpusIndex` and `RetrievalDatabase`; semantic scoring, I8 quantization, BM25,
metadata filtering, weighted fusion, trace generation, and persistence remain
owned by production `retrievalkit-core` code. No production public API, Swift
wrapper, Python wrapper, fixture byte, collection identifier, population, or
15-run configuration was changed.

This is a partial qualification, not the completed V3 benchmark. It emits no
final `manifest.json`, makes no graph-retrieval claim, and does not execute D-G.

## Revision history

The required starting commit was
`a960dc947f52c319e32b135e95fb35d87000a0f8` on
`codex/graph-m1-core-foundation`. Phase 1.1 was not amended or rewritten.

| Commit | Message | Scope |
| --- | --- | --- |
| `2e653d294ddd8016c828e7706b45d0726f71e80c` | `Map V3 collections into retrieval inputs` | Frozen-hash guard, record/chunk/query/filter adapter, production F32/I8 database ingestion, and adapter tests |
| `35a3d9700009a6e5ad96b960af02b7714e71f6cc` | `Execute V3 whole-corpus retrieval runs` | A-C execution, exhaustive native hits, projection, metrics, TREC output, save/load equivalence, and partial artifacts |
| `a20440aac47e4cb64175fe4ee2a71e6f48e5675a` | `Cross-check V3 retrieval rankings` | Independent F32/I8/BM25/hybrid calculator, exact ranking/TREC checks, and byte-rerun proof |
| `e291dec1b5377e6880c6df4a6e06b8b538eaf45c` | `Cross-check V3 retrieval metrics` | Independent per-query/macro metric checks and malformed-input tests |

The documentation commit is necessarily self-referential and is identified by
the commit containing this report; its full SHA is recorded in the final
handoff.

## Frozen inputs and populations

| Input | SHA-256 |
| --- | --- |
| Checked-in `collection.json` | `0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65` |
| Normative A-J fixture | `4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46` |
| Whole-corpus retrieval population R | `c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3` |
| Qrels | `140c97c2bcb8a65486114c6b7802a7da6348f3e96e8186c268f265b0f17a4994` |

R is exactly `qa qb qd qf qg qh qi`. Each A-C run declares and attempts all
seven queries, with seven valid executions, zero invalid executions, and zero
pre-freeze exclusions. The `qf` and `qg` exclusions apply only to the
topic-derived graph lane and do not exclude them from whole-corpus A-C.

## Run identities and configurations

All runs use collection `retrievalkit-v3-conformance` version `1.0.0`, corpus
`retrievalkit-v3-synthetic-corpus`, cosine scoring, unit-L2 normalization, the
frozen query filters, `top_k=10`, evaluation depth 10, no graph/seed/traversal
hashes, and the R population above.

| Run | Stable run ID | Logical-run SHA-256 | Encoding/mode | Candidate limits | Alpha |
| --- | --- | --- | --- | --- | ---: |
| A | `v3-a-whole-semantic-f32-na-cfg-984e4c3bf991` | `bf237c1a474816a1f8c8dcb0580694c19ccd53cb5420c99b0419c3dd8bba2711` | F32 semantic | none | n/a |
| B | `v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53` | `e0b946e2b8c926badacc6f6fa104d52c33f72f6e8408820f969b59f5d6a6261b` | I8 semantic | none | n/a |
| C | `v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0` | `df48c1d3a962997bf21f037c6eae1905ed423576933da54dde749b9170af0b21` | I8 weighted hybrid | vector 8, keyword 8 | 0.6 |

B and C use frozen quantization-policy SHA-256
`b7c0bb0252ea789e5810630e2e995aec0a75f635dc4880651db5402c0b2b4881`.
C uses production BM25 defaults `k1=1.2`, `b=0.75`, no stop words, and the
frozen Unicode-word/lowercase policy.

## Production execution and projection

The adapter ingests records and chunks in ascending stable
`(record_id, chunk_key)` order. Production assigns the corresponding numeric
chunk IDs, owns the canonical payload, merges inherited and chunk metadata,
normalizes vectors, constructs F32 or I8 retrieval state, and indexes chunk
text for BM25.

Semantic A/B request every active chunk before applying the unchanged query
filter. Weighted C requests the complete duplicate-free union permitted by its
8/8 vector and keyword candidate limits. Projection preserves native chunk
order, retains the first chunk for each record, stops at 10 unique records or
actual exhaustion, and writes rank-derived TREC scores
`evaluation_depth - rank + 1` while retaining native scores in diagnostics.

Duplicate collapse totals were identical across A-C:

| Run | `qa` | `qf` | `qg` | Total |
| --- | ---: | ---: | ---: | ---: |
| A | 1 | 1 | 1 | 3 |
| B | 1 | 1 | 1 | 3 |
| C | 1 | 1 | 1 | 3 |

All other queries collapsed zero chunks. Filtered queries `qb`, `qd`, and `qh`
returned only red-tenant chunks; `qi` returned only blue-tenant chunks.

## Retrieval metrics

These are macro means over the seven valid R queries. AP is reported as MAP in
the aggregate row.

| Run | NDCG@5 | NDCG@10 | Recall@5 | Recall@10 | Success@1 | Precision@5 | MRR@10 | MAP | Judged@5 | Judged@10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A | 0.8420881416271495 | 0.8420881416271495 | 1.0 | 1.0 | 0.7142857142857143 | 0.22857142857142856 | 0.7976190476190477 | 0.7738095238095237 | 0.3714285714285714 | 0.4081632653061224 |
| B | 0.8420881416271495 | 0.8420881416271495 | 1.0 | 1.0 | 0.7142857142857143 | 0.22857142857142856 | 0.7976190476190477 | 0.7738095238095237 | 0.3714285714285714 | 0.4081632653061224 |
| C | 0.8655095840190602 | 0.8655095840190602 | 1.0 | 1.0 | 0.7142857142857143 | 0.22857142857142856 | 0.8571428571428571 | 0.8571428571428571 | 0.3714285714285714 | 0.4081632653061224 |

The independent Python implementation recalculated all 21 query/run rankings,
F32 and I8 scores, BM25 scores and terms, normalized components, fusion scores,
filters, projection, TREC rows, ten per-query retrieval metrics, and all three
macro rows from the frozen inputs. Order, ranks, stable identities, terms,
projection, TREC bytes, and metrics agree exactly. Maximum absolute differences
were `2.9802322387695312e-08` for f32 diagnostic scores and `0.0` for metrics,
within the declared cross-implementation score tolerance of `2e-7`.

The repository's optional `ir_measures` validator was not run because
`ir_measures` is not installed in this environment and that validator currently
accepts the V2 schema, not the explicit Phase 1.2a partial metric schema. The
new independent validator covers the same TREC rows and retrieval metrics
without using Rust-produced expected values.

## Persistence and deterministic artifacts

For each of A-C, Rust built and executed a fresh production database, saved it
through `RetrievalDatabase::save_to_dir`, validated it, loaded it through
`RetrievalDatabase::load_from_dir`, and re-executed the complete run. Corpus
generation, vector configuration, stable numeric/stable chunk identities,
native ordering, filters, traces, projection, duplicate counts, TREC rows, and
metrics were exactly equal before and after reload.

Two fresh Phase 1.2a emissions compared byte-for-byte in lexical path order.
Both deterministic file sets had SHA-256
`dc63175fdc76281d5ab5ea2588e09400cd47205f529824d437632fe8122d49ed`
over the canonical sorted file index. Principal file hashes were:

| File | SHA-256 |
| --- | --- |
| `rust-results.json` | `a27437b3e72bdaf7cfa50ac5e77d36518e44d85701a979863cc0b36dbaba2f5f` |
| `metrics.json` | `dd0bf5052264cad9f0c6ef790b24cd68daf253a701affacc95e0dcab8b5b1fe1` |
| A TREC | `163afb17698a52cb3a46b2a86f8e34302a34f9896b6f8ce1d24cc9cd4c8348f3` |
| B TREC | `42eb57de32078fcc9038340607da579ac2f41d2e21e6fb94640eb30af3c1b9c8` |
| C TREC | `3610b185fddba9ed3fdc1b59a36831640c58aa94a12aef1db42a4ca21d108252` |
| `independent-cross-check.json` | `2177488b1d2298ab6d3d7fccf10bdbe7d723e2c9c41bfebc85e022a1eb0f0fae` |

Generated qualification artifacts remain ignored under
`target/benchmarks/v3/`; they are marked `partial=true`,
`publication_ready=false`, and `qualification_only_no_final_manifest`.

## Verification

The completed gates were:

```bash
cargo fmt --all -- --check
cargo test -p retrievalkit-cli
cargo clippy -p retrievalkit-cli --all-targets -- -D warnings

cargo run -p retrievalkit-cli -- bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --foundation-artifacts target/benchmarks/v3/phase-1.1-foundation-final \
  --verify-rerun
python3 scripts/quality/validate_v3_conformance.py \
  --collection benchmarks/retrieval-quality/v3 \
  --foundation-artifacts target/benchmarks/v3/phase-1.1-foundation-final

cargo run -p retrievalkit-cli -- bench quality-v3 \
  --collection benchmarks/retrieval-quality/v3 \
  --qualification-artifacts target/benchmarks/v3/phase-1.2a-qualification-final \
  --verify-rerun
python3 scripts/quality/validate_v3_phase_1_2a.py \
  --collection benchmarks/retrieval-quality/v3 \
  --artifacts target/benchmarks/v3/phase-1.2a-qualification-final

python3 -m unittest scripts.quality.test_validate_v3_phase_1_2a
python3 -m py_compile scripts/quality/validate_v3_phase_1_2a.py \
  scripts/quality/test_validate_v3_phase_1_2a.py \
  scripts/quality/validate_v3_conformance.py
ruff check scripts/quality/validate_v3_phase_1_2a.py \
  scripts/quality/test_validate_v3_phase_1_2a.py \
  scripts/quality/validate_v3_conformance.py
git diff --check
```

Results: 48 `retrievalkit-cli` tests passed, three independent-validator unit
tests passed, Rust lint/format passed, both Phase 1.1 validators passed, all 21
Rust/Python A-C comparisons passed, persistence equivalence passed for all
three runs, and the two fresh qualification emissions were byte-identical.
Existing V1 and V2 quality gates are included in the 48-test CLI suite.

## Files and remaining work

Phase 1.2a changed only evaluation code and documentation:

- `crates/retrievalkit-cli/src/quality.rs` and `quality/v3*.rs`;
- `scripts/quality/validate_v3_phase_1_2a.py` and its unit tests;
- this report, the retrieval-quality README, working memory, and benchmark
  roadmap status.

Phase 1 remains active. The next task is D-G: execute independent graph
selection for explicit/topic/team lanes, record seed provenance and canonical
paths, project generation-bound candidate scopes, run graph-scoped E-G, compute
evidence/candidate/path/truncation metrics and paired comparisons, prove
combined persistence equivalence, independently cross-check graph artifacts,
and only then emit the complete A-G V3 manifest.

Remaining risks are deliberately bounded:

- the conformance collection is synthetic and three-dimensional, so it supports
  correctness qualification, not public quality or device-performance claims;
- cross-platform production SIMD may differ in insignificant f32 residual bits,
  which is why independent diagnostic-score comparison uses the frozen `2e-7`
  tolerance while ranking and metrics remain exact; and
- full graph execution, graph metrics, combined generation fingerprints, and
  the publication-ready artifact schema remain unqualified until D-G pass.
