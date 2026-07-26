# RetrievalKit v0.1.0 release process

Status: release-candidate implementation complete; external publication blocked.

RetrievalKit ships the Swift and Python previews from one signed source
revision. Python retains separate base and graph native aggregates because
loading both into one process is unsupported. Swift publishes one graph-capable
aggregate containing both native capability surfaces.

## Release contents

- `RetrievalKitGraphFFI.xcframework.zip` for all public Swift products.
- macOS arm64 `retrievalkit` and `retrievalkit-graph` wheels for CPython 3.10–3.14.
- SHA-256 inventory, SwiftPM checksums, SPDX 2.3 SBOM, and in-toto/SLSA-style
  provenance subjects.
- Apache-2.0 `LICENSE` and the RetrievalKit company `NOTICE`.
- One Swift package with `RetrievalKit`, `RetrievalKitGraph`, `EmbeddingKit`,
  and `RetrievalKitPipeline` products.

The root `Package.swift` is the only public Swift package manifest and resolves
only `RetrievalKitGraphFFI`. A consumer selects `RetrievalKit`,
`RetrievalKitGraph`, or both products; all use the same native handle universe.
Repository verification uses `RETRIEVALKIT_USE_LOCAL_ARTIFACTS=1`. Internal
qualification still builds the graph-free `RetrievalKitFFI` artifact separately
to prove the Rust base boundary remains graph-neutral, but that artifact is not
part of the public Swift release.

## Candidate procedure

1. Confirm `VERSION`, Cargo, Python, Swift, changelog, and release configuration
   all identify `0.1.0`.
2. Run Phase 7 PR gates and Phase 6/README claim validation.
3. Build the three-slice arm64 graph-capable XCFramework and canonical zip
   archive; separately run the internal graph-neutrality qualification.
4. Build both wheel distributions for each CPython 3.10–3.14 interpreter.
5. Smoke-test every Swift product and a combined base-plus-graph consumer from
   the unified package, plus every Python artifact in fresh consumer
   environments.
6. Assemble the closed release bundle with checksums, SBOM, and provenance.
7. Repeat from a second clean root and compare every byte.
8. Validate the bundle independently and complete the
   [release approval checklist](release-approval-checklist.md).

The manual `release-candidate.yml` workflow performs the build and validation
without publishing. It never invokes a physical-device command.

## Publication gates

Publication fails closed unless all of the following are true for the release
revision:

- the root `LICENSE` remains Apache-2.0 and `NOTICE` retains the approved
  company attribution;
- Cargo and Python package metadata remain reconciled as Apache-2.0;
- Phase 7 scheduled and controlled release results are provisioned and passed;
- README numeric claims remain explicitly historical or are newly authorized;
- bundle inventory, checksums, SBOM, provenance, attestations, and fresh
  consumer smoke tests pass;
- `v0.1.0` is a verified signed tag matching the authorization;
- the protected release environment receives owner approval.

The publication workflow verifies these gates before requesting a GitHub
Release or trusted PyPI identity token. Its default permissions are read-only;
write and `id-token` permissions exist only in the protected publication jobs.

## Rollback

If validation fails before publication, discard the candidate artifacts and
fix forward on a new revision. Never reuse a checksum for changed bytes.

If an uploaded artifact is wrong but no consumer release has been announced,
mark the GitHub Release as a draft, remove the incorrect assets, and create a
new candidate from a new tag. PyPI files cannot be replaced; yank the affected
release, publish a new patch version, update SwiftPM URLs/checksums, and record
the incident in the changelog and security advisory when relevant. Never move
or recreate a published tag.
