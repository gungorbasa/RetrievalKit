# Graph Retrieval Phase 1.2c Graph-Scoped Retrieval Qualification

Date: 2026-07-17

Status: PASS for Phase 1.2c qualification. Overall graph-aware evaluation
Phase 1 remains active because official `trec_eval` and final public artifact
assembly are open.

## Scope and commits

Work started from clean branch `codex/graph-m1-core-foundation` at
`040105613701eedf2d5096431b545d6395ceaabb` (`Build V3 graph retrieval
databases`). Frozen V3 collection bytes, version, identities, populations,
embeddings, configurations, and contract rules were not changed.

Commits:

1. `040105613701eedf2d5096431b545d6395ceaabb` — `Build V3 graph retrieval databases`
2. `39e3b811e42cd4814b6dd741b1c9c68c727afee7` — `Correct V3 E and G fingerprint labels`
3. `baa440b152a212f49cd96fb3fc0370cb70557abe` — `Execute V3 graph-scoped semantic runs`
4. `ac7ac62e3bfaeb2578ca163652fa6005efb154a6` — `Execute V3 graph-scoped hybrid runs`
5. `c5eb5aa0e0afe36f378db2b539b1b00f8ac28a50` — `Evaluate V3 paired graph retrieval quality`
6. `d31864c1d36efbe729a36404832827269e16e897` — `Validate V3 graph retrieval persistence`
7. `8832588903d21944ae3fc244a0d5545fd169c57a` — `Cross-check V3 graph retrieval independently`
8. `Document V3 graph retrieval qualification` — this report's commit; its
   object ID is recorded in the final handoff because a commit cannot embed its
   own content-derived Git object ID.

Each E-G run owns a production `GraphRetrievalDatabase`. Seed resolution,
traversal, generation-bound selection, projection, production metadata
filtering, and scoped production ranking all use that database. Evaluation
code records diagnostics and calculates metrics; it does not reimplement
production traversal, vector scoring, quantization, BM25, fusion, or filters.

## E/G label-correction audit and exact preimages

The dated 2026-07-17 audit searched all tracked occurrences of `7b5d71ac`,
`9142876c`, and `485f5649`. The V3 section 4.4 formula, Rust generator, and
independent Python implementation were correct. Only the derived foundation
report's human-readable E and G labels were reversed. The correction changed
those labels and added regressions that reconstruct and validate every exact
canonical preimage field before binding a fingerprint to a mode.

| Family | Encoding/mode | Generation fingerprint |
| --- | --- | --- |
| E | F32 semantic | `485f564956610b65f16b7163b69085dad7c1a495aaf99aa44ac98d8aac9a4cef` |
| F | I8 semantic | `9142876c6ff687ae58d8c86ea25b553a9cde7744f2f91fa1bb2c34cf50a8eb1b` |
| G | I8 weighted hybrid | `7b5d71ac2e583b82bef661aa30ed57ea85e3e10b2fbc468fbbdb6689ef35cdb0` |

Shared retrieval-state inputs:

- corpus embeddings: `35dc5b55c85c352aa38589858fd0e0a9800b000d47afeaf022b6c2acf2c2571e`
- embedding manifest: `6b7e920e286182e9b1398d3253c003baadd91de1aa373f7136f70299017b1179`
- normalization policy: `5393ff7a62243465ae81ce89131c432eb0d0fc982b1e5c786d94f9f48ec1e69e`
- quantization policy: `b7c0bb0252ea789e5810630e2e995aec0a75f635dc4880651db5402c0b2b4881`
- BM25 policy: `988983907ff40ef4638477b37f67de0f26df9f83b4be00314ee99dd6c2db24b1`

Exact E retrieval-state preimage:

```json
{"bm25_policy_sha256":null,"files":[{"path":"corpus-embeddings.f32.jsonl","sha256":"35dc5b55c85c352aa38589858fd0e0a9800b000d47afeaf022b6c2acf2c2571e"},{"path":"manifests/embedding.json","sha256":"6b7e920e286182e9b1398d3253c003baadd91de1aa373f7136f70299017b1179"}],"metric":"cosine","normalization":"unit_l2","normalization_policy_sha256":"5393ff7a62243465ae81ce89131c432eb0d0fc982b1e5c786d94f9f48ec1e69e","quantization_policy_sha256":null,"vector_encoding":"f32"}
```

SHA-256:
`53f26299753298305f7a80ab581eea04f09cdd971e37b35d4ec742ca85cd3d4d`.

Exact F retrieval-state preimage:

```json
{"bm25_policy_sha256":null,"files":[{"path":"corpus-embeddings.f32.jsonl","sha256":"35dc5b55c85c352aa38589858fd0e0a9800b000d47afeaf022b6c2acf2c2571e"},{"path":"manifests/embedding.json","sha256":"6b7e920e286182e9b1398d3253c003baadd91de1aa373f7136f70299017b1179"}],"metric":"cosine","normalization":"unit_l2","normalization_policy_sha256":"5393ff7a62243465ae81ce89131c432eb0d0fc982b1e5c786d94f9f48ec1e69e","quantization_policy_sha256":"b7c0bb0252ea789e5810630e2e995aec0a75f635dc4880651db5402c0b2b4881","vector_encoding":"i8"}
```

SHA-256:
`e891c85b4a555e0995dcc28effab0a32217258ff75e02ec7238c0bedc441a547`.

Exact G retrieval-state preimage:

```json
{"bm25_policy_sha256":"988983907ff40ef4638477b37f67de0f26df9f83b4be00314ee99dd6c2db24b1","files":[{"path":"corpus-embeddings.f32.jsonl","sha256":"35dc5b55c85c352aa38589858fd0e0a9800b000d47afeaf022b6c2acf2c2571e"},{"path":"manifests/embedding.json","sha256":"6b7e920e286182e9b1398d3253c003baadd91de1aa373f7136f70299017b1179"}],"metric":"cosine","normalization":"unit_l2","normalization_policy_sha256":"5393ff7a62243465ae81ce89131c432eb0d0fc982b1e5c786d94f9f48ec1e69e","quantization_policy_sha256":"b7c0bb0252ea789e5810630e2e995aec0a75f635dc4880651db5402c0b2b4881","vector_encoding":"i8"}
```

SHA-256:
`03cfae1edb14adcd2a4904a85ec7a9fa70c7dc6a33f5afe34b0a5cea870d0d1e`.

Each outer preimage contains corpus ID `vectorkit-v3-synthetic-corpus`, corpus
state `18053f800d41297c493f62bbbf913c2960048f4a254663cb5a1a4b25d2da4ad7`,
graph state `ce9ca6f2a1c82c3e69b481dacd52c240d146dd060fb5be9c0477ddbca1bef32e`,
the corresponding retrieval-state hash, and schema version `1`.

Exact canonical outer preimages, in E/F/G order:

```json
{"corpus_id":"vectorkit-v3-synthetic-corpus","corpus_state_sha256":"18053f800d41297c493f62bbbf913c2960048f4a254663cb5a1a4b25d2da4ad7","graph_state_sha256":"ce9ca6f2a1c82c3e69b481dacd52c240d146dd060fb5be9c0477ddbca1bef32e","retrieval_state_sha256":"53f26299753298305f7a80ab581eea04f09cdd971e37b35d4ec742ca85cd3d4d","schema_version":1}
{"corpus_id":"vectorkit-v3-synthetic-corpus","corpus_state_sha256":"18053f800d41297c493f62bbbf913c2960048f4a254663cb5a1a4b25d2da4ad7","graph_state_sha256":"ce9ca6f2a1c82c3e69b481dacd52c240d146dd060fb5be9c0477ddbca1bef32e","retrieval_state_sha256":"e891c85b4a555e0995dcc28effab0a32217258ff75e02ec7238c0bedc441a547","schema_version":1}
{"corpus_id":"vectorkit-v3-synthetic-corpus","corpus_state_sha256":"18053f800d41297c493f62bbbf913c2960048f4a254663cb5a1a4b25d2da4ad7","graph_state_sha256":"ce9ca6f2a1c82c3e69b481dacd52c240d146dd060fb5be9c0477ddbca1bef32e","retrieval_state_sha256":"03cfae1edb14adcd2a4904a85ec7a9fa70c7dc6a33f5afe34b0a5cea870d0d1e","schema_version":1}
```

## Nine runs and populations

