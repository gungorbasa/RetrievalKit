# Graph Retrieval Phase 0 Independent Review

Date: 2026-07-15

## Scope and review method

This report records the independent dry-run specification review required by
section 12 of `docs/product/graph-retrieval-evaluation-contract-v3.md` and the
Phase 0 exit gate in
`docs/product/complete-retrieval-benchmark-and-marketing-roadmap.md`.

This was an independent AI-agent specification review, not an external human
audit and not human relevance judging. Two AI reviewers received identical
instructions with no inherited task context, worked independently, did not
communicate, did not see one another's conclusions before submitting, and made
no file changes. The coordinating agent waited for both completed reviews
before comparing their results.

The review was specification-only. It did not implement Phase 1, create
fixtures or calculators, download datasets, or modify production code, the V3
contract, the roadmap, or working memory.

## Reviewer A complete result

### Query-validity dry-run

Statuses are per metric and lane, not necessarily a single global property of
a query.

#### A. No positive qrel

- Status: `excluded_pre_freeze`.
- Exclusion reason: `no_relevant_documents`.
- Execution: no.
- Retained lanes: none; this is not a derived-lane-only exclusion and removes
  the query from every run.
- Macro participation: none.
- Validity: the collection and run remain valid if the exclusion occurs before
  freeze. Retaining the query as retrieval-valid violates sections 3.7 and
  5.1.
- Reporting: an exclusion row and count under `no_relevant_documents`; no
  ordinary retrieval metric may assign the query a convenient zero or one.

#### B. Declares `evidence` but has no evidence judgment

- Status: no valid per-query metric status is available because section 5.1
  declares the collection invalid rather than assigning `not_applicable`,
  `excluded_pre_freeze`, or `invalid_execution`.
- Exclusion reason: none may be invented. In particular, the condition cannot
  be silently converted at evaluation time to `missing_complete_evidence`.
- Execution: no canonical run may proceed on the invalid collection.
- Retained lanes: none validly execute.
- Macro participation: none; there is no valid aggregate.
- Validity: invalidates the collection before run execution.
- Reporting: a collection-validation error identifying the missing evidence
  row. If an adapter determines before freeze that upstream complete evidence
  is absent, it may instead omit the query with the separately defined
  `missing_complete_evidence` exclusion, but that is a different input state.

#### C. Valid retrieval and evidence judgments, no expected path

- Status: `valid` for retrieval and evidence metrics; `not_applicable` for Path
  Accuracy.
- Exclusion reason: none.
- Execution: yes.
- Retained lanes: whole-corpus lanes and every otherwise-eligible explicit or
  successfully resolved derived graph lane.
- Macro participation: retrieval and evidence metrics participate; Path
  Accuracy does not.
- Validity: does not invalidate the collection or run.
- Reporting: null Path Accuracy with `not_applicable` and its status count.
  Missing paths must not be scored as incorrect paths.

#### D. Seed resolution succeeds, projection is empty

- Status: a valid graph outcome for retrieval, evidence/candidate, and Empty
  Scope metrics.
- Exclusion reason: none.
- Execution: yes; no whole-corpus fallback is stated.
- Retained lanes: its graph seed lane and applicable whole-corpus lanes.
- Macro participation: retrieval, evidence, and candidate metrics use the
  actual empty result and contribute defined zeroes; Empty Scope contributes
  `1`; Candidate Reduction Ratio is null and omitted from the ratio macro,
  which is defined over non-empty scopes.
- Validity: does not invalidate the collection or run.
- Reporting: the empty-scope outcome and projection/filter counts.
- Specification problem: sections 5.1 and 5.4 provide no compatible per-metric
  status for the null Candidate Reduction Ratio. `valid` means included in the
  macro denominator, while `not_applicable` is defined as lacking a judgment.
  Neither describes a valid empty outcome whose ratio is null and excluded
  from the ratio macro. This prevents an exact `metrics.json` implementation.

#### E. Valid but truncated graph result

- Status: `valid`.
- Exclusion reason: none.
- Execution: yes.
- Retained lanes: the applicable graph lane and applicable whole-corpus lanes.
- Macro participation: all applicable retrieval, candidate, evidence, path,
  and empty-scope metrics use the actual partial result and stay in their
  denominators.
- Validity: does not invalidate the collection or run.
- Reporting: overall truncation rate, reason-specific rate, raw count, and the
  actual truncation reason. No retry or fallback is stated.

#### F. Stale or generation-mismatched selection

