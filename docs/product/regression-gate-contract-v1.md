# RetrievalKit Regression Gate Contract V1

Status: frozen before Phase 7 threshold implementation

Frozen: 2026-07-21

Machine-readable contract: `benchmarks/regression/contract-v1.json`

## Purpose and boundary

This contract converts the accepted Phase 1–6 correctness, quality,
performance, persistence, graph-isolation, and publication guarantees into
fail-closed merge and release gates. It does not reopen or reinterpret any
Phase 4–6 artifact, authorize external publication, expand the fewer-than-50K
product boundary, qualify USearch timing, or authorize physical-device work.

The three tiers are deliberately separate. Pull-request gates use only
checked-in synthetic inputs and deterministic calculations. Scheduled gates
require explicitly provisioned, hash-pinned licensed inputs. Release gates
require controlled-platform evidence and a separate owner authorization.
Missing scheduled or release inputs produce `not_provisioned`, never `passed`.

## Gate identity and ownership

Every gate has one immutable ID in `gate-registry-v1.json`, one owner, one
tier, one metric definition, one threshold, one baseline, platform and input
requirements, failure severity, evidence paths, rebaseline rules, and claim
impact. IDs are never reused. A semantic change creates a new registry and
contract version.

Retrieval-core owners own result, ordering, exclusion, filtering, and replay
gates. Graph-core owners own selection, projection, scoped ranking, and empty
scope behavior. Benchmark owners own metrics, artifacts, and deterministic
serialization. Release owners own controlled hardware evidence and explicit
device authorization.

## Tier semantics

`pull_request` gates are blocking and permit only `passed` or `failed`. A
hidden skip, absent test, unknown gate, extra artifact, schema error, or
infrastructure error is a failure. They use the checked-in
`graph-quality-smoke-v1` fixture, execute without a dataset download, network
service, secret, private corpus, unstable timing threshold, or device.

`scheduled_full` gates are blocking on a controlled runner once provisioned.
The observation must bind every external input, dependency lock, toolchain,
source revision, population, and baseline hash. An absent or mismatched input
is `not_provisioned` or `failed`; neither is a pass. Full quality, external
reference correctness, two-root determinism, and Phase 6 validation run here.

`release` gates are blocking on the named controlled platform. They validate
complete 10K/25K/50K F32/I8 evidence, correctness, P50/P95/P99, graph-free
median-session P95 ratios, peak memory, component and total persisted size,
and build/save/validation/load/replay behavior. The workflow only validates
pre-collected evidence. It contains no install, launch, collection, or other
device command. The 100K physical-device lane has no option and is permanently
forbidden.

## Metrics and thresholds

Identity and correctness metrics use exact equality. Quality metrics use the
same calculations as the frozen Phase 6 package: NDCG@10 with binary gains,
Recall@10, Complete Evidence Recall@10, macro candidate recall, macro candidate
complete evidence, and an integer unexpected-empty-scope count. Smoke metrics
must equal 1.0 with zero unexpected empty scopes because the tiny synthetic
fixture has exhaustive deterministic judgments.

Full quality floors equal the frozen accepted Phase 6 scoped values. They are
not rounded display values. This detects any loss relative to the accepted
296-query baseline; changing the population is a baseline mismatch rather than
an implicit rebaseline. Exact external-reference recall remains 1.0. USearch
performance is never eligible because its frozen Recall@10 gate failed.

Hardware-sensitive absolute latency never blocks generic shared CI. On the
controlled iPhone 17 Pro Max configuration, query and lifecycle P50/P95/P99
may be at most 1.10 times their bound baseline values. Ten percent is the
pre-registered run-to-run allowance for the initial gate, not permission to
widen a failed gate. The graph-free candidate/baseline median-session P95 ratio
retains the stricter frozen maximum of 1.03. Peak process memory must remain at
or below 1,610,612,736 bytes. Each persisted component and total may be at most
1.05 times its frozen Phase 4b value; the smaller allowance reflects byte
determinism and isolates format growth from timing noise.

## Result and failure semantics

Results conform to `result-schema-v1.json` and contain the contract, registry,
baseline, fixture, source revision, tier, overall status, and exactly the
expected gate IDs. Gate statuses are `passed`, `failed`, or `not_provisioned`.
`not_provisioned` is permitted only for scheduled or release gates and makes
the tier overall status `not_provisioned`, never `passed`.

Every failure records the metric, expected threshold, actual value, baseline
identity, affected guarantee or claim, evidence path, reproduction command,
and blocking tier. `failure-summary.md` is a deterministic human view of the
machine-readable failures. Infrastructure errors are serialized as failures
when possible and always return a nonzero process status.

## Serialization, hashing, and inventory

All JSON is UTF-8, sorted-key, two-space-indented JSON with no non-finite
numbers and one trailing LF. Markdown is UTF-8 with LF endings and one trailing
LF. Symlinks and special files are forbidden. The Phase 7 manifest lists the
closed static inventory and SHA-256 of every static payload. Its canonical
artifact-set SHA-256 is calculated over sorted UTF-8 lines
`<relative-path>\t<sha256>\n`, excluding the manifest itself.

Two result roots produced from the same source revision, tier, inputs, and
observations must be recursively byte-identical. Results contain no wall-clock
timestamp, absolute path, random identifier, or unstable host data.

## Baselines and rebaseline policy

Baselines are immutable, versioned records. A baseline or threshold update
requires a reviewed change containing the old and new values, rationale,
controlled evidence, source revision, platform/toolchain identity, sample
count, changed claim impact, regenerated static manifest, and new canonical
hashes. The validator rejects an edited registry whose baseline identity or
authorization record is not updated consistently. A failed run must never
rewrite or widen its own threshold.

Rerun or expiration occurs when retrieval, ranking, filtering, deletion,
persistence, graph selection, dispatch, serialization, benchmark code,
dependency lock, model, dataset, population, compiler, hardware, OS, timing
boundary, or licensing state changes. Phase 6 claims still expire on
2027-07-21. Release device evidence is additionally invalid after any named
device, OS build, release toolchain, executable, or authorization change.

## Safety and claim constraints

Ordinary PR gates use no physical device and no external licensed input.
Scheduled and release workflows use least privilege, pinned actions, bounded
timeouts, concurrency controls, and no pull-request secrets. Raw inputs remain
outside Git. Rejected evidence is preserved but never counted as accepted.

The release workflow accepts only 10K, 25K, and 50K, F32 and I8 evidence. It
must reject a 100K execution, support, capacity, performance, or marketing
statement. It must also reject any USearch performance-winner statement,
non-equivalent graph winner claim, hidden skip, missing platform/version
qualifier, or accepted stress artifact count other than zero.

## Independent validation

`benchmarks/regression/validate_gates.py` independently validates the closed
inventory, canonical serialization, static hashes, contract/registry/baseline
references, fixture judgments, result membership and status semantics,
threshold calculations, failure summaries, prohibited claims, platform
qualifiers, and two-root byte identity. It does not import the gate runner.

