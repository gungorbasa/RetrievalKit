# Graph Retrieval Phase 1 Graph Selection Report

Status: Phase 1.2b Run D complete; overall Phase 1 remains active

Date: 2026-07-16

## Outcome and boundary

The three frozen graph-only D lanes passed through production
`GraphDatabase`. Rust builds the canonical corpus and schema-driven graph,
resolves explicit or derived seeds, executes bounded traversal, projects graph
matches into a generation-bound `CandidateScope`, applies the production
metadata filter, and materializes stable chunk identities. D builds no vector,
BM25, or other retrieval state.

This completes only Phase 1.2b Run D. E-G have not begun, the output remains
partial and non-publication-ready, and no final A-G `manifest.json` or public
graph-retrieval quality claim is authorized.

The required starting commit was
`ddb018a0662cf1d834454deb4a8842bff5977111` on
`codex/graph-m1-core-foundation`.

| Commit | Message |
| --- | --- |
| `cec099a719257644886606c98b4ab0ee001e13bf` | `Expose corpus-owned candidate scope operations` |
| `85735f732f937b689bc815fef8bbdcd8223c342d` | `Expose graph candidate projection through FFI` |
| `8a5d40b032ebdbcae8cbfa8d9c64380b027d15cc` | `Add native Swift graph candidate projection` |
| `40333eba43b2a62902c1b15b462f042d0b73cec9` | `Document graph candidate projection parity` |
| `036cc62421f8e3ae372f6338427b9918674b0bcf` | `Execute V3 graph selection runs` |
| `cdfcf89db0081cfb3b238db044a03cebcf233800` | `Validate V3 graph selection persistence` |
| `71e4ea9eda439a7bca85b3a5e2d94310c560ba5f` | `Cross-check V3 graph selections` |
| report commit | `Document V3 graph selection qualification` |

## Production API decision

`CandidateScope` remains opaque. Internal IDs, membership representation,
iteration, and containment did not become public. Its owning corpus validates
the corpus/generation binding before filtering or identity materialization:

```rust
pub fn filter_candidate_scope(
    &self,
    scope: &CandidateScope,
    filter: Option<&Filter>,
) -> Result<CandidateScope>

pub fn candidate_scope_identities(
    &self,
    scope: &CandidateScope,
) -> Result<Vec<ChunkIdentity>>
```

`ExactVectorIndex` delegates the same operations. `GraphDatabase` and
`GraphRetrievalDatabase` expose:

```rust
pub fn project_candidate_identities(
    &self,
    result: &GraphResult,
    filter: Option<&Filter>,
) -> Result<GraphCandidateProjection>
```

The projection contains lexically ordered `(record_id, chunk_key)` identities
and source/before-filter/after-filter counts. The C ABI v7 exposes typed
`RetrievalKitGraphChunkIdentity` and `RetrievalKitGraphCandidateProjection` values through the two
database projection functions, with explicit free/clear functions. Swift
exposes native `GraphChunkIdentity`, `GraphCandidateProjection`, and async
`projectCandidates(from:filter:)` on both database actors. Python projection
remains deferred until a Python graph wrapper exists.

Filtering is O(scope size); stable hydration is O(scope size) plus lexical
sorting. The graph-free hot path is unchanged, and graph-only callers do not
construct retrieval state or cross the wrapper boundary once per candidate.

## Frozen graph, runs, and seeds

The production build contains 7 records, 8 active chunks, 15 nodes, 26 edges,
and 0 diagnostics. Its graph-only generation fingerprint is
`af1434a2db31b7ac356d665feb7554dbb6bc9202dcda1c030a247028905b6ccf`.

