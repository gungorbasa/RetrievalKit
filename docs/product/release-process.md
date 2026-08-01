# RetrievalKit v0.1.0 release process

Status: v0.1.0 scope, identities, platform statuses, and release claims frozen
on 2026-08-01; release-candidate and runtime authorization implementation is
complete. The commit containing the machine-readable `release_freeze` record
is the eligible candidate source revision. Any later source or release-truth
change requires a new freeze commit and repeat validation before candidate
assembly.
The public repository, protected GitHub environments, all three PyPI projects,
all five npm packages, their trusted publishers, and Maven signing identity are
configured. The Maven Central namespace and protected user token are
configured. v0.1.0 publication remains blocked on final registry
re-verification, the signed tag, and the release evidence gates below.

The freeze does not authorize the release-candidate workflow, Phase 7 scheduled
or controlled release workflows, a tag, a GitHub Release, or registry
publication. Those remain separate owner-controlled steps.

The automated release candidate ships the Swift, Python, Node.js, browser, and
Kotlin previews from one signed source revision. Python, Node, and Kotlin retain
separate base and graph native aggregates because loading both into one process
is unsupported. Swift publishes one graph-capable aggregate containing both
native capability surfaces.

The approved npm package names are `@gungorbasa/retrievalkit`,
`@gungorbasa/retrievalkit-graph`, `@gungorbasa/retrievalkit-embedding`,
`@gungorbasa/retrievalkit-browser`, and
`@gungorbasa/retrievalkit-browser-embedding`. npm rejected the equivalent
unscoped base name as too similar to an existing package, so every npm package
uses one consistent owner scope. The approved Maven group is
`io.github.gungorbasa`. All five npm names and all three PyPI names were
bootstrapped and connected to the protected GitHub publication workflow by
2026-08-01.
The `io.github.gungorbasa` Central namespace was verified and its
protected
credentials were installed on 2026-07-26. The signed tag and provisioned
release evidence remain fail-closed external prerequisites.

On 2026-07-31, the owner decided to keep the product name `RetrievalKit` after
considering the unrelated `retrieval-kit` crate. This owner decision resolves
naming as a release blocker; it is not a claim that outside legal counsel
performed trademark clearance. The approved registry identities remain PyPI
`retrievalkit`, `retrievalkit-graph`, and `retrievalkit-embedding`; npm
`@gungorbasa/retrievalkit`, `@gungorbasa/retrievalkit-graph`,
`@gungorbasa/retrievalkit-embedding`,
`@gungorbasa/retrievalkit-browser`, and
`@gungorbasa/retrievalkit-browser-embedding`; and Maven
`io.github.gungorbasa`. Rust crates remain source-only.

## Release contents

- `RetrievalKitGraphFFI.xcframework.zip` for all public Swift products.
- macOS arm64 `retrievalkit`, `retrievalkit-graph`, and
  `retrievalkit-embedding` wheels for CPython 3.10–3.14.
- macOS arm64 npm tarballs for `@gungorbasa/retrievalkit`,
  `@gungorbasa/retrievalkit-graph`, and
  `@gungorbasa/retrievalkit-embedding`, plus the platform-independent
  `@gungorbasa/retrievalkit-browser` Worker/WASM retrieval package and
  `@gungorbasa/retrievalkit-browser-embedding` Worker package.
- Maven publications under `io.github.gungorbasa` for JVM/Android base, graph,
  and embedding packages, limited to the targets declared in their metadata.
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
4. Build all three wheel distributions for each CPython 3.10–3.14 interpreter.
5. Build and inspect the five approved npm tarballs and the six unsigned Maven
   publications.
6. Smoke-test every Swift product and a combined base-plus-graph consumer,
   every Python artifact, all npm tarballs, and all JVM publications in fresh
   consumer environments. For Android, resolve and compile fresh Gradle
   consumers against each selected AAR and retain the package, ABI, and JNI
   inspection evidence; do not require device execution.
