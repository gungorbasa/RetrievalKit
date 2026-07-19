# Target-Device Graph Benchmark Contract V1 Amendment 2

Status: active Phase 4b foreground and authorization-lineage amendment

Date approved: 2026-07-19

This amendment authorizes one evaluation-harness correction after three
preserved `read_only_validation` attempts completed before UIKit reported the
application as active. It also defines how already accepted v3 evidence and
new v4 evidence form one final Phase 4b qualification without changing the
measured retrieval implementation.

## Foreground gate

Automated base and graph benchmark launches must wait until
`UIApplication.applicationState == .active` before entering their benchmark
runner. The wait is bounded at 30 seconds and fails closed if activation does
not occur. It executes before any FFI benchmark call, lifecycle operation, RSS
sampler, or measured timer starts. It is not part of a latency result. Manual
launches do not use the gate.

This is an evaluation-harness reliability correction. It does not change Rust
retrieval, graph, persistence, FFI, or public Swift behavior. Any executable
containing the correction requires a new authorization and new executable
fingerprints.

## Immutable v3 evidence

Authorization v3 remains immutable at SHA-256
`9bc321b7b4ca6970870243a8df0709b9914911b278234bbff229ec1e9fba1240`.
The following accepted v3 paths are retained byte-for-byte:

- all 30 supported query session artifacts under
  `devices/iphone17-pro-max/supported/*/*/query/session-*.json`;
- `10k-384d-v3/f32` lifecycle `prepare.json`;
- all three warmups and 20 samples for the `build` operation; and
- all three warmups and 20 samples for the `save` operation.

This closed set contains 77 files. Its canonical sorted path/SHA-256 preimage
has SHA-256
`a7d021e0b45fbd2a722482af44428335eac0d8ab188032676c4643e051e7a9dc`.
The canonical preimage is compact, sorted-key UTF-8 JSON over objects with
exactly `path` and `sha256` fields. No rejected or diagnostic artifact is part
of this accepted set.

The owner-approved iOS variance remains explicit for v3 evidence: accepted v3
artifacts may report build `23F81` or `23F84`. This allowance changes neither
their authorization hash nor their recorded environment.

## v4 evidence boundary

Authorization v4 must name the v3 authorization hash, the four closed path
patterns, the 77-file count, the preserved artifact-set hash, and the two
allowed prior OS builds. It covers every required final path not matched by
the closed v3 patterns, including:

- `read_only_validation`, `cold_load`, `warm_load`, and `replay` for
  `10k-384d-v3/f32`;
- the complete lifecycle lanes for every other supported workload/encoding;
- all graph-free sessions; and
- the preflight-authorized 100K diagnostic stress lanes.

The v4 collector must skip a valid existing artifact only when it carries the
current v4 authorization or when it is a byte-verified v3 artifact at one of
the closed paths. An old authorization at any other destination fails closed.
The final validator must independently verify both authorizations, both app
executable identities, both linkage boundaries, the preserved artifact-set
hash, per-path authorization selection, and fresh-process uniqueness across
the combined evidence set.

Installing the v4 apps must be an in-place update. Do not uninstall either app
or erase its Application Support container, because the remaining read-only
and load operations depend on the persisted directory created by the accepted
v3 `prepare` operation.

## Unchanged contract

This amendment does not change workload bytes, embeddings, graph data,
configuration, sample counts, percentile rules, correctness rules, thermal
rules, graph-free thresholds, or the fewer-than-50K V1 product boundary. The
100K workload remains diagnostic, non-marketing stress evidence. The three
foreground-false attempts remain rejected evidence and cannot be promoted or
relabelled.