- Status: `invalid_execution` in the affected graph-scoped run.
- Exclusion reason: none; this is not a pre-freeze exclusion and must not add a
  runtime row to `exclusions.jsonl`.
- Execution: attempted because it was not excluded, but no successful scoped
  retrieval may be reported after the mismatch.
- Retained lanes: the query remains in the frozen populations; independent
  whole-corpus runs remain applicable. The affected graph run is invalid.
- Macro participation: it cannot be silently omitted or folded into a
  zero-valued mean; it is counted as invalid and fails the run gate.
- Validity: the collection is not invalidated by the runtime event; the
  affected run is. Section 4.7 additionally says stale-selection acceptance
  invalidates the complete run.
- Reporting: `invalid_execution` count and stale/generation-mismatch
  diagnostics.

#### G. Positive judgment conflicts with the metadata filter

- Status: `excluded_pre_freeze`.
- Exclusion reason: `filter_label_conflict`.
- Execution: no.
- Retained lanes: none; this is not a derived-only reason and removes the query
  from every run.
- Macro participation: none.
- Validity: valid if caught and excluded during the section 7.2 lock sequence;
  retaining it in the frozen executable collection violates the contract.
- Reporting: exclusion row and count under `filter_label_conflict`, frozen
  before tuning.

#### H. Derived resolver produces no alias match

- Status: `excluded_pre_freeze` in that derived lane.
- Exclusion reason: `derived_seed_no_match`.
- Execution: whole-corpus execution remains; derived D-G do not execute. An
  independent explicit lane, if present, is unaffected.
- Retained lanes: whole-corpus runs and any independently valid explicit lane;
  not the derived graph lane.
- Macro participation: whole-corpus and valid explicit-lane macros only; not
  conditional derived graph macros.
- Validity: does not invalidate the collection or run when frozen before
  tuning.
- Reporting: a failure in published derived-seed resolution coverage, whose
  denominator is all otherwise graph-eligible queries. Resolver diagnostics
  retain policy/version/hash, alias-table hash, normalization version,
  matches/offsets, candidate seeds, failure reason, and provenance. No
  fallback is permitted.

#### I. Multiple distinct longest-match seeds

- Status: `excluded_pre_freeze` in that derived lane.
- Exclusion reason: `derived_seed_ambiguous`.
- Execution: the same lane behavior as H.
- Retained lanes: whole-corpus runs and any independently valid explicit lane;
  not the derived graph lane.
- Macro participation: whole-corpus and applicable explicit macros only; not
  conditional derived graph macros.
- Validity: does not invalidate the collection or run when frozen before
  tuning.
- Reporting: derived-resolution coverage failure and full resolver
  diagnostics. Multiple longest matches for one canonical seed succeed, but
  multiple distinct seeds fail. Shorter aliases must not be tried.

#### J. Whole-corpus valid, derived-seed graph lane excluded

- Status: `valid` in whole-corpus runs and `excluded_pre_freeze` in the derived
  lane.
- Exclusion reason: `derived_seed_no_match` or `derived_seed_ambiguous`.
- Execution: whole-corpus yes; derived D-G no; independent explicit execution
  remains possible.
- Retained lanes: whole corpus and any valid explicit lane, not the conditional
  derived graph population.
- Macro participation: whole-corpus and applicable explicit metrics only.
- Validity: does not invalidate the collection or run.
- Reporting: lane-scoped exclusion plus a resolution-coverage failure. Paired
  whole/scoped metrics use the successful derived population without silently
  intersecting populations after execution.

### Metric dry-run

Raw inputs:

- Positive qrels: `L_q = {d1,d2}`, with grades `d1=2`, `d2=1`; `d3=0` is
  judged but nonrelevant.
- Ranking: `[d2,d3,d1]`.
- Evidence alternatives: `E1={d1,d2}`, `E2={d1,d4}`.
- Candidate documents: `{d1,d4}`.
- `N_q=10`, `C_q=2`.

Calculations:

- DCG@5: `1/log2(2) + 0/log2(3) + 3/log2(4) = 1 + 0 + 1.5 = 2.5`.
- IDCG@5: `3/log2(2) + 1/log2(3) = 3 + 0.6309297535714574 =
  3.6309297535714573`.
- NDCG@5: `2.5 / 3.6309297535714573 = 0.6885288809404666`.
  The displayed value is `0.688528880940467`; absolute difference is
  `4.440892098500626e-16`, within `1e-12`.