| Lane | Run ID | Logical SHA-256 | Declared | Executed | Declared SHA-256 | Executed SHA-256 |
| --- | --- | --- | ---: | ---: | --- | --- |
| explicit | `v3-d-selection-none-none-explicit-cfg-13feb2a18ac3` | `1bedbc6a99c164ed8ab69287192bf7287577eeb278406b9475cf3232bb2b0bde` | 3 | 3 | `533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5` | same |
| topic | `v3-d-selection-none-none-topic-cfg-bf6bed5c72e7` | `03e34447316a451bb023fb82635d0c91dee8f343e37eab909697528e2095302a` | 5 | 3 | `a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8` | `be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59` |
| team | `v3-d-selection-none-none-team-cfg-7278e2315c8f` | `2c7850eb3ca1c9258765ff9b7dd338d00387e3132b6a4e5380bbac072d38c1aa` | 1 | 1 | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` | same |

Explicit seeds resolved for `qb`, `qc`, and `qh`. Topic resolution selected
`qd -> alpha`, `qe -> beta`, and `qh -> gamma`; `qf` was excluded pre-freeze
for no match and `qg` for the ambiguous `shared-east`/`shared-west` alias. Team
resolution selected `qi -> mobile`. Diagnostics retain offsets, aliases, policy
hashes, candidates, failure reasons, and selected seeds.

## Selection counts

Projection happens before filtering. “Eligible” is the complete corpus after
the same production filter; “candidates” and “documents” are after intersecting
the projected scope.

| Lane/query | Matches | Before filter | Candidates | Documents | Paths | Eligible |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| explicit `qb` | 2 | 2 | 1 | 1 | 2 | 4 |
| explicit `qc` | 1 | 2 | 2 | 1 | 1 | 8 |
| explicit `qh` | 1 | 2 | 1 | 1 | 1 | 4 |
| topic `qd` | 3 | 4 | 3 | 3 | 3 | 4 |
| topic `qe` | 2 | 2 | 2 | 2 | 2 | 8 |
| topic `qh` | 3 | 4 | 3 | 3 | 3 | 4 |
| team `qi` | 2 | 2 | 1 | 1 | 2 | 4 |

All 7 executions were valid and produced 14 canonical paths. There was no
empty scope, duplicate path, truncation, stale selection, or invalid execution.

## D metrics

Retrieval-ranking metrics are `not_applicable`, not zero. Macro means use only
valid metric values; excluded and not-applicable status buckets remain intact.

| Lane | Macro candidate recall | Macro complete evidence | Macro reduction | Macro empty | Macro path accuracy | Macro truncated | Micro candidate recall | Micro reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| explicit | 0.5 | 0 | 4 | 0 | 0.5 | 0 | 0.5 (2/4) | 4 (16/4) |
| topic | 1 | 1 | 2.222222222222222 | 0 | 0.5 | 0 | 1 (6/6) | 2 (16/8) |
| team | n/a | n/a | 4 | 0 | n/a | 0 | n/a (0/0) | 4 (4/1) |

Path accuracy is 1 for explicit `qc` and topic `qe`, and 0 for both applicable
`qh` lanes. This is the contract result: the frozen expected `qh` path does not
match the production occurrence/direction identity. Candidate recall is 0.5
for explicit `qc` and `qh`, and 1 for topic `qd`, `qe`, and `qh`.

## Independent comparison, persistence, and stale selection

The Python oracle independently reconstructs D run identities and populations,
normalization and offsets, alias resolution, nodes/edges, directional
traversal, paths, projection, filtering, stable identities, fingerprint,
exclusions, all per-query metrics, and macro/micro aggregates. Rust files are
read only after calculation. All 7 query/runs and 14 paths agree exactly; the
maximum numeric and structural difference is 0.

For every lane, Rust executed before save, persisted through production graph
persistence, validated, loaded, and re-executed. Seeds, nodes, candidates,
filters, counts, traces, truncation, paths, metrics, and deterministic rows were
exactly equal. Tests reject results from another corpus or generation, reject
filtering and materialization of stale scopes, and reject projection through an
incompatible database. Qualification is staged and atomically renamed, so an
injected invalid D identity leaves no selection row, path row, or partial
destination artifact.

## Deterministic artifacts

Two fresh CLI emissions were byte-identical. The final qualification directory,
including both A-C and D independent reports, has SHA-256
`45fc088a713b63a26612230f7887f81f8200b52e5c8caad605220949d4bdf628`
over its canonical sorted `{path, sha256}` file index.

| Artifact | SHA-256 |
| --- | --- |
| `graph-rust-results.json` | `a68086d8a9a18c967c5cae34778f999a1ffd282e07a9cde708a82341f03b070f` |
| `graph-metrics.json` | `733596f2725078adced95ceeab99f6d5e5c91bd64c9cedf57bc4a2592536e77e` |
| `graph-projection-identities.jsonl` | `cae8a5cc2068be87e88f8331119b0f696d271ff1e321d38d051738d7dcf1849b` |
| `seed-resolution-diagnostics.json` | `fbff0d99b17005121e2e037b49802cc2b22067c1117245834fe1cef0d6a4ed75` |
| `graph-generation-fingerprint.json` | `6b070451b12d46a0a73122419548a2b37e5213625fb3396c6c995aa0f06450ec` |
| `graph-persistence-validation.json` | `7e66ecf93003dc62f1a59b23e87a73a5086be7fee855a387e71f624a971f5177` |
| `independent-cross-check.json` | `2177488b1d2298ab6d3d7fccf10bdbe7d723e2c9c41bfebc85e022a1eb0f0fae` |
| `graph-independent-cross-check.json` | `2379e5087ebf741cada90cb921742b7f5c5e323c1cf54848750360484652d938` |

Artifacts remain ignored under `target/benchmarks/v3/` and contain no final
manifest.

## Verification

The required Rust gates passed: `cargo fmt --all -- --check`; complete tests
for `retrievalkit-core` (129 unit plus 10 M1 integration), `retrievalkit-graph` (30
tests), default `retrievalkit-ffi` (15 tests), graph-enabled `retrievalkit-ffi` (19
tests), and `retrievalkit-cli` (62 tests); and warning-denying all-target Clippy
for all four crates.

The full Swift wrapper verifier passed shared (2), base/injest (20), and graph
(11) tests, all three quickstarts, linkage, and the Rust/Swift conformance
fixture. Phase 1.1 Rust byte-rerun and independent Python conformance passed all
15 frozen run identities. The A-D CLI qualification rerun was byte-identical;
the Phase 1.2a oracle checked 21 query/runs with exact metrics and maximum score
difference `2.9802322387695312e-08`, while the Phase 1.2b oracle checked 7
query/runs and 14 paths with exact agreement. Six Python unit tests,
`py_compile`, Ruff, and `git diff --check` passed. Frozen collection inputs and
their indexed hashes were unchanged; only the unindexed `README.md` was added.

## Performance and remaining gates

This tranche proves correctness and deterministic persistence, not latency or
device claims. The synthetic eight-chunk collection cannot support a public
performance claim.

Phase 1 remains active. E-G must next execute graph-scoped F32 semantic, I8
semantic, and I8 weighted-hybrid retrieval, qualify paired ranking/evidence
metrics and combined persistence, and pass the full artifact rerun. The
deferred A-C `ir_measures`/`trec_eval` publication cross-check must also pass
before a complete A-G manifest or claim. Phase 2 remains inactive.