7. Assemble the closed release bundle with checksums, SBOM, and provenance.
8. Repeat from a second clean root and compare every byte.
9. Validate the bundle independently and complete the
   [release approval checklist](release-approval-checklist.md).

The manual `release-candidate.yml` workflow performs the build and validation
without publishing. It never invokes a physical-device command.

Android API 24+ arm64-v8a is an explicit v0.1.0 preview. The candidate must
retain cross-compilation, AAR assembly, closed-inventory, ABI/architecture,
JVM/JNI-contract, and fresh consumer dependency-resolution/compilation checks.
Live Android device model acquisition, inference, lifecycle, memory, thermal,
offline-restart, compatibility, and performance evidence remains unqualified
and is deferred until a device is available. Missing live-device evidence is
not a v0.1.0 publication blocker, and release material must not imply that
Android device inference passed or make production, performance, or device-
compatibility claims beyond the retained evidence.

## Node, browser, and Kotlin candidate construction

The Node assembler requires the approved npm names and an explicit
`--names-approved` assertion. It builds and inspects separate macOS arm64 base,
graph, and embedding tarballs, preserves the checked-in packages as private
repository placeholders, proves capability isolation, and emits SHA-256,
SHA-512, and package-integrity evidence:

```bash
python3 scripts/release/assemble_node_packages.py \
  --base-name @gungorbasa/retrievalkit \
  --graph-name @gungorbasa/retrievalkit-graph \
  --embedding-name @gungorbasa/retrievalkit-embedding \
  --names-approved \
  --version 0.1.0 \
  --output dist/release/node
```

The separate browser embedding assembler produces the approved Worker package
without a native addon or model artifact:

```bash
python3 scripts/release/assemble_browser_embedding_package.py \
  --name @gungorbasa/retrievalkit-browser-embedding \
  --name-approved \
  --version 0.1.0 \
  --output dist/release/browser-embedding
```

The browser retrieval build first produces and qualifies separate portable and
SIMD128 `wasm-bindgen` web artifacts. Its dedicated assembler then packages
those artifacts with the Worker wrapper and performs a fresh local-install
resolution smoke test:

```bash
scripts/check-browser-wasm.sh target/release-browser-wasm
python3 scripts/release/assemble_browser_package.py \
  --name @gungorbasa/retrievalkit-browser \
  --name-approved \
  --version 0.1.0 \
  --generated-root target/release-browser-wasm \
  --output dist/release/browser-retrieval
```

The Kotlin assembler uses the approved Maven group. It produces six isolated
publications—JVM/Android base, graph, and embedding—with POM, sources, Javadoc,
checksums, architecture validation, and a deterministic Central Portal bundle.
Central publication also requires namespace verification, detached PGP
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

## PyPI trusted publication

The PyPI job runs in the protected `pypi` environment with `id-token: write`
and no API token. It verifies the complete authorized bundle checksum set,
publishes the fifteen macOS arm64 CPython wheels, then verifies the public
registry records and retains publication evidence.

The owner completed the one-time PyPI bootstrap setup by 2026-08-01:

1. `retrievalkit`, `retrievalkit-graph`, and `retrievalkit-embedding` each
   received a non-SDK `0.0.0a0` placeholder;
2. each project trusts the public `gungorbasa/RetrievalKit` repository,
   `publish-release.yml` workflow, and `pypi` environment;
3. the temporary bootstrap publishers and one-time bootstrap workflows were
   removed; and
4. none of the projects contains v0.1.0 SDK artifacts.

PyPI requires different pending-publisher identities when multiple not-yet-
created project names are bootstrapped, so the projects temporarily used
separate bootstrap workflows. Once each project existed, it was configured
with the same protected production publisher. All three public records
resolved anonymously by 2026-08-01. The `retrievalkit-embedding` bootstrap ran
successfully as GitHub Actions run `30690365488`; its temporary publisher and
temporary `main` environment access were then removed, leaving the production
publisher and `v*` tag policy. Re-verify the records and exact publisher
settings before setting the required `pypi_trusted_publishers_ready` dispatch
input to true. The `0.0.0a0` artifacts reserve ownership only and must never be
described as usable SDK releases.