- Recall@5: `2/2 = 1`.
- Success@1: `1`, because `d2` is relevant.
- Precision@5: `2/5 = 0.4`.
- MRR@10: `1/1 = 1`.
- AP: `(P@1 + P@3)/2 = (1 + 2/3)/2 = 5/6 =
  0.8333333333333333`.
- Judged@5: all three returned documents have qrel rows, including grade-zero
  `d3`; `3/min(5,3) = 1`.
- Supporting Document Recall@5: `max(2/2,1/2) = 1`.
- Complete Evidence Recall@5: `1`, because `E1` is a subset of
  `{d1,d2,d3}`.
- Candidate Recall: `max(1/2,2/2) = 1`.
- Candidate Complete Evidence: `1`, because `E2` is entirely present.
- Candidate Reduction Ratio: `10/2 = 5`.
- Empty Scope: `0`.

Every displayed numeric answer agrees exactly except the rounded NDCG
representation, whose difference is below the required tolerance.

For the empty-candidate variant, Reviewer A treated the condition as an actual
graph-scoped execution with no retrieval fallback, so the graph-scoped ranking
is empty:

- NDCG@5: `0`.
- Recall@5: `0/2 = 0`.
- Success@1: `0`.
- Precision@5: `0/5 = 0`.
- MRR@10: `0`.
- AP: `0`.
- Judged@5: `0` under the explicit empty-result rule.
- Supporting Document Recall@5: `0`.
- Complete Evidence Recall@5: `0`.
- Candidate Recall: `max(0/2,0/2) = 0`.
- Candidate Complete Evidence: `0`.
- Candidate Reduction Ratio: null because `C_q=0`.
- Empty Scope: `1`.

The four values explicitly displayed for the variant in section 5.7 agree:
Candidate Recall `0`, Candidate Complete Evidence `0`, Candidate Reduction
Ratio null, and Empty Scope `1`.

### Implementation-clarity review

Reviewer A identified these blocking ambiguities:

1. **Empty-scope status encoding (sections 4.6, 5.1, 5.4).** The null,
   macro-excluded reduction ratio has no compatible per-metric status.
2. **Excluded-query representation and population counts (sections 3.5, 3.7,
   4.6, 5.1, 8).** The contract does not say whether excluded IDs appear in
   `queries.jsonl`, each run's `queries` array, neither, or only exclusion
   summaries, or whether they affect `query_population_sha256`.
3. **Undefined `graph-valid` and `otherwise graph-eligible` populations
   (sections 5.4, 6.2, 8).** No formal predicate determines D-G membership or
   the derived-resolution-coverage denominator.
4. **Native chunk-hit exhaustion (section 4.3).** The contract does not fix how
   many native chunk hits must be requested before document projection.
5. **Run-configuration hash inputs (section 4.2).** Exact preimages for
   `seed_policy_sha256` and `traversal_policy_sha256` and allowed values for
   multiple configuration fields are not fully specified.
6. **Micro evidence output matrix (sections 4.6, 5.3, 5.6).** The contract
   does not say which separate micro objects are required or their exact field
   sets.
7. **Floating-point byte determinism (sections 2.2, 4.7, 5.2).** Metric
   arithmetic does not pin a reproducible `log2`, operation order, or final
   rounding rule, allowing platform last-bit differences in canonical f64
   strings.
8. **Derived longest-match length domain (section 6.2).** It is unclear
   whether greatest Unicode-scalar length is measured in original or fully
   normalized/case-folded text.
9. **Stale-selection invalidation scope (sections 4.7, 5.1).** `the run` and
   `the complete run` do not say whether the affected run ID, seed-lane
   matrix, or whole A-G artifact is rejected.

Reviewer A found these policies sufficiently clear: alternatives are not
pooled and their macro and micro tie rules are explicit; truncated partial
results remain valid and scored; missing paths make only Path Accuracy
`not_applicable`; filter conflicts are pre-freeze exclusions; explicit and
derived lanes are not aggregated; ordinary macros are arithmetic means apart
from the empty-ratio contradiction; first-chunk document deduplication and
native/IDCG/evidence/path tie rules are deterministic after a native hit list
exists; and canonical structural JSON ordering is clear apart from floating
arithmetic.

### Reviewer A conclusion

**FAIL.** The formulas reproduce, but the empty-scope status contradiction,
undefined graph populations, unspecified native chunk retrieval depth,
incomplete run-hash preimages, and floating determinism can produce materially
different populations, rankings, IDs, aggregates, and artifact bytes.

