# VectorKit Benchmark Publication Contract v1

Status: frozen

Contract data: `benchmarks/publication/contract-v1.json`

Frozen: 2026-07-21

## Purpose and boundary

This contract governs the repository-local Phase 6 publication package. It
does not authorize a website, release, upload, push, announcement, or other
external publication. It does not change the VectorKit product boundary of
fewer than 50K chunks and it does not reopen measurement.

The package reports three evidence families separately: Phase 3 HotpotQA
retrieval quality, Phase 5 Mac exact-search comparisons, and Phase 4b physical
device qualification. Exact search, ANN, BM25, hybrid ranking, graph selection,
persistence, filtering, and deletion remain distinct capabilities. Results
from non-equivalent capabilities must not be combined into a winner table.

## Closed report inventory

The publication root contains exactly the ten paths listed in
`contract-v1.json`. JSON records are schema-versioned. Markdown is a rendered
view; machine-readable claims and values remain authoritative. Extra files,
missing files, directories, or symlinks fail validation.

The claim-register schema contains package dates, status counts, and claim
objects with claim text, status, evidence, scope, system/version, environment,
metric, population, calculation, qualifiers, forbidden interpretations,
revision, expiry/rerun, and license eligibility. The evidence-index schema has
separate quality, Mac, and device families with frozen configuration,
recomputed values, and evidence identities. Licensing, checksums, and manifest
schemas are enumerated in `contract-v1.json`; unknown top-level publication
files are not extensible under V1.

## Claim eligibility

Every proposed statement has a stable ID and one of three statuses:

- `permitted`: all referenced evidence is accepted, or is an explicitly
  qualified negative result, and every scope and qualifier is present.
- `prohibited`: the interpretation contradicts the frozen evidence or policy.
- `withheld`: the evidence or publication permission is insufficient.

A permitted claim must identify exact evidence paths and hashes, workload,
system and version, capability, hardware and OS when applicable, metric,
sample population, calculation, source revision, report date, expiry, required
qualifiers, prohibited interpretations, rerun conditions, and licensing
eligibility. Diagnostic, partial, rejected, failed, or disqualified evidence
cannot support a positive public claim. A qualified negative result may report
a failed gate but cannot turn its disqualified timings into a comparison.

## Numerical reproduction

Published percentiles use nearest rank:
`sorted_samples[ceil(p * n) - 1]`. Phase 5 uses the 100 measured samples after
20 warmups. Phase 4b query tables first compute each of five session
percentiles over 1,000 measured queries after 100 warmups and then publish the
median session percentile. Ratios divide the comparison-system integer
nanoseconds by the VectorKit integer nanoseconds with decimal arithmetic.

Source integers and full-precision values remain in `evidence-index.json`.
Displayed milliseconds use three decimal places, ratios two, and percentages
two, all with decimal `ROUND_HALF_UP`. A changed source value, calculation, or
display rounding fails validation.

## Evidence links and identities

Evidence links are repository-relative and include SHA-256 identities. The
manifest binds the Phase 3, Phase 4b, and Phase 5 frozen artifact identities,
their source revisions, this contract, and the independent validator. Evidence
that is intentionally not redistributed is referenced by identity and
reproduction instructions. Rejected or disqualified paths are forbidden in
permitted positive claims.

## Licensing and redistribution

`licensing.json` records the primary license source, version, use, and
publication decision for each third-party input. Raw HotpotQA inputs and
transformed corpus payloads remain outside the publication root under
repository policy; only identities, acquisition instructions, provenance, and
eligible aggregates are included. Raw device captures, unpublished binaries,
and identifiers are also excluded.

The repository currently has no root project license. This Phase 6 task permits
creation of owner-authored repository artifacts but does not create a general
downstream redistribution grant. External distribution remains withheld until
the owner adds an applicable project license and required notices. Unknown or
incompatible material is excluded.

## Expiration and rerun conditions

Claims expire on 2027-07-21. Validation after that date fails. A claim must be
rerun sooner when its listed benchmark implementation, timing boundary,
retrieval behavior, dependency, embedding model, dataset, population, qrels,
workload, compiler, hardware, OS, or license condition changes. Results do not
silently transfer to a new source revision or environment.

## Canonical serialization and hashing

JSON is UTF-8 with sorted keys, compact separators, no non-finite numbers, and
one trailing LF. Markdown is UTF-8 with LF line endings and one trailing LF.
`checksums.json` contains the SHA-256 for the eight payload files. The manifest
also binds `checksums.json`; its canonical artifact-set hash is SHA-256 over
sorted UTF-8 lines `<relative-path>\t<sha256>\n` for all nine non-manifest
files. `manifest.json` is excluded from its own preimage.

## Independent validation and acceptance

The independent validator must not import the generator. It recomputes the
published Phase 3 quality values, Phase 4b query percentiles and gates, and
Phase 5 percentiles, exact ratios, and ANN recall values from frozen evidence.
It also checks inventory closure, every hash and reference, exact claim
membership and status, licensing decisions, hardware/OS/version qualifiers,
expiry, forbidden interpretations, and deterministic report rendering.

Acceptance requires every gate in `contract-v1.json`, successful mutation
tests, and byte-identical generation into two fresh roots. Any failed gate
makes the publication package ineligible.
