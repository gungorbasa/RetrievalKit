# RetrievalKit documentation

> [RetrievalKit](../README.md) › Documentation

Use this directory to move from product-level guidance to implementation and
evidence. The product specification is authoritative; guides explain public
APIs; reports record completed qualification; research documents describe work
that is not part of the current product.

## Start here

| You want to… | Read |
| --- | --- |
| Integrate the SDK | [Swift](guides/swift.md), [Python](guides/python.md), [TypeScript/browser](guides/typescript.md), or [Kotlin/Android](guides/kotlin.md) guide |
| Understand the supported product | [Product specification](product/retrievalkit-product-spec.md) |
| Check compatibility and migration rules | [Compatibility policy](product/compatibility-policy.md) and [v0.1.0 migration](product/v0.1.0-migration.md) |
| Build or publish a release | [Release process](product/release-process.md) and [approval checklist](product/release-approval-checklist.md) |
| Review benchmark evidence | [Publication contract](product/benchmark-publication-contract-v1.md) and [reports](product/reports/) |
| Continue agent work | [Working memory](product/working-memory.md) |

## Active product direction

RetrievalKit V1 is a local-first SDK for indexes with fewer than 50K chunks.
Its primary engine is exact vector search, combined with BM25, query-time
hybrid ranking, metadata filters, graph traversal, graph-scoped retrieval,
transactional persistence on native targets, and idiomatic language wrappers.

HNSW, ANN, server mode, synchronization, and distributed database features are
outside V1 unless the product specification changes.

## Guides

- [Swift](guides/swift.md)
- [Python](guides/python.md)
- [TypeScript and browser](guides/typescript.md)
- [Kotlin/JVM and Android](guides/kotlin.md)

Each guide uses the same Project Apollo scenario so API choices can be compared
without changing the problem being solved.

## Product documents

- [Product specification](product/retrievalkit-product-spec.md) — supported
  architecture, behavior, and scope.
- [Capability-separated architecture](product/capability-separated-architecture.md)
  — ownership and parity across base, graph, and combined products.
- [Compatibility policy](product/compatibility-policy.md) — public API,
  persistence, and platform compatibility.
- [Release process](product/release-process.md) — guarded build, validation,
  authorization, and publication workflow.
- [Artifact retention policy](product/artifact-retention-policy.md) — durable
  and generated evidence boundaries.
- [Working memory](product/working-memory.md) — current project-scoped handoff
  context; not a substitute for the product specification.

## Evidence and reports

Completed qualification reports live under [`product/reports/`](product/reports/).
Start with:

- [v0.1.0 release candidate](product/reports/v0.1.0-release-candidate-report.md)
- [Cross-language wrapper parity](product/reports/cross-language-wrapper-parity-audit.md)
- [Retrieval quality V2](product/reports/retrieval-quality-v2-report.md)
- [SDK developer experience](product/reports/sdk-devex-live-audit-2026-07-24.md)
- [Browser portable baseline](product/reports/browser-wasm-portable-baseline-2026-07-26.md)

Benchmark claims must remain scoped to their frozen inputs, source revision,
device, and expiry. The root README is checked against the Phase 6 claim
register in CI.

## Research

Documents under [`research/`](research/) are exploratory and do not authorize
product behavior. In particular, the HNSW and TurboVec notes do not change the
exact-search-first V1 direction.

## Contributing to documentation

Update the product specification when supported behavior or scope changes.
Keep implementation reports factual and dated, keep runnable examples tied to
checked source, and move superseded handoff notes out of working memory.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for repository checks.