## Reviewer B complete result

### Query-validity dry-run

Status is metric- and lane-specific; it is not always a single property of the
query.

#### A. No positive qrel

- Status: `excluded_pre_freeze`.
- Exclusion reason: `no_relevant_documents`.
- Execution: never executed.
- Retained lanes: none; the query is removed from every run.
- Macro participation: none for ordinary retrieval metrics.
- Validity: no invalidation if excluded before freeze; leaving it executable
  violates sections 3.7 and 5.1.
- Reporting: exclusion row and reason/count reporting, including split counts
  before and after the reason.

#### B. Declares `evidence` but has no evidence judgment

- Status: no valid per-query execution status repairs this input; section 5.1
  says the collection is invalid.
- Exclusion reason: none under the stated case. Although
  `missing_complete_evidence` is an allowed pre-freeze reason, an included
  query declaring `evidence` without its judgment is invalid.
- Execution: no canonical run should execute.
- Retained lanes: none validly execute.
- Macro participation: none.
- Validity: invalidates the collection.
- Reporting: collection-validation error. The contract does not prescribe an
  exact diagnostic artifact for a rejected collection.

#### C. Valid retrieval and evidence judgments, no expected path

- Status: retrieval and evidence metrics remain `valid`; Path Accuracy is
  `not_applicable`.
- Exclusion reason: none.
- Execution: yes.
- Retained lanes: every otherwise-applicable whole-corpus and seed lane.
- Macro participation: retrieval and evidence values participate; Path
  Accuracy does not.
- Validity: neither collection nor run is invalid.
- Reporting: Path Accuracy's `not_applicable` status and count.

#### D. Seed resolution succeeds, projection is empty

- Status: `valid`; section 5.1 makes an empty graph selection a valid scored
  outcome.
- Exclusion reason: none.
- Execution: yes; there is no fallback.
- Retained lanes: its graph lane and otherwise-applicable whole-corpus/paired
  population.
- Macro participation: applicable scored metrics participate. The empty
  outcome is not an exclusion.
- Validity: neither collection nor run is invalid.
- Reporting: valid-status counts and the empty outcome; sections 4.4 and 5.4
  require detailed counts outside the four validity sections.

#### E. Valid but truncated graph result

- Status: `valid`.
- Exclusion reason: none.
- Execution: yes, using the actual partial result.
- Retained lanes: every otherwise-applicable graph and paired whole-corpus
  population.
- Macro participation: applicable metrics participate.
- Validity: neither collection nor run is invalid.
- Reporting: separate truncation reporting; no retry, enlarged limit, or
  fallback is authorized.

#### F. Stale or generation-mismatched selection

- Status: `invalid_execution` in the receiving graph-scoped run.
- Exclusion reason: none; it is not a pre-freeze exclusion.
- Execution: the scoped operation may be attempted, but it cannot produce a
  valid canonical result and the selection cannot be accepted.
- Retained lanes: the affected graph run is invalid; unrelated whole-corpus or
  correctly executed lanes do not become exclusions.
- Macro participation: never silently omitted or treated as zero; it fails the
  run gate.
- Validity: invalidates the affected run under section 5.1.
- Reporting: `invalid_execution` status/count and stale/generation-mismatch
  diagnostic.

#### G. Positive judgment conflicts with the metadata filter

- Status: `excluded_pre_freeze`.
- Exclusion reason: `filter_label_conflict`.
- Execution: no.
- Retained lanes: none; the query is removed from every run.
- Macro participation: none.
- Validity: no invalidation if detected and frozen; a collection retaining it
  as executable is invalid.
- Reporting: exclusion row, reason count, and split before/after counts.

#### H. Derived resolver finds no alias

- Status: `excluded_pre_freeze` only in that derived lane.
- Exclusion reason: `derived_seed_no_match`.
- Execution: not executed in derived D-G; no fallback.
- Retained lanes: whole-corpus runs and, if independently eligible, an
  explicit-seed lane.
- Macro participation: valid whole-corpus and explicit metrics, not conditional
  derived graph metrics.
- Validity: neither collection nor run is invalid when frozen before tuning.
- Reporting: the full section 6.2 resolver diagnostic, frozen derived
  exclusion/hash, and resolution coverage with this failure in the
  denominator.

#### I. Multiple distinct longest-match seeds