## npm trusted publication

The npm job runs in the protected `npm` environment with `id-token: write` and
no npm token. It installs the pinned OIDC-capable npm CLI, verifies the complete
authorized bundle checksum set, stages exactly the three native Node tarballs
and the browser retrieval and browser embedding tarballs, and publishes those
five artifacts with
`--provenance`. It then compares each registry `dist.integrity`
value with the authorized inventory, attests the tarballs/evidence, and retains
the publication record for 180 days.

npm trusted publishing cannot establish a package name that does not exist.
The owner completed all five packages' one-time bootstrap setup by 2026-08-01:

1. all five names received the non-release `0.0.0-bootstrap.0` placeholder;
2. each package trusts the public
   `gungorbasa/RetrievalKit` repository, `publish-release.yml` workflow, and
   `npm` environment;
3. the local bootstrap credential was removed; and
4. none of the packages contains v0.1.0 SDK artifacts.

All five public records resolved anonymously by 2026-08-01, including
`@gungorbasa/retrievalkit-browser` with the reviewed four-file placeholder and
exact trusted publisher. Re-verify all five records and exact trusted-publisher
settings before setting the required
`npm_trusted_publishers_ready` dispatch input to true. The pre-approval job then
verifies that all five public package records exist, and the npm job verifies
that `0.1.0` is unused. Missing bootstrap, missing OIDC trust, an existing
version, a changed tarball, or a registry integrity mismatch fails closed.
Because multiple npm uploads cannot be transactional, a failure after the
first succeeds requires an incident record and fix-forward release; published
npm versions are never overwritten.

## Maven Central publication

The Maven job runs in the protected `maven` environment. It verifies the
authorized bundle, requires group `io.github.gungorbasa`, copies the exact 24
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

The owner completed Central setup on 2026-07-26: `io.github.gungorbasa` is
verified, the public signing key is distributed, a six-month Portal user token
is installed as the two Central secrets, all three signing secrets are present,
and the protected `maven` environment accepts only `v*` tags. Rotate the Portal
token before 2027-01-26. The required `maven_central_ready` dispatch input is an
explicit re-verification assertion; missing secrets, signing failures,
namespace rejection, or upload failure stops publication.

The dedicated RetrievalKit release key is checked in as a public verification
artifact at
[`release/retrievalkit-release-signing-key.asc`](../../release/retrievalkit-release-signing-key.asc).
Its fingerprint is
`0E82 F1A5 487A 4EF3 CCF1 ED6C 3932 66CD 4DD1 58ED`; it expires on
2028-07-25. The private key is never stored in the repository. Its local
passphrase is retained in macOS Keychain under
`RetrievalKit-Maven-GPG`, and CI receives the key only through protected
environment secrets.

Before `git verify-tag`, the publication workflow reads the checked-in public
key through a fresh temporary GnuPG home, requires its full fingerprint to
match the release truth above, and imports only that key. This makes tag
verification deterministic on a clean GitHub-hosted runner instead of relying
on a pre-populated user keyring.

## Required external GitHub configuration

Before dispatching publication, complete the remaining items and re-verify the
already configured controls:

- the repository plan and visibility must support required reviewers for the
  `release` environment as described by
  [GitHub's deployment protection rules](https://docs.github.com/actions/reference/deployments-and-environments#deployment-protection-rules);
- re-verify the existing `release` environment owner-review rule and `v*` tag
  restriction;
- re-verify the protected `pypi` environment and all three projects' trusted
  publisher for this repository and workflow;
- re-verify the protected `npm` environment and all five packages'
  [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/);
- re-verify `io.github.gungorbasa` in Central Portal, the published PGP public
  key, and all five secrets in the protected `maven` environment using the
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
  consumer smoke tests pass, with Android limited to dependency resolution,
  compilation, package/ABI inspection, and other host-verifiable checks;
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
