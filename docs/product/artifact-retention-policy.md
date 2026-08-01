# Release artifact retention policy

## Published releases

Retain the signed tag, source archive, binaries, wheels, checksums, SBOM,
provenance, attestations, approval record, and final validation result for the
lifetime of the release. Repository-level immutable releases must be enabled
before publication so the published Git tag and GitHub Release assets are
locked after publication.

## Release candidates

The public repository's GitHub Actions retention limit is 90 days. Retain
successful candidate bundles and validation logs for that maximum period.
Retain failed candidate manifests and failure summaries for 90 days; large
intermediate build directories may be deleted after the failure is understood.
Never promote a failed candidate by editing its evidence. For a published
release, the validated package artifacts and compact authorization evidence are
also attached to the immutable GitHub Release for lifetime preservation.

## Benchmark evidence

Retain frozen claim registers and compact accepted evidence indefinitely while
any public claim references them. Licensed datasets, device captures, rejected
partials, and large raw measurements follow their existing access and
redistribution rules and are not automatically release assets.

Checksums are identities, not recovery copies. At least two access-controlled
copies of every published binary bundle and its signing/attestation records
must be maintained by the owner.
