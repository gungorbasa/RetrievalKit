# v0.1.0 release approval checklist

Every item is required unless explicitly marked candidate-only.

## Identity and legal

- [ ] `VERSION`, Cargo, Python, Swift, changelog, and manifests equal `0.1.0`.
- [ ] Release revision is clean and matches the verified signed `v0.1.0` tag.
- [ ] The publication workflow is dispatched with `--ref v0.1.0`; its
  `github.sha` and `github.workflow_ref` resolve to the exact signed tag commit.
- [ ] Root `LICENSE` remains the owner-approved Apache-2.0 text.
- [ ] `NOTICE` retains the owner-approved company attribution and required
  third-party notices.
- [ ] Cargo and Python metadata remain Apache-2.0.

## Evidence and tests

- [ ] Phase 7 PR gates pass.
- [ ] Phase 7 scheduled gates are provisioned and pass for this revision.
- [ ] Phase 7 controlled release gates are provisioned and pass for this revision.
- [ ] Phase 6 publication and README claim validators pass.
- [ ] Numeric claims are historical frozen-revision observations or explicitly reauthorized.
- [ ] Rust, Python, Swift, wrapper-isolation, snippets, links, and package tests pass.
- [ ] The wrapper onboarding qualification succeeds from independent clean-source
  exports for Python, Swift, Node, and Kotlin; retain its machine-readable
  timing, machine/toolchain, and dependency-cache evidence with the release
  record.

## Artifacts

- [ ] The public graph-capable XCFramework contains arm64 macOS, iOS, and iOS
  Simulator slices.
- [ ] Both Python distributions pass on CPython 3.10–3.14 macOS arm64.
- [ ] Every Swift product and the combined base-plus-graph consumer pass; the
  internal graph-neutrality and Python co-import negative tests pass.
- [ ] Two clean roots produce byte-identical artifacts.
- [ ] SHA-256 inventory and SwiftPM checksums independently validate.
- [ ] SPDX SBOM, provenance, attestations, and artifact retention metadata validate.

## Publication

- [ ] The repository plan/visibility supports required reviewers for private
  environments, or the repository has been made public.
- [ ] The `release` environment has an owner-approved required reviewer and is
  restricted to signed release tags; prevention of self-review is enabled when
  the reviewer topology permits it.
- [ ] The candidate, scheduled Phase 7, and release Phase 7 workflow runs all
  succeeded at the exact signed-tag revision.
- [ ] `candidate-evidence.json` binds the three run IDs, two passing result
  hashes, and the bundle inventory/checksum/manifest hashes.
- [ ] The protected `release` environment approval is present in the GitHub
  workflow-run approvals API response.
- [ ] `publication-authorization-provenance.json` validates against the exact
  candidate evidence, tag, revision, publication run ID, and run attempt.
- [ ] The authorization record, SHA-256, and candidate evidence are retained as
  a 180-day Actions artifact and attached to the GitHub Release.
- [ ] GitHub Release is created from the verified tag with validated assets only.
- [ ] The Swift package publishes all four products from the signed revision and
  resolves only `RetrievalKitGraphFFI`.
- [ ] Trusted PyPI publication uploads exactly the validated wheel inventory.
- [ ] Fresh remote SwiftPM and PyPI consumer projects pass.
- [ ] Changelog, compatibility notes, and rollback owner are confirmed.
