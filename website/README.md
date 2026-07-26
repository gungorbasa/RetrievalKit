# RetrievalKit Docs

Public, searchable documentation for the RetrievalKit source preview. The site
publishes one end-to-end Python installation path for macOS Apple Silicon,
documents Swift, Node.js, Kotlin/JVM, and Android APIs, and keeps released
platform support separate from portability evidence.

## Prerequisites

- Node.js `>=22.13.0`
- npm

## Local Development

```bash
npm ci
npm run dev
npm test
npm run build
```

The site is built with [vinext](https://github.com/cloudflare/vinext) and hosted
with OpenAI Sites. Hosting identity is recorded in `.openai/hosting.json`.

## Source Preview Archive

The public download at
`public/downloads/retrievalkit-python-source-preview.tar.gz` is a deterministic
archive of a committed repository revision. It includes the Rust workspace,
graph-capable Python wrapper, release truth, legal files, and a dedicated
`SOURCE_PREVIEW.md` quickstart. Other language source paths remain available
from the linked public repository.

```bash
python3 scripts/release/build_source_preview.py --revision <full-commit-sha>
python3 scripts/release/build_source_preview.py --check
```

The builder atomically replaces the archive and updates `app/release.ts` with
the full source revision and SHA-256 digest. Check mode regenerates from that
revision and fails if the bytes, checksum, path safety, or required inventory
drift.
