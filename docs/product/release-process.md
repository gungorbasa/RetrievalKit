# RetrievalKit v0.1.0 release process

Status: release-candidate and runtime authorization implementation complete;
external publication blocked on repository/environment configuration and the
remaining release gates.

The automated release candidate ships the Swift, Python, Node.js, and Kotlin
previews from one signed source revision. Python, Node, and Kotlin retain
separate base and graph native aggregates because loading both into one process
is unsupported. Swift publishes one graph-capable aggregate containing both
native capability surfaces.

The approved npm package names are `@gungorbasa/retrievalkit` and
`@gungorbasa/retrievalkit-graph`. npm rejected the equivalent unscoped base
name as too similar to an existing package, so both Node packages use one
consistent owner scope. The approved Maven group is
`io.github.gungorbasa`. Approval of those identities does not imply that the
registries are configured: npm bootstrap/trusted-publisher setup, Maven Central
namespace verification, signing keys, protected environments, and registry
credentials remain fail-closed external prerequisites.

## Release contents

- `RetrievalKitGraphFFI.xcframework.zip` for all public Swift products.
- macOS arm64 `retrievalkit` and `retrievalkit-graph` wheels for CPython 3.10–3.14.
- macOS arm64 npm tarballs for `@gungorbasa/retrievalkit` and
  `@gungorbasa/retrievalkit-graph`.
- Maven publications under `io.github.gungorbasa` for JVM base/graph and
  Android base/graph, limited to the targets declared in their package metadata.
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
5. Build and inspect the two approved npm tarballs and the four unsigned Maven
   publications.
6. Smoke-test every Swift product and a combined base-plus-graph consumer,
   every Python artifact, both npm tarballs, and all JVM/Android publications
   in fresh consumer environments.
7. Assemble the closed release bundle with checksums, SBOM, and provenance.
8. Repeat from a second clean root and compare every byte.
9. Validate the bundle independently and complete the
   [release approval checklist](release-approval-checklist.md).

The manual `release-candidate.yml` workflow performs the build and validation
without publishing. It never invokes a physical-device command.

## Node and Kotlin candidate construction

The Node assembler requires the approved npm names and an explicit
`--names-approved` assertion. It builds and inspects separate macOS arm64 base
and graph tarballs, preserves the checked-in packages as private repository
placeholders, proves graph exclusion from the base artifact, and emits SHA-256,
SHA-512, and package-integrity evidence:

```bash
python3 scripts/release/assemble_node_packages.py \
  --base-name @gungorbasa/retrievalkit \
  --graph-name @gungorbasa/retrievalkit-graph \
  --names-approved \
  --version 0.1.0 \
  --output dist/release/node
```

The Kotlin assembler uses the approved Maven group. It produces four
isolated publications—JVM base/graph and Android base/graph—with POM, sources,
Javadoc, checksums, architecture validation, and a deterministic Central Portal
bundle. Central publication also requires namespace verification, detached PGP
signatures, and a Portal token:

```bash
python3 scripts/release/assemble_kotlin_packages.py \
  --group io.github.gungorbasa \
  --version 0.1.0 \
  --java-home "$JAVA_HOME" \
  --output dist/release/kotlin
```

These commands only construct candidate packages; they never reserve names,
upload artifacts, or publish. The candidate workflow adds their exact outputs
to the closed release bundle. Publication consumes those authorized bytes
rather than rebuilding packages in a registry job.

## Authorization model

The protected GitHub `release` environment is the publication authority. The
repository deliberately does not contain a completed authorization file.
Committing a file whose `source_revision` must equal the commit containing that
file would be self-referential: changing the file necessarily changes the
commit. A template showing the runtime record is available at
[`release/publication-authorization-provenance.example.json`](../../release/publication-authorization-provenance.example.json).

Publication has two distinct stages:

1. The read-only `validate-candidate` job checks out the signed tag and requires
   the workflow run itself to have been dispatched from that tag. It downloads
   the candidate, scheduled-gate, and release-gate artifacts and their GitHub
   run metadata. `publication_authorization.py candidate` fails unless every
   run succeeded at the exact tag revision, both Phase 7 results passed at that
   revision, and the bundle manifest/inventory/checksum identities agree. It
   emits immutable `candidate-evidence.json`.
2. The `github-release` job enters the protected `release` environment. After a
   required reviewer approves that deployment, it fetches the workflow-run
   approval events from GitHub, requires an `approved` event naming the
   `release` environment, and emits
   `publication-authorization-provenance.json`. The record embeds the closed
   candidate evidence and binds the reviewer, approval timestamp, exact tag and
   commit, workflow ref, publication run/attempt and start time, all three
   prerequisite run IDs, gate-result hashes, and bundle
   inventory/checksum/manifest hashes.

`validate_release.py --publication` accepts only that post-approval record and
the exact candidate evidence from which it was made. An empty approval API
response, an unprotected environment, an approval predating the current run
attempt, a branch-based workflow ref, a different revision/run/attempt, failed
or stale gate evidence, or any changed candidate byte fails closed.

The authorization record, its SHA-256, and the candidate evidence are retained
for 180 days as a dedicated Actions artifact. They are also attested and
attached to the GitHub Release alongside the package artifacts. PyPI, npm, and
Maven publication jobs depend on successful completion of this protected job.

## npm trusted publication

The npm job runs in the protected `npm` environment with `id-token: write` and
no npm token. It installs the pinned OIDC-capable npm CLI, verifies the complete
authorized bundle checksum set, stages only
`artifacts/node/gungorbasa-retrievalkit-0.1.0.tgz` and
`artifacts/node/gungorbasa-retrievalkit-graph-0.1.0.tgz`, and publishes those
tarballs with `--provenance`. It then compares each registry `dist.integrity`
value with the authorized inventory, attests the tarballs/evidence, and retains
the publication record for 180 days.

