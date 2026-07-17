# V3 Graph-Retrieval Phase 1 Publication Report

Status: PASS; evaluation-artifact Phase 1 complete on 2026-07-17. Phase 2 is
not started.

## Scope

This gate publishes the frozen synthetic A-G evaluation result in the exact V3
deterministic-quality layout. It does not evaluate a public collection, measure
performance or a physical device, or authorize a quality or marketing claim.
No production Rust API, FFI, Swift wrapper, or Python wrapper changed; all work
is confined to evaluation tooling, CLI quality code, tests, and documentation.

## Official `trec_eval` identity

The evaluation-only bootstrap uses the official NIST repository at
`https://github.com/usnistgov/trec_eval`, pinned to commit
`f4253652c8efd0d86ddffd0d163cc0a0f813111a` (`10.0-rc3`). The pinned codeload
archive SHA-256 is
`3cc2618656038df53b6783aef44de24d72854a4877064ce1d12b2205fcd63165`.
The verified source is kept under ignored `target/benchmarks/tools/`.

Official output is normally rounded to four decimal places. The bootstrap
therefore applies a checksum-verified, output-only formatting patch changing
`%6.4f` to `%.17g` in `meas_print_final.c` and `meas_print_single.c`; no metric
algorithm changes. The patched source-tree SHA-256 is
`b68cb9ad8d407c6e1e4d1bce9d867a7525a841d4a1b98b19478a984dde445e28`.
The resulting executable SHA-256 is
`2e7be5f86c08d1a89f813af09504af505e310ded22b6a9cfad3d27556740bcdd`,
built with `/usr/bin/cc`, Apple clang 21.0.0
(`clang-2100.1.1.101`), target `arm64-apple-darwin25.5.0`.

All 12 ranked A-C/E-G TREC runs pass at tolerance `1e-9`. Exact mappings are:

- NDCG@5/10 to `ndcg_cut.5/10`, after the contract's exact `2^rel-1` qrel-gain
  projection;
- Recall@5/10 to `recall.5/10`;
- Precision@5 to `P.5`;
- MRR@10 to `recip_rank` after exact top-10 run truncation;
- AP/MAP to per-query and aggregate `map`; and
- Success@1 to `success.1`.

Judged@5/10 and the graph/evidence metrics have no exact official mapping and
are recorded as unsupported, not approximated. Maximum absolute difference is
`0.0` for both per-query and aggregate comparisons. There is no fallback to
`ir_measures` or `pytrec_eval`.

## Release determinism identity

The exact release executable SHA-256 is
`66746e7d4c4a7620b779cba6f515fa448ee8c8169c13014e7822c43951790636`.
It reports rustc `1.92.0`, target `aarch64-apple-darwin`, LLVM `21.1.3`, CPU
architecture `arm64`, OS build `25F84`, one execution thread, locale `C`,
round-to-nearest-ties-to-even floating-point mode, and no runtime flags. The
canonical determinism-environment SHA-256 is
`f449dcaa01175d9811f45917fb62cda4f74d73d6a0ed7cba7c2dd021329f04e3`.
The manifest records the complete sorted CPU-feature set and the exact clean
40-character implementation revision; `source_sha256` is null.

The context-independent qualified logical-run mapping is:

| Run | Lane | `logical_run_sha256` |
|---|---|---|
| A | none | `bf237c1a474816a1f8c8dcb0580694c19ccd53cb5420c99b0419c3dd8bba2711` |
| B | none | `e0b946e2b8c926badacc6f6fa104d52c33f72f6e8408820f969b59f5d6a6261b` |
| C | none | `df48c1d3a962997bf21f037c6eae1905ed423576933da54dde749b9170af0b21` |
| D | explicit | `1bedbc6a99c164ed8ab69287192bf7287577eeb278406b9475cf3232bb2b0bde` |
| D | team | `2c7850eb3ca1c9258765ff9b7dd338d00387e3132b6a4e5380bbac072d38c1aa` |
| D | topic | `03e34447316a451bb023fb82635d0c91dee8f343e37eab909697528e2095302a` |
| E | explicit | `fd70339f21946498b010c4d26e719158212a9de0a2e745fcbc4d75b3c0ccdb25` |
| E | team | `ffdf1b57a1cab91c5e3ecb0f7841a3ca69f8db8f58531c1c4f943ec85a3a7a02` |
| E | topic | `665dc02290fb825c82a55c728febd3bb8c1e98e9c7cc1fd475481aa0b9cccdd8` |
| F | explicit | `1825b9e865bdd436095e5d98984a1ef9faf83dbe02ffa3268e04d463a5fd4de2` |
| F | team | `9e3b11888396550e38aafcec9baffdd970c588a838c561cecb3655e66b4b3f77` |
| F | topic | `da4bbb529aaf3ba23fa09177f62a7f760f018438d499dae00641fa2720622cd8` |
| G | explicit | `91a780087bce21816e0a71017146d19fdc87e1b0d38b3fea2a02e36254bec0aa` |
| G | team | `0f0022104a1921d80f09e302e653a1877ef502d363f70a9dc46dc7c0c0bbcf7a` |
| G | topic | `1a6c8c0e321bd3b92194ede4257f041eaddcdf2e9e4388bbebb3ad9b006218c2` |

Release-context run IDs additionally bind the final documentation commit.
Embedding those IDs or the final artifact-set hash in that same commit would
create a self-reference. Following the contract's publication rule, the final
artifact is regenerated and revalidated after this report is committed. The
exact clean revision, 15 release-context run IDs, and artifact-set SHA-256 are
therefore recorded in the ignored
`target/benchmarks/v3/final-publication-summary.json`, in both final manifests,
and in the release handoff accompanying this report. The independent section
4.7 projection proves all 15 map to the logical hashes above.

## Publication result

The final public root contains exactly 44 regular files: 43 deterministic files
plus `manifest.json`, whose `files` map has exactly 43 entries. It includes the
three judgment/exclusion inputs, 12 rankings, 12 selections, 12 path files,
closed `rust-results.json` and `metrics.json`, and the single deterministic
`not_measured` timing row. It contains no qualification marker, validation
report, persistence report, hash index, intermediate result, temporary file,
or invalid execution.

Two fresh emissions from the same committed executable and determinism context
are recursively byte-identical across all 44 files. The independent validator
passes exact inventory, canonical serialization, closed schemas, source-copy
identity, run/population/logical hashes, generation fingerprints, status,
manifest digests and sizes, executable/environment/revision identity, official
cross-check identity, persistence equivalence, and section 4.7 portability.
Failure-injection tests also prove that no manifest or partial final directory
is published when a gate fails.

The separate qualification artifact remains exactly 56 hash-index preimage
files with unchanged artifact SHA-256
`ee264e919ab5872fd400354f5aa332993fd55fdedcaab400e6f5ba41619f631c`.

## Next task

Phase 1 is complete. The exact next roadmap task is Phase 2: document the
selection between HotpotQA and 2WikiMultiHopQA, then build the first public
graph collection adapter with checksum, license, count, schema, under-50K
construction, qrel-independent graph construction, and frozen seed resolution.