- Status: `excluded_pre_freeze` only in that derived lane.
- Exclusion reason: `derived_seed_ambiguous`.
- Execution: not executed in derived D-G; no shorter match or other seed
  fallback.
- Retained lanes, macro participation, and validity: the same as H.
- Reporting: retained aliases and offsets, candidate seeds, failure reason and
  provenance, frozen exclusion/hash, and resolution coverage.

#### J. Whole-corpus valid, derived-seed graph lane excluded

- Status: `valid` in whole-corpus runs and `excluded_pre_freeze` in the derived
  graph lane.
- Exclusion reason: the frozen reason that occurred,
  `derived_seed_no_match` or `derived_seed_ambiguous`; the case alone does not
  select one.
- Execution: whole-corpus yes; derived D-G no.
- Retained lanes: whole corpus and any independently eligible explicit lane.
- Macro participation: whole-corpus and explicit metrics, not conditional
  derived metrics.
- Validity: neither collection nor run is invalid.
- Reporting: the derived exclusion reason/count, complete resolver diagnostic,
  frozen exclusion hash, and coverage including this query in its denominator.

### Metric dry-run

Inputs:

- Relevant documents: `L_q={d1,d2}`.
- Ranking: `[d2,d3,d1]`.
- Grades: `d1=2`, `d2=1`, `d3=0`.
- Evidence alternatives: `E1={d1,d2}`, `E2={d1,d4}`.
- Candidates: `{d1,d4}`.
- `N_q=10`, `C_q=2`.

Calculations:

- DCG@5: `(2^1-1)/log2(2) + (2^0-1)/log2(3) +
  (2^2-1)/log2(4) = 1 + 0 + 3/2 = 2.5`.
- IDCG@5: `3 + 1/log2(3) = 3.6309297535714578`.
- NDCG@5: `2.5/3.6309297535714578 = 0.6885288809404666`.
  The displayed value is `0.688528880940467`; absolute difference is
  `4.440892098500626e-16`, below `1e-12`.
- Recall@5: `2/2 = 1`.
- Success@1: `1`, because the first result `d2` is relevant.
- Precision@5: `2/5 = 0.4`.
- MRR@10: `1`, because the first relevant rank is 1.
- AP: `(1 + 2/3)/2 = 0.8333333333333333`.
- Judged@5: `3/min(5,3) = 1`, including grade-zero `d3`.
- Supporting Document Recall@5: `max(2/2,1/2) = 1`.
- Complete Evidence Recall@5: `1`, because `E1` is contained in the returned
  set.
- Candidate Recall: `max(1/2,2/2) = 1`.
- Candidate Complete Evidence: `1`, because `E2` is in the candidate set.
- Candidate Reduction Ratio: `10/2 = 5`.
- Empty Scope: `0`.

All displayed numeric values agree within absolute tolerance `1e-12`.

For the empty-candidate variant, Reviewer B read the sentence literally as
changing only the candidate set to empty and `C_q` to zero while retaining the
independently stipulated ranking:

- Candidate Recall: `max(0/2,0/2) = 0`.
- Candidate Complete Evidence: `0`.
- Candidate Reduction Ratio: null.
- Empty Scope: `1`.
- `N_q=10`, `C_q=0`.
- Under this literal candidate-only change, ranking-based retrieval and final
  evidence metrics remain unchanged.

Reviewer B also stated that if the intended variant instead represents an
actual graph-scoped execution, the empty candidate scope necessarily produces
an empty ranking, making NDCG, Recall, Success, Precision, MRR, AP, Judged,
Supporting Document Recall, and Complete Evidence Recall all `0`. Section 5.7
does not explicitly state that its pre-stipulated ranking is replaced, so the
two variants cannot be conflated without inventing a policy.

### Implementation-clarity review

Reviewer B identified these blocking ambiguities:

1. **Per-run populations and task semantics (sections 3.6, 5.1, 6.2, 8).**
   `retrieval-valid`, `evidence-valid`, `graph-valid`, and `otherwise
   graph-eligible` are not fully defined from tasks and seed fields.
2. **Empty-scope reduction status (sections 4.6, 5.1, 5.4).** The null,
   macro-excluded ratio has no compatible status.
3. **`metrics.json` shape and counts (section 4.6).** Status varies by metric,
   but the exact nesting of status counts and the `queries`, `macro`, and
   `micro` objects is not defined.
4. **Collection-wide excluded queries in metric artifacts (sections 3.7, 4.6,
   5.1).** It is unclear where excluded IDs appear and how their counts relate
   to post-exclusion `Q`.
