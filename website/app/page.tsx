"use client";

import { useMemo, useState } from "react";
import { release } from "./release";

type DocSection = {
  id: string;
  eyebrow: string;
  title: string;
  summary: string;
  body: string;
  code?: string;
  tags: string[];
};

const sections: DocSection[] = [
  {
    id: "install",
    eyebrow: "Golden path",
    title: "Install the Python source preview",
    summary:
      "The first public path is intentionally narrow: macOS arm64, Python 3.10–3.14, and a local Rust build.",
    body:
      "Download the versioned source bundle, extract it, run the checked-in validation script, then execute the Project Apollo example. The script creates an isolated environment, builds the native wheel, checks typing and lint, runs the tests, and verifies an installed wheel.",
    code: `# First use the Download preview link on this page.
ARCHIVE="$HOME/Downloads/${release.archiveName}"
tar -xzf "$ARCHIVE"
cd ${release.directoryName}

PYTHON_BIN=python3 scripts/check-python-graph-wrapper.sh
target/python-graph-wrapper-check-venv-py*/bin/python \\
  wrappers/python-graph/examples/graph_retrieval_quickstart.py

# expected
# graph-hybrid=decision-swift`,
    tags: ["python", "install", "quickstart", "macos", "source"],
  },
  {
    id: "python",
    eyebrow: "Python",
    title: "Progressive builders, Rust-owned retrieval",
    summary:
      "Pass ordinary records and direct embeddings. Rust infers dimensions and owns identity, filtering, ranking, traces, and persistence.",
    body:
      "Choose retrievalkit-graph when relationships should constrain search. Choose retrievalkit for a flat corpus. The graph aggregate already contains retrieval, so install or load exactly one distribution per process.",
    code: `from retrievalkit_graph import GraphRetrievalDatabaseBuilder

builder = GraphRetrievalDatabaseBuilder(
    corpus_id="apollo",
    graph=schema,
)
builder.upsert(
    record,
    embedding=[1.0, 0.0, 0.0],
)
database = builder.build()

hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0, 0.0],
    alpha=0.6,
)`,
    tags: ["python", "api", "graph", "hybrid", "builder"],
  },
  {
    id: "node",
    eyebrow: "TypeScript / Node.js",
    title: "Typed async APIs for Node.js LTS",
    summary:
      "Promise-based N-API calls keep native work off the event loop and preserve Float32Array, bigint, and typed graph values.",
    body:
      "The repository-local preview currently targets Node.js 20+ on macOS arm64. Browser and WebAssembly builds are not part of this target. The base and graph packages are mutually exclusive in one process.",
    code: `import { RetrievalDatabaseBuilder }
  from "retrievalkit-node-local";

const builder = new RetrievalDatabaseBuilder({
  corpusId: "apollo"
});
await builder.add([{
  id: "decision-swift",
  text: "Apollo chose Swift.",
  embedding: new Float32Array([1, 0, 0])
}]);

await using database = await builder.build();
const hits = await database.search({
  mode: "hybrid",
  text: "Why Swift?",
  embedding: new Float32Array([1, 0, 0]),
  alpha: 0.6
});`,
    tags: ["typescript", "node", "napi", "api", "async"],
  },
  {
    id: "kotlin",
    eyebrow: "Kotlin / Android",
    title: "Blocking, typed JNI with deterministic lifetime",
    summary:
      "Kotlin uses FloatArray, sealed value types, typed exceptions, and AutoCloseable resources over the shared Rust core.",
    body:
      "Use JDK 17 for the build. Run disk, build, and search work on an application-selected background dispatcher on Android. The current Android artifact targets API 24+ and arm64-v8a.",
    code: `RetrievalDatabase.Builder("apollo").use { builder ->
    builder.upsert(
        Document("decision-swift", "Apollo chose Swift."),
        floatArrayOf(1f, 0f, 0f),
    )
    builder.build().use { database ->
        val hits = database.search(
            text = "Why Swift?",
            embedding = floatArrayOf(1f, 0f, 0f),
            alpha = 0.6f,
        )
    }
}`,
    tags: ["kotlin", "android", "jni", "api", "jdk17"],
  },
  {
    id: "errors",
    eyebrow: "Debugging",
    title: "Errors explain the correction",
    summary:
      "Stable language-specific exception types retain actionable messages from the Rust core.",
    body:
      "A query with the wrong embedding dimension identifies the expected and actual values and tells the caller to use the same embedding model. Invalid alpha values explain the allowed range and the vector-only and BM25-only endpoints.",
    code: `invalid vector dimension: expected 384, got 768;
use the same embedding model for indexing and queries

invalid query parameter 'alpha':
alpha must be finite and between 0 and 1;
use 1 for vector-only or 0 for BM25-only`,
    tags: ["errors", "debugging", "dimension", "alpha"],
  },
  {
    id: "platforms",
    eyebrow: "Compatibility",
    title: "Know what is qualified today",
    summary:
      "Portability checks and released-platform support are tracked separately so CI evidence never becomes an accidental product promise.",
    body:
      "Python source portability is checked on Ubuntu and Windows, while the initial release wheel target remains macOS arm64. Node is macOS arm64. Kotlin/JVM is qualified on macOS with JDK 17, and Android packages arm64-v8a. Other targets remain unclaimed until their full package and consumer matrices pass.",
    tags: ["platforms", "windows", "linux", "macos", "android", "support"],
  },
];

const platforms = [
  ["Python", "macOS arm64", "Release target"],
  ["Python", "Ubuntu / Windows", "Portability CI"],
  ["Node.js", "macOS arm64", "Repository preview"],
  ["Kotlin/JVM", "macOS arm64 + JDK 17", "Repository preview"],
  ["Android", "API 24+ / arm64-v8a", "Repository preview"],
];