npm trusted publishing cannot establish a package name that does not exist.
Before the first RetrievalKit release, an npm owner must:

1. bootstrap both names with a separately reviewed non-release version using a
   short-lived granular token and required 2FA;
2. configure each package's trusted publisher for the public
   `gungorbasa/RetrievalKit` repository, `publish-release.yml` workflow, and
   `npm` environment;
3. revoke the bootstrap token; and
4. confirm both names and trusted-publisher configuration through the required
   `npm_trusted_publishers_ready` dispatch input.

The pre-approval job verifies that both public package records exist. The npm
job verifies that `0.1.0` is unused. Missing bootstrap, missing OIDC trust, an
existing version, a changed tarball, or a registry integrity mismatch fails
closed. Because two npm uploads cannot be transactional, a failure after the
first succeeds requires an incident record and fix-forward release; published
npm versions are never overwritten.

## Maven Central publication

The Maven job runs in the protected `maven` environment. It verifies the
authorized bundle, requires group `io.github.gungorbasa`, copies the exact 16
authorized primary POM/JAR/AAR files, and records their SHA-256 values before
signing. It imports the environment-protected PGP key, creates detached ASCII
signatures without rebuilding any primary artifact, constructs the signed
Central Portal bundle, and uploads it with `publishingType=AUTOMATIC` using the
protected Central user-token secrets. The signed bundle, publication evidence,
and deployment ID are attested or retained for 180 days.

The `maven` environment must contain:

- `MAVEN_GPG_PRIVATE_KEY`: armored private signing key;
- `MAVEN_GPG_KEY_ID`: exact signing key identity;
- `MAVEN_GPG_PASSPHRASE`: key passphrase;
- `MAVEN_CENTRAL_USERNAME` and `MAVEN_CENTRAL_PASSWORD`: Central Portal
  user-token credentials.

Central namespace verification, public signing-key distribution, token
creation, and environment protection are external owner actions. The required
`maven_central_ready` dispatch input is an explicit assertion that they are
complete; missing secrets, signing failures, namespace rejection, or upload
failure stops publication.

The dedicated RetrievalKit release key is checked in as a public verification
artifact at
[`release/retrievalkit-release-signing-key.asc`](../../release/retrievalkit-release-signing-key.asc).
Its fingerprint is
`0E82 F1A5 487A 4EF3 CCF1 ED6C 3932 66CD 4DD1 58ED`; it expires on
2028-07-25. The private key is never stored in the repository. Its local
passphrase is retained in macOS Keychain under
`RetrievalKit-Maven-GPG`, and CI receives the key only through protected
environment secrets.

## Required external GitHub configuration

Before dispatching publication:

- the repository plan and visibility must support required reviewers for the
  `release` environment as described by
  [GitHub's deployment protection rules](https://docs.github.com/actions/reference/deployments-and-environments#deployment-protection-rules);
- create the `release` environment, add at least one owner-approved required
  reviewer, restrict deployment to signed release tags, and preferably enable
  prevention of self-review;
- create the separate `pypi` environment and configure PyPI trusted publishing
  for this repository and workflow;
- create the protected `npm` environment and configure both bootstrapped
  packages for
  [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/);
- verify `io.github.gungorbasa` in Central Portal, publish the PGP public key,
  and configure the protected `maven` environment using the
  [Central Publisher API](https://central.sonatype.org/publish/publish-portal-api/);
- confirm the workflow token can read Actions run metadata and the
  [`GET /repos/{owner}/{repo}/actions/runs/{run_id}/approvals` review-history endpoint](https://docs.github.com/rest/actions/workflow-runs#get-the-review-history-for-a-workflow-run);
- create and push the verified signed release tag. The publication workflow
  must be dispatched with that tag as its workflow ref, not merely supplied as
  the `tag` input.

The existing repository is public. The `release`, `pypi`, `npm`, and `maven`
environments are restricted to `v*` tags; `release` requires approval from the
repository owner. Self-review is enabled because the owner is currently the
only eligible reviewer. The three registry environments run only after that
protected release approval succeeds.

Example dispatch after all external gates exist:

```bash
gh workflow run publish-release.yml \
  --ref v0.1.0 \
  -f tag=v0.1.0 \
  -f candidate_run_id=<candidate-run-id> \
  -f scheduled_run_id=<scheduled-phase7-run-id> \
  -f release_gate_run_id=<release-phase7-run-id> \
  -f pypi_trusted_publishers_ready=true \
  -f npm_trusted_publishers_ready=true \
  -f maven_central_ready=true
```

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
- `v0.1.0` is a verified signed tag, the publication workflow runs from that
  tag, and every prerequisite run resolves to the same commit;
- the protected release environment records a required-reviewer approval that
  is bound into the runtime authorization/provenance record.

The publication workflow verifies these gates before requesting a GitHub
Release or registry identity. Its default permissions are read-only; write,
registry credentials, and `id-token` permissions exist only in protected
publication jobs.

## Rollback

If validation fails before publication, discard the candidate artifacts and
fix forward on a new revision. Never reuse a checksum for changed bytes.

If an uploaded artifact is wrong but no consumer release has been announced,
mark the GitHub Release as a draft, remove the incorrect assets, and create a
new candidate from a new tag. PyPI files cannot be replaced; yank the affected
release. npm and Maven versions are also immutable; deprecate or otherwise
withdraw the affected version according to registry policy and publish a new
patch. Update SwiftPM URLs/checksums and record the incident in the changelog
and security advisory when relevant. Never move or recreate a published tag.