5. **Chunk retrieval depth before document projection (section 4.3).** Native
   overfetch/exhaustion is not specified.
6. **Stale-selection invalidation scope (sections 4.7, 5.1).** `the run` versus
   `the complete run` permits different gate scopes.
7. **Derived population and coverage denominator (section 6.2).** `all
   otherwise graph-eligible queries` is undefined.
8. **Run-configuration identity (section 4.2).** Several allowed values,
   nullability rules, exact hash inputs, and the run-letter-to-field mapping
   are incomplete.
9. **Generation fingerprint hashing (section 4.4).** Ordering, framing, and
   exact canonical preimage are not defined.
10. **Canonical JSON escape spelling (section 2.2).** `minimal JSON string
    escaping` does not settle equivalent escape spellings such as hex digit
    case.
11. **Worked example empty-candidate variant (section 5.7).** It changes the
    candidate set without stating whether the separately stipulated ranking
    also changes. This blocks use of the variant as a deterministic
    conformance case, though not the evaluator formulas for fully specified
    real inputs.

Reviewer B found these policies sufficiently clear: alternative evidence sets
are not pooled and maxima/micro tie-breaking are explicit; empty scope is a
valid scored outcome without fallback apart from the ratio-status problem;
truncated partial results remain in all applicable denominators; stable chunk,
IDCG, evidence, and derived-seed tie handling is specified; canonical records
are documents and first-chunk deduplication is defined after a native result
list exists; explicit and derived lanes are never aggregated; and V3's
exclusion-only derived policy supersedes the roadmap's earlier allowance for a
documented fallback.

### Reviewer B conclusion

**FAIL.** Formulas and most individual validity cases are clear, but unstated
population construction, empty-ratio status, artifact shape, native overfetch,
stale-failure scope, hashing preimages, and run-configuration values can
produce materially different Phase 1 implementations.

## Coordinator comparison of cases A-J

| Case | Reviewer A | Reviewer B | Coordinator comparison |
| --- | --- | --- | --- |
| A | Collection-wide `excluded_pre_freeze`; `no_relevant_documents`; no execution or macro participation | Same | Match |
| B | Included evidence-task query without evidence judgment invalidates collection; no valid execution status | Same | Match |
| C | Retrieval/evidence `valid`; Path Accuracy `not_applicable`; executes in otherwise-eligible lanes | Same | Match |
| D | Empty projection is valid, executes without fallback; zero-valued applicable metrics, Empty Scope `1`, reduction null | Same outcome; separately flags reduction-status problem | Match on validity and execution; same blocking status ambiguity |
| E | Truncation is valid; actual partial result executes and remains in denominators; report reason/rates | Same | Match |
| F | `invalid_execution`, no exclusion, no silent macro omission, affected graph run fails | Same | Match on case classification; both separately flag ambiguity over whether `the complete run` widens invalidation |
| G | Collection-wide `excluded_pre_freeze`; `filter_label_conflict`; no execution or macro participation | Same | Match |
| H | Derived-lane `excluded_pre_freeze`; `derived_seed_no_match`; whole and eligible explicit lanes remain; coverage failure | Same | Match |
| I | Derived-lane `excluded_pre_freeze`; `derived_seed_ambiguous`; no shorter fallback; whole/eligible explicit lanes remain | Same | Match |
| J | Whole-corpus `valid`, derived lane excluded for its actual no-match or ambiguous reason; whole/eligible explicit lanes remain | Same | Match |

The reviewers reached the same case-by-case validity decisions for A-J. That
agreement does not satisfy the gate because both found blocking policies that
the cases alone do not exercise, and because the empty-candidate worked-example
variant produced different readings.

## Coordinator metric comparison

### Primary section 5.7 example

| Metric | Reviewer A | Reviewer B | Absolute difference | Contract displayed | Maximum reviewer-to-contract difference |
| --- | ---: | ---: | ---: | ---: | ---: |
| NDCG@5 | 0.6885288809404666 | 0.6885288809404666 | 0 | 0.688528880940467 | 4.440892098500626e-16 |
| Recall@5 | 1 | 1 | 0 | 1 | 0 |
| Success@1 | 1 | 1 | 0 | 1 | 0 |
| Precision@5 | 0.4 | 0.4 | 0 | 0.4 | 0 |
| MRR@10 | 1 | 1 | 0 | 1 | 0 |
| AP | 0.8333333333333333 | 0.8333333333333333 | 0 | 0.8333333333333333 | 0 |
| Judged@5 | 1 | 1 | 0 | 1 | 0 |
| Supporting Document Recall@5 | 1 | 1 | 0 | 1 | 0 |
| Complete Evidence Recall@5 | 1 | 1 | 0 | 1 | 0 |
| Candidate Recall | 1 | 1 | 0 | 1 | 0 |
| Candidate Reduction Ratio | 5 | 5 | 0 | 5 | 0 |
| Empty Scope | 0 | 0 | 0 | 0 | 0 |

