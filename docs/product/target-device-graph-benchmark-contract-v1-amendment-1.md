# Target-Device Graph Benchmark Contract V1 Amendment 1

Status: active Phase 4b device-scope amendment

Date approved: 2026-07-18

This amendment changes only the required physical-device scope of
`target-device-graph-benchmark-contract-v1.md`. The repository owner removed
iPhone 14 Pro Max from the current Phase 4b gate. The sole required physical
device is now iPhone 17 Pro Max (`iPhone18,2`).

iPhone 14 Pro Max is an optional future qualification target. Its absence must
not block Phase 4b validation, completion, or reporting. Adding it later
requires a new explicit scope amendment and authorization; existing evidence
must not be relabeled or silently reused for that future gate.

This amendment does not change:

- the frozen 10K, 25K, 50K, or diagnostic 100K workload bytes or identities;
- F32/I8 configurations, query populations, correctness rules, sample counts,
  lifecycle protocol, thermal policy, or graph-free regression gate;
- the iPhone-17-only classification of the diagnostic 100K stress lane;
- signed application or framework binaries; or
- the authorization identity attached to evidence already collected under
  `phase4b-execution-authorization-v3.json`.

The v3 authorization remains an immutable provenance record. Its deferred
iPhone 14 entry and contemporaneous `overall_phase4b_pass_possible: false`
field describe the scope at authorization time and are superseded only for the
completion decision by this owner-approved amendment. Recorded artifacts must
continue to match the v3 authorization SHA-256; they must not be rewritten to
claim a new authorization hash.

The collector must expose only the iPhone 17 role for current measurement
actions. The independent Phase 4b validator must require exactly the iPhone 17
device directory and the complete supported, graph-free, and eligible stress
evidence defined by Contract V1. Historical authorization files and reports
may retain iPhone 14 metadata for auditability.
