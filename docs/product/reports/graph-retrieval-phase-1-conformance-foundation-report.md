# Graph Retrieval Phase 1 Conformance Foundation Report

Status: Phase 1.1 complete

Date: 2026-07-16

## Scope and outcome

Phase 1.1 adds an evaluation-only V3 conformance foundation. It does not run
the production graph/retrieval engines and does not emit selections, rankings,
metrics, or timings. Production Rust APIs and the Swift and Python wrappers are
unchanged.

The checked-in collection is under
`benchmarks/retrieval-quality/v3/`. It has 7 records, 8 chunks, 9 included
queries, 15 qrel rows, 4 evidence rows, 4 expected-path rows, and 3 exclusion
rows. Its synthetic coverage includes:

- explicit, topic-derived, and team-derived seed lanes;
- topic and team resolver success, topic no-match, and topic ambiguity;
- alternative complete-evidence sets and explicit/derived expected paths;
- query metadata filters, including record metadata overridden by one chunk;
- one two-chunk record for document projection and grade-zero qrel rows;
- one global exclusion and two topic-lane exclusions; and
- deterministic three-dimensional corpus and retrieval-query F32 vectors.

The six transformation manifests form the closed contract DAG. Their
collection inputs, upstream source inventory, outputs, file ownership, byte
counts, SHA-256 digests, population hashes, split lock, and collection file
index are checked from final canonical bytes.

## Exact fixture and population results

The normative A-J JSONL fixture is exactly 2,135 bytes and has SHA-256:

`4d7b920b8ae591f0c05cd41abbc36c50210bbf23e6bfa0e09b4eebbffdea4f46`

The checked-in `collection.json` is 2,823 bytes and has SHA-256:

`0452e0d1a3bd5d8aed8343fe6aedbcca7c70fab43c8c5edcbc051a930eb89a65`

All published contract populations were independently reproduced:

| Population | Ordered IDs | SHA-256 |
| --- | --- | --- |
| Q | qa qb qc qd qe qf qg qh qi | `91be2f127eff88b3d41229df2904cb3b7203992673711e3ee960ade05c35496d` |
| R | qa qb qd qf qg qh qi | `c373605c9580a90c0194ed28f5e07debfef5f8315547e9af5eb2cae963bfd4e3` |
| X_exp = S_exp | qb qc qh | `533bec415901af0a120dca2b883e9768aa2aae258c6476513959cd840e501bb5` |
| X_topic | qd qe qf qg qh | `a3b85dfbb4d7e5178e8cf34ab7c8d1474fbc03ceba933c731fbb83da012ad2f8` |
| F_topic | qf qg | `f1a82a3707574638a0dff6e16db2616c73c0692bcee0e55a21b565097d3267fb` |
| S_topic | qd qe qh | `be40e5a59829766e4ec9bc36e50f69f2c3f0b8c4f0e59fff0f253878622bac59` |
| X_team = S_team | qi | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` |
| F_team | empty | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| X_exp intersect R | qb qh | `2ce86656e11a1ddbe0d1710b2413ab7e6c2325271adc2ca5728eedb9b9534a1f` |
| X_topic intersect R | qd qf qg qh | `d9bd478b70d090c4b9543d346a42f300977480baf6f7d65f1c30e3608153a082` |
| S_topic intersect R | qd qh | `b64c45f1a2bef306eb3daca23aaa916bcbc151fef367325a7160e9520651f24e` |
| X_team intersect R = S_team intersect R | qi | `1737e84bdc92ff4adefee6614c6f22d67bd11d97170f28753ea05776050f3c0d` |

## Run identities and generation fingerprints

The foundation derives exactly 15 canonical configurations: A-C, D for
explicit/topic/team, and E-G independently for those same three lanes.

| Stable run ID | Logical-run SHA-256 |
| --- | --- |
| `v3-a-whole-semantic-f32-na-cfg-984e4c3bf991` | `bf237c1a474816a1f8c8dcb0580694c19ccd53cb5420c99b0419c3dd8bba2711` |
| `v3-b-whole-semantic-i8-na-cfg-e9898ca6ef53` | `e0b946e2b8c926badacc6f6fa104d52c33f72f6e8408820f969b59f5d6a6261b` |
| `v3-c-whole-weighted-i8-na-cfg-81e0395aa8e0` | `df48c1d3a962997bf21f037c6eae1905ed423576933da54dde749b9170af0b21` |
| `v3-d-selection-none-none-explicit-cfg-13feb2a18ac3` | `1bedbc6a99c164ed8ab69287192bf7287577eeb278406b9475cf3232bb2b0bde` |
| `v3-d-selection-none-none-team-cfg-7278e2315c8f` | `2c7850eb3ca1c9258765ff9b7dd338d00387e3132b6a4e5380bbac072d38c1aa` |
| `v3-d-selection-none-none-topic-cfg-bf6bed5c72e7` | `03e34447316a451bb023fb82635d0c91dee8f343e37eab909697528e2095302a` |
| `v3-e-graph-semantic-f32-explicit-cfg-d2855327ee28` | `fd70339f21946498b010c4d26e719158212a9de0a2e745fcbc4d75b3c0ccdb25` |
| `v3-e-graph-semantic-f32-team-cfg-9d005ed09abd` | `ffdf1b57a1cab91c5e3ecb0f7841a3ca69f8db8f58531c1c4f943ec85a3a7a02` |
| `v3-e-graph-semantic-f32-topic-cfg-dd783bc155d4` | `665dc02290fb825c82a55c728febd3bb8c1e98e9c7cc1fd475481aa0b9cccdd8` |
| `v3-f-graph-semantic-i8-explicit-cfg-9199f34e596a` | `1825b9e865bdd436095e5d98984a1ef9faf83dbe02ffa3268e04d463a5fd4de2` |
| `v3-f-graph-semantic-i8-team-cfg-c9fe28bfe8a2` | `9e3b11888396550e38aafcec9baffdd970c588a838c561cecb3655e66b4b3f77` |
| `v3-f-graph-semantic-i8-topic-cfg-748772f67f91` | `da4bbb529aaf3ba23fa09177f62a7f760f018438d499dae00641fa2720622cd8` |
| `v3-g-graph-weighted-i8-explicit-cfg-f5f6dfcae573` | `91a780087bce21816e0a71017146d19fdc87e1b0d38b3fea2a02e36254bec0aa` |
| `v3-g-graph-weighted-i8-team-cfg-0562c721d6e7` | `0f0022104a1921d80f09e302e653a1877ef502d363f70a9dc46dc7c0c0bbcf7a` |
| `v3-g-graph-weighted-i8-topic-cfg-36c6887ab88d` | `1a6c8c0e321bd3b92194ede4257f041eaddcdf2e9e4388bbebb3ad9b006218c2` |

The four unique D/E/F/G generation fingerprints are:

- graph-only D: `af1434a2db31b7ac356d665feb7554dbb6bc9202dcda1c030a247028905b6ccf`
- F32 semantic E: `485f564956610b65f16b7163b69085dad7c1a495aaf99aa44ac98d8aac9a4cef`
- I8 semantic F: `9142876c6ff687ae58d8c86ea25b553a9cde7744f2f91fa1bb2c34cf50a8eb1b`
- I8 weighted G: `7b5d71ac2e583b82bef661aa30ed57ea85e3e10b2fbc468fbbdb6689ef35cdb0`

### 2026-07-17 fingerprint-label audit

The earlier derived report reversed the human-readable E and G labels. The
section 4.4 contract formula, frozen collection bytes, Rust generator, and
independent Python implementation were already correct; this correction does
not change the contract or any fingerprint preimage. The audited mapping is E
F32 semantic `485f...`, F I8 semantic `9142...`, and G I8 weighted hybrid
`7b5d...`. Focused Rust and Python regressions now reconstruct each exact
retrieval-state preimage, check encoding and BM25/quantization policy presence,
hash the canonical inner and outer preimages, and prove that all three lane run
IDs for each letter bind to the expected fingerprint.

## Positive and negative validation

The focused Rust suite has 16 passing tests. Positive tests cover canonical
serialization, zero-row JSONL, the exact A-J bytes/hash, every published
population hash, the checked-in collection, all stable run/logical identities,
and byte-identical reruns. Negative tests make fresh fixture copies and prove
rejection of malformed:

- layout, file indexes, byte counts, digests, LF rules, and noncanonical JSON;
- unknown fields, invalid tagged records, record ordering, and graph schemas;
- tasks, filters, seeds, traversal relationships, qrels, evidence, paths, and
  global-versus-lane exclusions;
- embedding dimensions, collection counts, transformation outputs, and
  judgment leakage into graph construction;
- alias resolution, declared population hashes, source inventories, and split
  locks; and
- rerun bytes, with the first differing lexical file and byte offset reported.

The complete `retrievalkit-cli` suite has 39 passing tests, including the existing
V1 and V2 checked-in quality gates and the separate-collection TREC artifact
test. This establishes V1/V2 compatibility for the evaluation-only addition.

## Rust/Python agreement and deterministic rerun

`scripts/quality/validate_v3_conformance.py` is an independent implementation.
It does not call Rust for expected values. It reconstructs all 16 collection
files from its frozen synthetic source model, compares every byte, re-derives
the Q/R/X/F/S populations, reconstructs all 15 run-configuration preimages and
IDs, calculates logical-run hashes and generation fingerprints, and verifies
the foundation manifest's complete file index.

Rust and Python agree exactly on the collection bytes, population hashes, run
count, stable run IDs, logical-run hashes, generation fingerprints, artifact
byte counts, and artifact SHA-256 values.

The deterministic rerun command emitted the foundation into two fresh temporary
directories and compared every file in lexical path order. Result: byte
identical, with no mismatch. A separate negative comparator test changed byte
2 of `b.json` and reported that exact first file and offset.

## Commands executed

```text
cargo fmt --all
cargo fmt --all -- --check
cargo test -p retrievalkit-cli quality::v3 -- --nocapture
cargo test -p retrievalkit-cli
cargo test -p retrievalkit-cli quality::tests::checked_in_fixture_passes_quality_gates
cargo test -p retrievalkit-cli quality::tests::harder_v2_fixture_passes_quality_gates
cargo test -p retrievalkit-cli quality::tests::separate_collection_and_qrels_emit_deterministic_artifacts
cargo clippy -p retrievalkit-cli --all-targets --all-features -- -D warnings
cargo run -p retrievalkit-cli -- bench quality-v3 --collection benchmarks/retrieval-quality/v3 --foundation-artifacts target/v3-phase-1-1-final --verify-rerun
python3 scripts/quality/validate_v3_conformance.py --collection benchmarks/retrieval-quality/v3 --foundation-artifacts target/v3-phase-1-1-final
python3 -m py_compile scripts/quality/validate_v3_conformance.py
ruff check scripts/quality/validate_v3_conformance.py
git diff --check
```

## Remaining Phase 1 work and integration handoff

Phase 1.1 intentionally stops before full execution. The next integration task
should consume this validated collection through the existing capability-
separated Rust APIs and implement the contract's complete A-G execution and
artifact schemas. Specifically, it must:

1. resolve and record derived aliases with exact offset/provenance rows;
2. execute D-G graph selections independently per run and lane;
3. project generation-bound candidate scopes, intersect unchanged metadata
   filters, and execute A-C/E-G rankings with exhaustive chunk-to-document
   projection;
4. emit the normative selection, path, TREC, Rust diagnostic, metrics, timing,
   and manifest files without changing this collection version;
5. run before-save/after-reload equivalence and invalidate affected runs under
   the V3 attribution rules; and
6. add an independent Python check for actual candidate scopes, paths, rankings,
   and metrics.

That task must retain the 15 configuration IDs and population hashes reported
here unless a deliberate implementation-revision or collection-version change
requires new configuration IDs. It must not alter production APIs or wrappers,
begin a public dataset adapter, or emit placeholder quality/performance data.

No unresolved Phase 1.1 correctness risk remains. The remaining risk is the
unimplemented Phase 1 execution/integration surface listed above; this report
does not claim that A-G retrieval results, graph metrics, persistence reload
equivalence, or timing artifacts exist yet.