Maximum finite reviewer-to-reviewer difference for the primary example: `0`.
Maximum difference between either reviewer and a displayed contract value:
`4.440892098500626e-16`, below `1e-12`.

### Empty-candidate variant

| Metric | Reviewer A | Reviewer B | Comparison |
| --- | ---: | ---: | --- |
| Candidate Recall | 0 | 0 | Match; absolute difference 0 |
| Candidate Complete Evidence | 0 | 0 | Match; absolute difference 0 |
| Candidate Reduction Ratio | null | null | Match; no finite difference |
| Empty Scope | 1 | 1 | Match; absolute difference 0 |
| NDCG@5 | 0 | 0 only if the ranking is also emptied; otherwise unchanged at 0.6885288809404666 | Material interpretation disagreement; no single reviewer-to-reviewer difference |
| Recall@5 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |
| Success@1 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |
| Precision@5 | 0 | 0 if ranking emptied; otherwise 0.4 | Material interpretation disagreement |
| MRR@10 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |
| AP | 0 | 0 if ranking emptied; otherwise 0.8333333333333333 | Material interpretation disagreement |
| Judged@5 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |
| Supporting Document Recall@5 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |
| Complete Evidence Recall@5 | 0 | 0 if ranking emptied; otherwise 1 | Material interpretation disagreement |

The explicitly displayed empty-candidate values agree exactly. Section 5.7
does not say whether its hypothetical replaces only the candidate set or also
the already-stipulated ranking, so the reviewers did not reach one unique value
for every requested metric in that variant. The coordinator does not select
one reading.

## Combined ambiguity findings

### Material findings identified by both reviewers

1. **Graph/run population predicates and task semantics:** sections 3.6, 5.1,
   5.4, 6.2, and 8 do not formally define `retrieval-valid`,
   `evidence-valid`, `graph-valid`, or `otherwise graph-eligible` from query
   tasks, explicit seeds, and derived policy IDs. This changes run membership,
   population hashes, paired populations, and resolution coverage.
2. **Empty-scope ratio status:** sections 4.6, 5.1, and 5.4 require a null
   Candidate Reduction Ratio omitted from its macro while allowing only a
   `valid` status that participates or a `not_applicable` status defined by
   missing judgment. This blocks exact per-query status serialization.
3. **Excluded-query and metric-count representation:** sections 3.5, 3.7, 4.6,
   and 5.1 define `Q` after collection-wide exclusions while requiring
   excluded status counts, without defining whether excluded IDs appear in
   collection queries, run queries, top-level summaries, or population hashes.
   Section 4.6 also does not define how four status counts are represented when
   status differs by metric.
4. **Native chunk-hit depth before document projection:** section 4.3 does not
   define the native hit request depth or overfetch rule before scanning to
   `evaluation_depth` unique documents. Duplicate chunks can therefore change
   projected rankings and all retrieval metrics.
5. **Run-configuration identity:** section 4.2 does not completely enumerate
   values/nullability and exact hash preimages, notably
   `traversal_policy_sha256`, explicit-lane seed policy hashing,
   `metadata_filter_policy_id`, metric/normalization values, and the complete
   run-letter mapping. Different implementations can emit different run IDs.
6. **Stale-selection invalidation scope:** sections 4.7 and 5.1 use `the
   complete run` and `the run` without specifying whether invalidation covers
   one run ID, one seed-lane matrix, or the whole A-G artifact.
7. **Exact deterministic artifact schema/preimages:** both reviewers found
   missing details capable of changing byte output. Reviewer A identified
   unpinned floating metric arithmetic and incomplete micro evidence object
   requirements; Reviewer B identified an undefined generation-fingerprint
   preimage and incomplete `metrics.json` object shape.

### Additional material finding from Reviewer A

