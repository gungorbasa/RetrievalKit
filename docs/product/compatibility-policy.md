# Compatibility policy

RetrievalKit `0.x` releases are previews. Minor releases may refine APIs, but every
intentional source or persistence change must be documented in the changelog
and accompanied by migration guidance.

## Supported release surface

- Rust core and C ABI: exact behavior is revisioned with each release.
- Swift: Swift 6.2, iOS 15+, and macOS 14+ on the published arm64 Apple slices.
- Python: CPython 3.10–3.14 on macOS arm64 for the first preview, including the
  optional `retrievalkit-embedding` distribution.
- TypeScript: Node.js LTS on macOS arm64 for the first preview. The separate
  `@gungorbasa/retrievalkit-embedding` native package and
  `@gungorbasa/retrievalkit-browser-embedding` Worker package are included.
  The browser retrieval package remains unpublished and outside this release
  compatibility surface.
- Kotlin: Kotlin/JVM with Android arm64-v8a native packaging for the first
  preview, including the optional `retrievalkit-embedding` JVM and
  `retrievalkit-embedding-android` artifacts. Kotlin Multiplatform is not
  supported.
- Persistence: V1–V4 base snapshots remain readable; new saves use the current
  checksummed V4 format. Graph capability formats are validated independently.

Patch releases preserve documented public source compatibility unless a
correctness or security defect makes that unsafe. Deprecated APIs receive a
changelog entry before removal when practical. Major persistence migrations
must fail with actionable typed errors and preserve read/validate/migrate paths
for formats still listed as supported.

The Rust base and graph native aggregates remain alternatives, not co-linkable
modules. The public Swift package deliberately distributes only the
graph-capable aggregate and exposes `RetrievalKit` and `RetrievalKitGraph` as
separately selectable Swift products over it. Applications may select either
product or both without loading competing native libraries. A base-only Swift
consumer still downloads the shared graph-capable binary; graph APIs are not
part of its selected Swift target.

`retrievalkit` and `retrievalkit-graph` remain mutually exclusive inside one
Python process. `@gungorbasa/retrievalkit` and
`@gungorbasa/retrievalkit-graph` use the same alternative-aggregate rule in
Node, as do the Kotlin base and graph-capable packages. Their native aggregate
boundary is part of compatibility, not a temporary build limitation.

Packed result layouts are an aggregate-level ABI contract. Native libraries,
headers, and wrappers must be upgraded together. The graph aggregate exposes
an explicit ABI version; the base aggregate is revisioned with the preview
release until a separate runtime version check is introduced.

The preview ABI uses `RetrievalKit` type names and `RETRIEVALKIT_` constants.
The pre-rename `Vk` and `VK_` spellings have no compatibility aliases. Native
integrators migrating from an earlier development artifact must update the
header, native library, and wrapper source as one unit; the public Swift and
Python APIs are unaffected by this internal boundary rename.

The TypeScript npm names are fixed under the owner's `@gungorbasa` scope, and
Kotlin Maven coordinates are fixed under `io.github.gungorbasa`. Registry
publication is not claimed until the release gates pass. x86_64 Apple, Linux
desktop/server, Windows, the unpublished browser/WASM target, and Kotlin
Multiplatform have no compatibility commitment until a release manifest lists
them.
