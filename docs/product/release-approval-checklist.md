# v0.1.0 release approval checklist

Every item is required unless explicitly marked candidate-only.

## Identity and legal

- [ ] `VERSION`, Cargo, Python, Swift, Node, Kotlin, changelog, and manifests
  equal `0.1.0`.
- [ ] Release revision is clean and matches the verified signed `v0.1.0` tag.
- [ ] The publication workflow is dispatched with `--ref v0.1.0`; its
  `github.sha` and `github.workflow_ref` resolve to the exact signed tag commit.
- [ ] Root `LICENSE` remains the owner-approved Apache-2.0 text.
- [ ] `NOTICE` retains the owner-approved company attribution and required
  third-party notices.
- [ ] Cargo, Python, npm, and Maven metadata remain Apache-2.0.

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
- [ ] Android API 24+ arm64-v8a is recorded as an explicit preview. Its
  cross-compilation, AAR packaging, closed-inventory, ABI/architecture,
  JVM/JNI-contract, and fresh consumer resolution/compilation checks pass.
- [ ] Release evidence states that live Android device acquisition, inference,
  lifecycle, memory, thermal behavior, offline restart, compatibility, and
  performance remain unqualified and deferred. Missing physical-device
  evidence is not a v0.1.0 publication blocker and no device-inference pass is
  claimed.

## Artifacts

- [ ] The public graph-capable XCFramework contains arm64 macOS, iOS, and iOS
  Simulator slices.
- [ ] All three Python distributions pass on CPython 3.10–3.14 macOS arm64.
- [ ] The authorized npm inventory contains exactly
  `@gungorbasa/retrievalkit@0.1.0` and
  `@gungorbasa/retrievalkit-graph@0.1.0` and
  `@gungorbasa/retrievalkit-embedding@0.1.0` macOS arm64 tarballs, plus
  `@gungorbasa/retrievalkit-browser-embedding@0.1.0`.
- [ ] The authorized Maven inventory contains exactly six
  `io.github.gungorbasa` JVM/Android base/graph/embedding publications and 24 primary
  POM/JAR/AAR files.
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
- [ ] All three PyPI projects trust the public repository,
  `publish-release.yml`, and protected `pypi` environment; the
  `pypi_trusted_publishers_ready` dispatch gate is confirmed.
- [ ] All four npm names were bootstrapped with a non-release version; the public
  repository, `publish-release.yml`, and protected `npm` environment are
  configured as trusted publishers; the bootstrap credential is revoked.
- [ ] npm publication uses no registry token, uploads only the four authorized
  tarballs with provenance, and the observed `dist.integrity` values equal the
  authorized inventory.
- [ ] Central Portal verifies `io.github.gungorbasa`; the protected `maven`
  environment contains the PGP identity and Central user-token secrets.
- [ ] The Maven signing key fingerprint is
  `0E82 F1A5 487A 4EF3 CCF1 ED6C 3932 66CD 4DD1 58ED`, matches
  `release/retrievalkit-release-signing-key.asc`, remains publicly retrievable,
  and has not expired or been revoked.
- [ ] Maven publication signs the exact 24 authorized primary files without
  rebuilding them, attests the signed bundle, and retains the Central
  deployment ID.
- [ ] Fresh remote SwiftPM, PyPI, npm, and Maven JVM consumer projects pass.
  A fresh Maven Android consumer resolves and compiles each selected preview
  AAR; this check does not require or claim physical-device execution.
- [ ] Changelog, compatibility notes, and rollback owner are confirmed.
