# RetrievalKit Live Developer-Experience Audit

Date: 2026-07-24
Branch: `main`
Revision: `1da2791`
Status: `DONE_WITH_CONCERNS`

## Scope

This audit tests the SDK that exists in the repository today. It does not award
credit for the proposed progressive `Document` / `Record` API until that API is
implemented, documented, and exercised by the checked-in quickstarts.

The target developer is a startup application engineer or researcher replacing
a hosted retrieval service with private, local Swift or Python retrieval.

## Report Card

| Dimension | Score | Method | Evidence |
|---|---:|---|---|
| Getting started | 3/10 | Tested | The unauthenticated GitHub repository and the `v0.1.0` Swift binary URL both returned HTTP 404. With repository access and prerequisites already installed, the Swift graph build plus first result took 10.32 seconds and the Python graph check plus example took 5.21 seconds. |
| API / SDK ergonomics | 4/10 | Tested + source inspection | The live Swift retrieval quickstart has 51 nonblank lines, graph-only 57, combined 88, and Python combined 100. Common ingestion still requires `RecordInput`, `Chunk(key:)`, dimensions, and keyed embedding maps. Compatibility and capability APIs coexist, and graph/base packages duplicate public value types and search names. |
| Error messages | 6/10 | Tested | Missing embeddings raise a typed `MissingEmbeddingError` naming `body`; dimension mismatch reports expected `2` and actual `1`. Errors do not explain the correction or link to documentation, and Python exposes a long internal stack before the useful line. |
| Documentation | 5/10 | Tested + source inspection | Local Swift and Python guides are organized around one scenario, root README links resolve locally, and checked-in quickstarts run. There is no public or hosted documentation, search, or language-switching surface. `CHANGELOG.md` still claims an RRF default while the current product docs teach query-time weighted `alpha`; `CONTRIBUTING.md` incorrectly says no root license exists. |
| Upgrade path | 6/10 | Source inspection | A changelog, compatibility policy, and `v0.1.0` migration guide exist and persistence migration is explained. The stale hybrid-default statement weakens trust, and there is no automated source migration for the upcoming clean API break. |
| Developer environment | 7/10 | Tested + source inspection | Rust, Swift, Python, release, and platform workflows exist; the repository has 287 Rust tests, 56 Swift tests, and 29 Python tests. Warm local checks are fast. `verify-swift-wrapper.sh --help` unexpectedly started a full Apple build, while `build-xcframework.sh --help` behaved correctly. Public binary resolution is blocked. |
| Community and ecosystem | 2/10 | Tested | GitHub reports the repository as private, Discussions are disabled, the public page is a 404, there are no issues, and no homepage or community channel is configured. Bug, feature, and private-security templates are well structured but unavailable to outside developers. |
| DX measurement | 3/10 | Source inspection | Issue templates collect reproducible environment data, but there is no onboarding/TTHW instrumentation, docs analytics, feedback widget, activation measure, or recurring developer-satisfaction loop. |
| **Overall** | **5/10** | | **The local engineering foundation is good; outside adoption is currently blocked and the live API still exposes internal schema ceremony.** |

## Time to Hello World

### Public developer

| Step | Result | Friction |
|---|---|---|
| Open `https://github.com/gungorbasa/RetrievalKit` | HTTP 404 | Blocking |
| Resolve the Swift binary declared by the root package | HTTP 404 | Blocking |
| Install and run a public package | Not possible | Blocking |

Public TTHW is therefore **blocked**, not merely slow.

### Developer with repository access and installed toolchains

| Path | Measured work | Time | Result |
|---|---|---:|---|
| Swift graph + retrieval | Build the local macOS graph XCFramework, compile the package, run the quickstart | 10.32 s | `graph-hybrid=decision-swift` |
| Python graph + retrieval | Run Rust/PyO3 checks, Ruff, strict mypy, four tests, then the example | 5.21 s | `graph-hybrid=decision-swift` |

These are warm local measurements on the audit machine. They exclude clone,
Rust/Xcode/Python installation, model download, and embedding generation.

## Highest-Impact Findings

1. **Public onboarding is impossible.** The private repository, absent hosted
   docs, and unavailable release assets make every other onboarding improvement
   invisible to an outside developer.
2. **The live API does not match the approved simple design.** A developer still
   maintains a manual join between chunk keys and embeddings and must understand
   dimensions, record/chunk ownership, capability choices, and multiple search
   verbs before the first useful result.
3. **Documentation contradicts itself.** RRF versus weighted `alpha` and the
   stale licensing statement are credibility failures, not cosmetic copy bugs.
4. **Tooling is strong but uneven.** Build and verification automation is broad,
   yet a help request can start a full Apple build and public SwiftPM resolution
   cannot succeed.
5. **Errors identify the failure but rarely teach the fix.** Typed exceptions
   and actual values are good; correction guidance and stable documentation
   links are missing.
6. **There is no adoption feedback loop.** The project measures engine quality,
   speed, memory, release evidence, and correctness extensively, but not whether
   a new developer can discover, install, understand, and retain the SDK.

## Plan Versus Reality

The 2026-07-11 plan review targeted overall DX `9/10`, Python TTHW under two
minutes, and Swift TTHW under five minutes. Local source execution is well
inside those time targets once prerequisites and repository access exist.
Public TTHW remains blocked, and overall live DX is `5/10`.

| Measure | Plan | Live | Delta |
|---|---:|---:|---:|
| Overall DX | 9/10 | 5/10 | -4 |
| Python TTHW | <2 min | 5.21 s local; public blocked | Local pass, public fail |
| Swift TTHW | <5 min | 10.32 s local; public blocked | Local pass, public fail |

The boomerang result is a miss: the plan accurately described the intended
product, but distribution and the simplified public API have not shipped.

## Recommended Order

1. Implement the approved capability-specific progressive API and replace all
   canonical quickstarts with retrieval-only, graph-only, and combined examples.
2. Make one public installation path work end to end: publish the repository or
   docs, provide valid Swift artifacts and Python wheels, and test from a clean
   unauthenticated consumer.
3. Reconcile the changelog, migration guide, compatibility policy,
   contributing guide, and current hybrid semantics in one documentation pass.
4. Add consistent `--help` and unknown-argument handling to every developer
   script; help must never start a build.
5. Upgrade common errors to: problem, actual value, expected value, exact fix,
   and stable documentation link.
6. Enable a public feedback path and record clean-machine TTHW for each released
   language/package in CI or release qualification.
7. Re-run this live audit from an unauthenticated clean environment after the
   SDK API and distribution work land.

## What 10/10 Looks Like

A Swift developer adds one package dependency, copies a retrieval-only example
under 20 lines, supplies an embedding, and sees a ranked result in under two
minutes without cloning this repository or building Rust. The same concepts
expand to graph-only and combined retrieval without changing identity models.
Errors show the correction inline, docs are public and searchable, upgrades are
boring, and onboarding success is measured continuously.
