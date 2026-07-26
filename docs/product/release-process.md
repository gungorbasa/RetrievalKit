# RetrievalKit v0.1.0 release process

Status: release-candidate and runtime authorization implementation complete;
external publication blocked on repository/environment configuration and the
remaining release gates.

The current automated release candidate ships the Swift and Python previews
from one signed source revision. Python retains separate base and graph native
aggregates because loading both into one process is unsupported. Swift publishes
one graph-capable aggregate containing both native capability surfaces.

Node and Kotlin package construction is also implemented, but those artifacts
are not yet part of the automated publication workflow. Their public identities
and registry ownership remain unresolved, and Maven additionally requires a
signing identity. They must not be described as registry-published until the
external prerequisites below are completed.

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

## Pending Node and Kotlin distributions

The Node assembler requires two owner-approved npm names and an explicit
`--names-approved` assertion. It builds and inspects separate macOS arm64 base
and graph tarballs, preserves the checked-in packages as private repository
placeholders, proves graph exclusion from the base artifact, and emits SHA-256,
SHA-512, and package-integrity evidence:

```bash
python3 scripts/release/assemble_node_packages.py \
  --base-name '<approved-base-name>' \
  --graph-name '<approved-graph-name>' \
  --names-approved \
  --version 0.1.0 \
  --output dist/release/node
```

The Kotlin assembler requires an owner-approved Maven group. It produces four
isolated publications—JVM base/graph and Android base/graph—with POM, sources,
Javadoc, checksums, architecture validation, and a deterministic Central Portal
bundle. Central publication also requires namespace verification, detached PGP
signatures, and a Portal token:

```bash
python3 scripts/release/assemble_kotlin_packages.py \
  --group '<approved-maven-group>' \
  --version 0.1.0 \
  --java-home "$JAVA_HOME" \
  --output dist/release/kotlin
```

These commands only construct candidate packages; they never reserve names,
upload artifacts, or publish. After the identities and registry accounts are
approved, add the exact outputs to the closed release bundle and protected
publication workflow before describing Node or Kotlin as part of v0.1.0.

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
attached to the GitHub Release alongside the package artifacts. PyPI
publication depends on successful completion of this protected job.

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
- confirm the workflow token can read Actions run metadata and the
  [`GET /repos/{owner}/{repo}/actions/runs/{run_id}/approvals` review-history endpoint](https://docs.github.com/rest/actions/workflow-runs#get-the-review-history-for-a-workflow-run);
- create and push the verified signed release tag. The publication workflow
  must be dispatched with that tag as its workflow ref, not merely supplied as
  the `tag` input.

Required environment reviewers are not available for private repositories on
all GitHub plans. The repository is currently private and its present plan does
not expose the required protection. An unprotected environment is not a
substitute: no approval event will exist and publication will fail closed.
Upgrade to a plan that supports required reviewers for private repositories or
make the repository public before attempting publication. No workflow or
repository setting is changed automatically.

Example dispatch after all external gates exist:

```bash
gh workflow run publish-release.yml \
  --ref v0.1.0 \
  -f tag=v0.1.0 \
  -f candidate_run_id=<candidate-run-id> \
  -f scheduled_run_id=<scheduled-phase7-run-id> \
  -f release_gate_run_id=<release-phase7-run-id>
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
