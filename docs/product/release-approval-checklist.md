# v0.1.0 release approval checklist

Every item is required unless explicitly marked candidate-only.

## Identity and legal

- [ ] `VERSION`, Cargo, Python, Swift, changelog, and manifests equal `0.1.0`.
- [ ] Release revision is clean and matches the verified signed `v0.1.0` tag.
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
  exports for Python, Node, and Kotlin; retain its machine-readable timing,
  machine/toolchain, and dependency-cache evidence with the release record.

## Artifacts

- [ ] Base and graph XCFrameworks contain arm64 macOS, iOS, and iOS Simulator slices.
- [ ] Both Python distributions pass on CPython 3.10–3.14 macOS arm64.
- [ ] Base/graph Swift linkage and Python co-import negative tests pass.
- [ ] Two clean roots produce byte-identical artifacts.
- [ ] SHA-256 inventory and SwiftPM checksums independently validate.
- [ ] SPDX SBOM, provenance, attestations, and artifact retention metadata validate.

## Publication

- [ ] Protected release environment approval is recorded.
- [ ] GitHub Release is created from the verified tag with validated assets only.
- [ ] Base and graph Swift package repositories publish manifests from the same
  signed revision and resolve only their matching native aggregate.
- [ ] Trusted PyPI publication uploads exactly the validated wheel inventory.
- [ ] Fresh remote SwiftPM and PyPI consumer projects pass.
- [ ] Changelog, compatibility notes, and rollback owner are confirmed.