8. **Derived longest-match measurement domain:** section 6.2 does not state
   whether greatest Unicode-scalar length is measured before or after full
   normalization/case-fold expansion, which can change seed resolution and
   derived populations.
9. **Floating arithmetic determinism:** sections 2.2, 4.7, and 5.2 prescribe
   f64 serialization but not reproducible `log2`, operation order, or final
   rounding, allowing last-bit differences in byte-identical artifacts.
10. **Micro evidence output matrix:** sections 4.6, 5.3, and 5.6 define a
    diagnostic micro concept without enumerating the required metric objects
    and exact fields.

### Additional material findings from Reviewer B

11. **Generation fingerprint preimage:** section 4.4 names inputs but does not
    define their ordering, framing, or canonical hash preimage.
12. **Canonical JSON escaping:** section 2.2's `minimal JSON string escaping`
    does not fully choose among equivalent spellings such as hexadecimal digit
    case in Unicode escapes.
13. **Empty-candidate worked-example transition:** section 5.7 does not state
    whether making the candidate set empty also replaces the already-declared
    ranking with an empty graph-scoped ranking. This produced an actual
    independent-review disagreement.

The findings above are blocking for the Phase 0 criterion because they can
produce different valid-looking query populations, coverage denominators,
rankings, statuses, run IDs, hashes, gate scopes, metrics, or deterministic
bytes. They are not merely editorial notes.

## Coordinator verdict

Reviewer A verdict: **FAIL**.

Reviewer B verdict: **FAIL**.

The reviewers matched on all A-J validity decisions and on every finite value
in the primary worked example. The maximum primary-example metric difference
between reviewers was `0`, and the maximum difference from a displayed
contract value was `4.440892098500626e-16`, within the required tolerance.

They did not reach the same complete implementable interpretation. Both found
multiple blocking unspecified policies, and their readings of the section 5.7
empty-candidate variant differed materially. The coordinator has not resolved
the disagreement by majority vote or by choosing one interpretation.

**Coordinator verdict: FAIL. The Phase 0 independent dry-run gate did not pass,
Phase 0 remains open, and Phase 1 must not begin under the current contract.**

## Exact integration handoff

The integration task must perform the following actions in the source-of-truth
documents before attempting to close Phase 0:

1. Keep the V3 contract status and roadmap Phase 0 gate marked as independent
   dry-run pending/open. Do not mark the gate complete from this review.
2. Conduct an integration review of the exact contract wording in sections
   2.2, 3.5-3.7, 4.2-4.7, 5.1, 5.3-5.7, 6.2, and 8.
3. Amend the contract to define, without inference:
   - formal retrieval-valid, evidence-valid, path-valid, graph-valid, and
     otherwise-graph-eligible predicates and exact per-lane populations;
   - whether and where collection-wide and lane-scoped exclusions appear in
     `queries.jsonl`, run query arrays, status counts, and population hashes;
   - a legal per-metric status and macro-denominator rule for null Candidate
     Reduction Ratio on a valid empty scope;
   - the exact `metrics.json` schema, per-metric status-count structure, macro
     and micro objects, and required micro evidence outputs;
   - the native chunk-hit request/overfetch/exhaustion rule before unique
     document projection;
   - every run-configuration enum/nullability rule and exact seed, traversal,
     source, and generation hash preimage;
   - the invalidation scope of a stale or generation-mismatched selection;
   - the scalar-length domain used by derived longest-alias selection;
   - deterministic floating metric arithmetic/rounding and exact JSON escape
     spelling sufficient for byte identity; and
   - whether the section 5.7 empty-candidate variant replaces the ranking, then
     display every affected metric under that one explicit interpretation.
4. Record the contract integration as a new reviewed source-of-truth revision
   in the roadmap/contract as appropriate; do not treat this report itself as
   authority to select any missing policy.
5. After the wording is integrated, rerun the complete independent dry-run
   with exactly two isolated reviewers. Phase 0 may close only if both then
   identify the same valid populations, calculate every requested primary and
   empty-variant metric within `1e-12`, find no blocking unstated policy, and
   reach the same implementable interpretation.
6. Begin Phase 1 only after that rerun passes and the integration task records
   the gate as complete.

## Change confirmation

This review created only
`docs/product/reports/graph-retrieval-phase-0-independent-review.md`. It did not
change production Rust, Swift, Python, FFI, wrapper, benchmark, or evaluation
code. It did not change the V3 source-of-truth contract, the roadmap, working
memory, or any pre-existing dirty-worktree file. No files were staged or
committed.