export default function Home() {
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  const filtered = useMemo(
    () =>
      normalizedQuery
        ? sections.filter((section) =>
            [
              section.eyebrow,
              section.title,
              section.summary,
              section.body,
              ...section.tags,
            ]
              .join(" ")
              .toLowerCase()
              .includes(normalizedQuery),
          )
        : sections,
    [normalizedQuery],
  );

  return (
    <main>
      <header className="site-header">
        <a className="brand" href="#top" aria-label="RetrievalKit documentation">
          <span className="brand-mark">RK</span>
          <span>RetrievalKit</span>
        </a>
        <nav aria-label="Primary navigation">
          <a href="#install">Install</a>
          <a href="#languages">Languages</a>
          <a href="#platform-matrix">Platforms</a>
        </nav>
        <a className="header-cta" href={release.archiveUrl}>
          Download preview
        </a>
      </header>

      <section className="hero" id="top">
        <div className="hero-copy">
          <div className="status-pill">
            <span />
            v0.1.0 source preview
          </div>
          <p className="kicker">Fast, private retrieval for edge AI</p>
          <h1>Search locally.<br />Keep the evidence.</h1>
          <p className="hero-lede">
            Exact vectors, BM25 keyword evidence, metadata filters, and
            relationship-scoped retrieval. One Rust core with native Python,
            Swift, Kotlin, and Node.js APIs.
          </p>
          <div className="hero-actions">
            <a className="primary-button" href="#install">
              Run the Python quickstart
            </a>
            <a className="secondary-button" href="#languages">
              Explore the APIs
            </a>
          </div>
          <p className="release-meta">
            Source revision <code>{release.sourceRevision}</code> · SHA-256{" "}
            <code>{release.archiveSha256}</code>
          </p>
        </div>
        <div className="result-card" aria-label="Example retrieval trace">
          <div className="terminal-bar">
            <span />
            <span />
            <span />
            <b>apollo.search</b>
          </div>
          <div className="query-block">
            <small>QUERY</small>
            <p>Why did we choose Swift?</p>
          </div>
          <div className="trace-step">
            <span>01</span>
            <div>
              <b>Scope</b>
              <p>Project Apollo · approved notes</p>
            </div>
          </div>
          <div className="trace-step">
            <span>02</span>
            <div>
              <b>Rank</b>
              <p>semantic 0.6 · BM25 0.4</p>
            </div>
          </div>
          <div className="hit">
            <div>
              <small>TOP HIT</small>
              <strong>decision-swift</strong>
            </div>
            <b>0.96</b>
          </div>
        </div>
      </section>

      <section className="proof-strip" aria-label="Product qualities">
        <div><strong>Local-first</strong><span>No retrieval server</span></div>
        <div><strong>Explainable</strong><span>Scores and traces</span></div>
        <div><strong>Deterministic</strong><span>Stable exact search</span></div>
        <div><strong>Native</strong><span>Rust-owned hot path</span></div>
      </section>

      <section className="docs-shell" id="languages">
        <aside>
          <p className="aside-label">Documentation</p>
          <label className="search-box">
            <span aria-hidden="true">⌕</span>
            <span className="sr-only">Search documentation</span>
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search APIs, platforms…"
            />
          </label>
          <div className="side-links">
            {sections.map((section) => (
              <a key={section.id} href={`#${section.id}`}>
                <span>{section.eyebrow}</span>
                {section.title}
              </a>
            ))}
          </div>
        </aside>

        <div className="docs-content">
          <div className="section-heading">
            <div>
              <p className="kicker">From first result to integration</p>
              <h2>One mental model, native in every language.</h2>
            </div>
            <p>{filtered.length} of {sections.length} sections</p>
          </div>

          {filtered.length ? (
            filtered.map((section) => (
              <article id={section.id} key={section.id} className="doc-card">
                <div className="doc-copy">
                  <p className="eyebrow">{section.eyebrow}</p>
                  <h3>{section.title}</h3>
                  <p className="summary">{section.summary}</p>
                  <p>{section.body}</p>
                  {section.id === "install" && (
                    <a className="text-link" href={release.archiveUrl}>
                      Download {release.archiveName} →
                    </a>
                  )}
                </div>
                {section.code && (
                  <pre><code>{section.code}</code></pre>
                )}
              </article>
            ))
          ) : (
            <div className="empty-state">
              <p>No documentation matches “{query}”.</p>
              <button type="button" onClick={() => setQuery("")}>Clear search</button>
            </div>
          )}
        </div>
      </section>

      <section className="platform-section" id="platform-matrix">
        <div>
          <p className="kicker">Platform truth, not platform theater</p>
          <h2>Qualified targets stay narrow until the evidence expands.</h2>
          <p>
            CI portability is useful evidence, but it is not a release claim.
            Every public target must pass package construction, installed-consumer
            smoke tests, lifecycle tests, and artifact inspection first.
          </p>
        </div>
        <div className="platform-table" role="table" aria-label="Platform support">
          {platforms.map(([sdk, platform, status]) => (
            <div className="platform-row" role="row" key={`${sdk}-${platform}`}>
              <strong role="cell">{sdk}</strong>
              <span role="cell">{platform}</span>
              <small role="cell">{status}</small>
            </div>
          ))}
        </div>
      </section>

      <footer>
        <div>
          <span className="brand-mark">RK</span>
          <p>RetrievalKit v0.1.0 source preview</p>
        </div>
        <p>Apache-2.0 · Local retrieval for fewer than 50K chunks</p>
      </footer>
    </main>
  );
}
