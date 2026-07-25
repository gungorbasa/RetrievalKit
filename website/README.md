# RetrievalKit Docs

Public, searchable documentation for the RetrievalKit source preview. The site
publishes one end-to-end Python installation path for macOS Apple Silicon and
keeps the Node.js, Kotlin/JVM, Android, and Windows portability status explicit.

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
`public/downloads/retrievalkit-python-source-preview.tar.gz` is generated from a
committed repository revision:

```bash
git archive \
  --format=tar.gz \
  --prefix=retrievalkit-python-source-preview/ \
  --output=website/public/downloads/retrievalkit-python-source-preview.tar.gz \
  <revision> \
  Cargo.toml Cargo.lock LICENSE NOTICE THIRD_PARTY_NOTICES.md README.md \
  crates wrappers/python-graph \
  scripts/check-python-graph-wrapper.sh \
  scripts/preflight-python-wrapper.sh \
  benchmarks/graph-conformance/v1/fixture.json
```

After regenerating it, update `app/release.ts` with the source revision and
SHA-256 digest, then run `npm test`.
