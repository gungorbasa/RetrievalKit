# Target-Device Graph Benchmark Contract V1 Amendment 3

Status: active Phase 4b device-safety terminal-outcome amendment

Date approved: 2026-07-20

This amendment adds one fail-closed terminal outcome for an owner-directed
physical-device safety cancellation. It does not rewrite, relax, or invalidate
Contract V1 or Amendments 1 and 2. Their workload, evidence, authorization,
lineage, correctness, thermal, foreground, sample-count, and claim rules remain
immutable.

## 1. Scope

The repository owner permanently canceled further physical-device execution of
`100k-384d-v3-stress` after the iPhone became excessively hot. The cancellation
applies only to the diagnostic 100K physical-device lane on iPhone 17 Pro Max.
It changes neither the supported 10K/25K/50K matrix nor the graph-free matrix.

The 100K workload remains outside the fewer-than-50K V1 support boundary. It is
still classified only as `stress` and remains incapable of creating or
invalidating a support, performance, quality, latency, product, capacity, or
marketing claim.

## 2. Mutually exclusive terminal outcomes

The Phase 4b validator must accept exactly one of these outcomes for every
required 100K physical-device lane:

1. `completed`: the complete eligible F32/I8 stress evidence required by
   Contract V1 validates normally; or
2. `not_run_device_safety`: no accepted stress artifact exists and a separate,
   explicit device-safety cancellation authorization validates under section 3.

The second outcome is a terminal administrative safety result, not a benchmark
result. It does not mean passed, failed, safe, performant, supported, or tested.
It permits Phase 4b closeout under the active contract lineage without any
further physical-device execution.

Missing or incomplete stress evidence without a valid cancellation
authorization continues to fail closed. A validator must not infer a safety
cancellation from an empty directory, rejected files, prose, a thermal field,
or a previous execution authorization.

## 3. Cancellation authorization

`not_run_device_safety` requires a checked-in JSON authorization supplied
explicitly to the validator. The authorization is an immutable provenance
record distinct from every execution authorization. It must bind, by exact
value and SHA-256 where applicable:

- this amendment's repository-relative path and byte identity;
- the current execution-authorization SHA-256 without changing that file;
- the device role and exact 100K workload identity;
- the terminal outcome `not_run_device_safety` and classification `stress`;
- the owner-directed cancellation timestamp and device-safety reason category;
- zero accepted stress artifacts;
- the supported and graph-free accepted artifact counts and canonical
  path/SHA-256 set identities used for closeout;
- the rejected cancellation manifest's repository-root-relative location and
  SHA-256;
- the exact count and canonical identity of preserved partial artifacts; and
- explicit false values for support, performance, quality, latency, product,
  capacity-change, and marketing eligibility.

Unknown or missing authorization fields fail closed. The authorization path is
data, not a validator constant: the validator must safely resolve the declared
relative rejected-evidence directory beneath the supplied artifact root and
verify its contents. This makes the mechanism reusable for a future explicitly
authorized device-safety cancellation without creating an artifact-directory
bypass.

## 4. Accepted and rejected evidence boundary

For `not_run_device_safety`, the accepted stress tree must contain zero files,
symlinks, or other non-directory entries. Empty directories may remain. Any
accepted preflight, query, lifecycle, summary, marker, or partial file makes the
cancellation outcome invalid. Partial evidence cannot be combined with the
cancellation outcome and cannot count toward completion.

Preserved partial evidence must remain only below the authorization's declared
`rejected/.../canceled-by-user/...` directory. The validator must verify the
cancellation manifest, every declared original accepted path, every preserved
file SHA-256, the canonical preserved-set SHA-256, exact file count, absence of
undeclared files in that cancellation directory, and explicit prohibitions on
promotion or accepted counting. Moving, copying, relabeling, or counting a
rejected artifact as accepted invalidates closeout.

Rejected evidence is retained only to audit why collection stopped. It is not
eligible stress evidence and must never be used for a performance distribution,
correctness result, product gate, support statement, or marketing statement.

## 5. Unchanged supported and graph-free gates

The complete 10K/25K/50K F32/I8 supported-product query and lifecycle matrix
must still pass every existing split-lineage, executable, framework, device,
process-isolation, foreground, thermal, correctness, persistence, memory, and
inventory rule. The graph-free matrix must still pass identical-result,
zero-graph-activity, and maximum `1.03` median-P95 ratio rules. This amendment
adds no waiver, fallback, or threshold change to either gate.

## 6. Required validator result

The validator must report the closeout dimensions independently. For this
owner-directed cancellation they are:

```text
supported_product_qualification: passed
graph_free_qualification: passed
stress_outcome: not_run_device_safety
phase4b_closeout: passed
```

`phase4b_closeout: passed` means only that all requirements of Contract V1 as
modified by active Amendments 1, 2, and 3 reached a valid terminal state. It
must never be shortened or paraphrased into a claim that the 100K benchmark,
the original execution-only stress lane, or a broader supported capacity
passed.

## 7. Authorization and evidence immutability

Execution authorizations v3 and v4, their application/framework identities,
the 77-file preserved v3 set, and every accepted artifact remain byte-exact.
This amendment does not reauthorize an executable and authorizes no new device
launch. It authorizes validation of a distinct terminal safety outcome only.

The five partial F32 files and their cancellation manifest remain rejected
evidence. They must not be restored to the accepted tree. No physical-device
benchmark may be installed, launched, resumed, or repeated to implement this
amendment.
