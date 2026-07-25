# Compatibility policy

RetrievalKit `0.x` releases are previews. Minor releases may refine APIs, but every
intentional source or persistence change must be documented in the changelog
and accompanied by migration guidance.

## Supported release surface

- Rust core and C ABI: exact behavior is revisioned with each release.
- Swift: Swift 6.2, iOS 15+, and macOS 14+ on the published arm64 Apple slices.
- Python: CPython 3.10–3.14 on macOS arm64 for the first preview.
- Persistence: V1–V4 base snapshots remain readable; new saves use the current
  checksummed format. Graph capability formats are validated independently.

Patch releases preserve documented public source compatibility unless a
correctness or security defect makes that unsafe. Deprecated APIs receive a
changelog entry before removal when practical. Major persistence migrations
must fail with actionable typed errors and preserve read/validate/migrate paths
for formats still listed as supported.

The base and graph native aggregates are alternatives, not co-linkable modules.
Swift distributes them through separate `RetrievalKit` and
`RetrievalKitGraph` package manifests so resolving one capability never
downloads the other native aggregate.
`retrievalkit` and `retrievalkit-graph` are likewise mutually exclusive inside one
Python process. This boundary is part of compatibility, not a temporary build
limitation.

Packed result layouts are an aggregate-level ABI contract. Native libraries,
headers, and wrappers must be upgraded together. The graph aggregate exposes
an explicit ABI version; the base aggregate is revisioned with the preview
release until a separate runtime version check is introduced.

The preview ABI uses `RetrievalKit` type names and `RETRIEVALKIT_` constants.
The pre-rename `Vk` and `VK_` spellings have no compatibility aliases. Native
integrators migrating from an earlier development artifact must update the
header, native library, and wrapper source as one unit; the public Swift and
Python APIs are unaffected by this internal boundary rename.

Kotlin, TypeScript, x86_64 Apple, Linux, and Windows have no compatibility
commitment until a release manifest lists them.