| Run ID | Logical run SHA-256 |
| --- | --- |
| `v3-e-graph-semantic-f32-explicit-cfg-d2855327ee28` | `fd70339f21946498b010c4d26e719158212a9de0a2e745fcbc4d75b3c0ccdb25` |
| `v3-e-graph-semantic-f32-topic-cfg-dd783bc155d4` | `665dc02290fb825c82a55c728febd3bb8c1e98e9c7cc1fd475481aa0b9cccdd8` |
| `v3-e-graph-semantic-f32-team-cfg-9d005ed09abd` | `ffdf1b57a1cab91c5e3ecb0f7841a3ca69f8db8f58531c1c4f943ec85a3a7a02` |
| `v3-f-graph-semantic-i8-explicit-cfg-9199f34e596a` | `1825b9e865bdd436095e5d98984a1ef9faf83dbe02ffa3268e04d463a5fd4de2` |
| `v3-f-graph-semantic-i8-topic-cfg-748772f67f91` | `da4bbb529aaf3ba23fa09177f62a7f760f018438d499dae00641fa2720622cd8` |
| `v3-f-graph-semantic-i8-team-cfg-c9fe28bfe8a2` | `9e3b11888396550e38aafcec9baffdd970c588a838c561cecb3655e66b4b3f77` |
| `v3-g-graph-weighted-i8-explicit-cfg-f5f6dfcae573` | `91a780087bce21816e0a71017146d19fdc87e1b0d38b3fea2a02e36254bec0aa` |
| `v3-g-graph-weighted-i8-topic-cfg-36c6887ab88d` | `1a6c8c0e321bd3b92194ede4257f041eaddcdf2e9e4388bbebb3ad9b006218c2` |
| `v3-g-graph-weighted-i8-team-cfg-0562c721d6e7` | `0f0022104a1921d80f09e302e653a1877ef502d363f70a9dc46dc7c0c0bbcf7a` |

| Lane | Declared | Executed | Hashes | Exclusions per family |
| --- | --- | --- | --- | ---: |
| explicit | `qb`, `qh` | `qb`, `qh` | `2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f` | 0 |
| topic | `qd`, `qf`, `qg`, `qh` | `qd`, `qh` | declared `d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082`; executed `b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e` | 2 |
| team | `qi` | `qi` | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` | 0 |

Topic `qf` is frozen as `derived_seed_no_match`; `qg` is
`derived_seed_ambiguous`. Totals are nine runs, 15 valid executions, and six
exclusion instances.

## Equality, rankings, and persistence

All E-G selections and paths were executed independently and are logically
equal to matching D lane/query artifacts after ignoring only run ID and
generation fingerprint. Per family, explicit has two selections/three paths,
team has one/two, and topic has two/six. E-G totals are 15 selections and 33
paths, all equal.

Rankings were identical across E, F, and G on this fixture:

| Lane/query | Document/chunk order | Duplicate collapses per run |
| --- | --- | ---: |
| explicit `qb` | `phone/summary` | 0 |
| explicit `qh` | `alpha/details` | 0 |
| team `qi` | `mobile/summary` | 0 |
| topic `qd` | `alpha/details`, `beta/summary`, `gamma/summary` | 0 |
| topic `qh` | `gamma/summary`, `beta/summary`, `alpha/details` | 0 |

Every E/F/G run executed before save, used production combined save and
validation, loaded, recreated selections, and re-executed retrieval. Generation,
seeds, matches, candidates, paths, projection counts, rankings, diagnostics,
TREC, metrics, and comparisons were stable. Tests rejected stale same-corpus
and incompatible cross-corpus/database selections.

## Aggregate E-G metrics

E, F, and G have identical aggregates within each lane on this small synthetic
fixture. `N/A` is the preserved `not_applicable` status, not zero.

| Metric | Explicit | Topic | Team |
| --- | ---: | ---: | ---: |
| NDCG@5 / @10 | 0.5 | 0.9819702166583266 | 1.0 |
| Recall@5 / @10 | 0.5 | 1.0 | 1.0 |
| Success@1 | 0.5 | 1.0 | 1.0 |
| Precision@5 | 0.1 | 0.30000000000000004 | 0.2 |
| MRR@10 | 0.5 | 1.0 | 1.0 |
| AP/MAP | 0.5 | 0.9166666666666666 | 1.0 |
| Judged@5 / @10 | 1.0 | 0.6666666666666666 | 1.0 |
| Supporting-document recall@5 / @10 | 0.5 | 1.0 | N/A |
| Complete-evidence recall@5 / @10 | 0.0 | 1.0 | N/A |
| Candidate recall | 0.5 | 1.0 | N/A |
| Candidate complete-evidence | 0.0 | 1.0 | N/A |
| Candidate reduction | 4.0 | 1.3333333333333333 | 4.0 |
| Empty scope | 0.0 | 0.0 | 0.0 |
| Path accuracy | 0.0 | 0.0 | N/A |
| Truncation/all reason buckets | 0.0 | 0.0 | 0.0 |

## Nine paired comparisons

The comparison restricts finalized A/B/C per-query results to each frozen
scoped population. It does not rerun a baseline or use a runtime-success
intersection. The artifact retains absolute/relative deltas and
wins/ties/losses for all 14 metrics. Selected aggregates are:

| Pair/lane | NDCG@10 baseline -> scoped (delta; W/T/L) | Recall@10 | AP | Complete evidence@10 | Reduction | Relevant/evidence lost |
| --- | --- | --- | --- | --- | ---: | ---: |
| A-E explicit | 0.7153382790 -> 0.5 (-0.2153382790; 1/0/1) | 1 -> 0.5 | 0.625 -> 0.5 | 1 -> 0 | 4 | 1 / 1 |
| A-E topic | 0.9819702167 -> 0.9819702167 (0; 0/2/0) | 1 -> 1 | 0.9166666667 -> 0.9166666667 | 1 -> 1 | 1.3333333333 | 0 / 0 |
| A-E team | 1 -> 1 (0; 0/1/0) | 1 -> 1 | 1 -> 1 | N/A | 4 | 0 / 0 |
| B-F explicit | 0.7153382790 -> 0.5 (-0.2153382790; 1/0/1) | 1 -> 0.5 | 0.625 -> 0.5 | 1 -> 0 | 4 | 1 / 1 |
| B-F topic | 0.9819702167 -> 0.9819702167 (0; 0/2/0) | 1 -> 1 | 0.9166666667 -> 0.9166666667 | 1 -> 1 | 1.3333333333 | 0 / 0 |
| B-F team | 1 -> 1 (0; 0/1/0) | 1 -> 1 | 1 -> 1 | N/A | 4 | 0 / 0 |
| C-G explicit | 0.8154648768 -> 0.5 (-0.3154648768; 1/0/1) | 1 -> 0.5 | 0.75 -> 0.5 | 1 -> 0 | 4 | 1 / 1 |
| C-G topic | 0.8983537905 -> 0.9819702167 (+0.0836164262; 1/1/0) | 1 -> 1 | 1 -> 0.9166666667 | 1 -> 1 | 1.3333333333 | 0 / 0 |
| C-G team | 1 -> 1 (0; 0/1/0) | 1 -> 1 | 1 -> 1 | N/A | 4 | 0 / 0 |

These are synthetic conformance results, not marketing claims. Explicit `qh`
loses one relevant/evidence document; topic G improves NDCG@10 while reducing
AP. Positive and negative results remain visible.

## Independent, `ir_measures`, and deterministic checks

The independent Python oracle calculates expected seed resolution, traversal,
projection, filtering, F32/I8 scores, scoped BM25 with whole-index statistics,
weighted fusion, chunk/document rankings, TREC, retrieval/evidence/graph
metrics, pairs, and generation preimages from frozen inputs before reading
Rust artifacts.

- nine runs, 15 executions, and 33 paths checked
- maximum Rust/Python score difference: `2.9802322387695312e-08`
- maximum Rust/Python metric difference: `0.0`
- structural numeric difference: `0.0`
- tolerances: scores `2e-7`, metrics `1e-12`

The evaluation-only `ir_measures==0.4.3` dependency checked A-C plus all nine
E-G runs: 360 per-query values and every aggregate. Mappings are AP,
Judged@5/10, RR@10, NDCG@5/10 with gains `{0:0,1:1,2:3}`, P@5, Recall@5/10,
and Success@1. Per-query and aggregate maximum differences are both `0.0`
against `1e-9`. Evidence/graph metrics remain in the independent oracle because
they have no standard external mapping. Official `trec_eval` was unavailable
and remains a publication gate.

Two fresh complete emissions were recursively byte-identical after adding both
cross-check reports and the canonical index:

- `target/benchmarks/v3/phase-1.2c-final-a`
- `target/benchmarks/v3/phase-1.2c-final-b`

Each index covers 56 files. Artifact-set SHA-256:
`ee264e919ab5872fd400354f5aa332993fd55fdedcaab400e6f5ba41619f631c`.
The marker remains partial/non-publication-ready, no final manifest was emitted,
and ignored target artifacts were not committed.

## Verification and conclusion

Passed checks:

- `cargo fmt --all -- --check`
- core: 129 unit + 10 M1 integration tests
- graph: 30 tests across all suites
- FFI: 15 base and 19 graph-feature tests
- CLI: 67 tests
- warning-denying Clippy for core, graph, FFI base/graph, and CLI
- Swift base/graph linkage, suites, cross-wrapper fixture, and quickstarts
- Phase 1.1 foundation conformance/rerun; Phase 1.2a A-C; Phase 1.2b D;
  Phase 1.2c E-G, independent Python, `ir_measures`, and hash index
- nine Python unit tests, `py_compile`, Ruff, `git diff --check`, and frozen
  fixture `git diff --exit-code`

Phase 1.2c is complete. Overall evaluation Phase 1 stays active until official
`trec_eval` runs against the release artifact and the final public A-G
manifest/checksum package is assembled without changing frozen inputs. Phase
2, target-device claims, public quality claims, and graph marketing claims are
not claimed.
