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
    eyebrow: "Release readiness",
    title: "Public installs are pending; source paths are available",
    summary:
      "SwiftPM, PyPI, npm, and Maven publication have not happened. The commands below show the intended install shape, not packages that are available today.",
    body:
      "Python and Swift have intended package names. npm names and Maven coordinates still require approval, so their placeholders must not be pasted literally. Until publication, use the repository source quickstarts; the Python graph source bundle is the only public download.",
    code: `# PENDING — not published
python -m pip install retrievalkit-graph
npm install <approved-retrievalkit-graph-package>

// Package.swift — PENDING
.package(
  url: "https://github.com/gungorbasa/RetrievalKit.git",
  from: "0.1.0"
)

// build.gradle.kts — PENDING
implementation(
  "<approved-group>:retrievalkit-graph:0.1.0"
)`,
    tags: [
      "install",
      "publication",
      "pending",
      "swiftpm",
      "pypi",
      "npm",
      "maven",
      "source",
    ],
  },
  {
    id: "python",
    eyebrow: "Python",
    title: "Progressive builders, Rust-owned retrieval",
    summary:
      "Pass ordinary records and direct embeddings. Rust infers dimensions and owns identity, filtering, ranking, traces, and persistence.",
    body:
      "PyPI publication is pending. Today, download the macOS arm64 graph source preview or build from a repository checkout. After publication, choose retrievalkit-graph when relationships should constrain search and retrievalkit for a flat corpus. Install exactly one distribution per process.",
    code: `from retrievalkit_graph import (
    GraphRecordNode,
    GraphRetrievalDatabaseBuilder,
    GraphSchema,
)

schema = GraphSchema(
    record_nodes=[GraphRecordNode("Note", "Note", ["title"])]
)
builder = GraphRetrievalDatabaseBuilder(
    corpus_id="apollo",
    graph=schema,
    encoding="f32",
)
builder.upsert(
    {
        "id": "decision-swift",
        "record_type": "Note",
        "fields": {"title": "Apple client decision"},
        "content": "Apollo chose Swift for its Apple client.",
    },
    embedding=[1.0, 0.0],
)
database = builder.build()

hits = database.retrieval.hybrid_search(
    "Why did we choose Swift?",
    [1.0, 0.0],
    alpha=0.6,
    limit=1,
)
print(hits[0]["document_id"])  # decision-swift`,
    tags: ["python", "api", "graph", "hybrid", "builder", "self-contained"],
  },
  {
    id: "swift",
    eyebrow: "Swift / Apple platforms",
    title: "One package, selectable retrieval and graph products",
    summary:
      "Choose RetrievalKit for local search or RetrievalKitGraph for graph traversal and scoped retrieval. Both products share one graph-capable native artifact.",
    body:
      "SwiftPM publication is pending because the repository, version tag, and release XCFramework are not public. The source preview supports macOS 14+ arm64 and iOS 15+ arm64 devices and Apple-silicon simulators. Build the XCFramework before running source examples.",
    code: `import RetrievalKit

@main
struct ApolloSearch {
  static func main() async throws {
    let builder = try RetrievalDatabase.Builder(
      corpusID: "apollo",
      encoding: .f32
    )
    try await builder.upsert(
      Document(
        id: "decision-swift",
        text: "Apollo chose Swift for its Apple client."
      ),
      embedding: [1, 0]
    )
    let database = try await builder.build()
    let hits = try await database.search(
      text: "Why did we choose Swift?",
      embedding: [1, 0],
      alpha: 0.6,
      limit: 1
    )
    print(hits[0].documentID)
  }
}

// Build first:
// scripts/build-xcframework.sh --macos-only`,
    tags: [
      "swift",
      "ios",
      "macos",
      "apple",
      "swiftpm",
      "api",
      "graph",
      "async",
    ],
  },
  {
    id: "node",
    eyebrow: "TypeScript / Node.js",
    title: "Typed async APIs for Node.js LTS",
    summary:
      "Promise-based N-API calls keep native work off the event loop and preserve Float32Array, bigint, and typed graph values.",
    body:
      "npm names are not approved and no package is published. Use the repository source build on macOS arm64 with Node.js 22.13+ LTS or Node.js 24 LTS. Browser, WebAssembly, Windows, and Linux builds are not claimed. Base and graph packages are mutually exclusive in one process.",
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
});
console.log(hits[0]?.documentId);`,
    tags: ["typescript", "node", "napi", "api", "async"],
  },
  {
    id: "kotlin",
    eyebrow: "Kotlin / Android",
    title: "Blocking, typed JNI with deterministic lifetime",
    summary:
      "Kotlin uses FloatArray, sealed value types, typed exceptions, and AutoCloseable resources over the shared Rust core.",
    body:
      "Maven coordinates are not approved and no artifact is published. The source build uses JDK 17 and targets a macOS arm64 JVM native library; compiled bytecode can run on Java 11+. Android targets API 24+ and arm64-v8a. Other desktop targets and Android ABIs are not claimed.",
    code: `import ai.retrievalkit.Document
import ai.retrievalkit.RetrievalDatabase
import ai.retrievalkit.VectorEncoding

fun main() {
    RetrievalDatabase.Builder(
        "apollo",
        encoding = VectorEncoding.F32,
    ).use { builder ->
        builder.upsert(
            Document("decision-swift", "Apollo chose Swift."),
            floatArrayOf(1f, 0f),
        )
        builder.build().use { database ->
            val hits = database.search(
                text = "Why Swift?",
                embedding = floatArrayOf(1f, 0f),
                alpha = 0.6f,
                limit = 1,
            )
            println(hits.first().documentId)
        }
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
      "Swift source qualification covers arm64 macOS and iOS through one XCFramework. Python source portability is checked on Ubuntu and Windows, while the initial release wheel target remains macOS arm64. Node is macOS arm64. Kotlin/JVM is qualified on macOS with JDK 17, and Android packages arm64-v8a. Other targets remain unclaimed until their full package and consumer matrices pass.",
    tags: ["platforms", "windows", "linux", "macos", "android", "support"],
  },
];

const platforms = [
  ["Swift", "macOS 14+ arm64", "Source-qualified; release pending"],
  ["Swift", "iOS 15+ arm64 device / simulator", "Source-qualified; release pending"],
  ["Python", "macOS arm64 / CPython 3.10–3.14", "Initial wheel target; unpublished"],
  ["Python", "Ubuntu / Windows", "Portability CI only"],
  ["Node.js", "macOS arm64 / Node 22.13+ or 24 LTS", "Source-qualified; unpublished"],
  ["Kotlin/JVM", "macOS arm64 / JDK 17 build", "Source-qualified; unpublished"],
  ["Android", "API 24+ / arm64-v8a", "Source-qualified; unpublished"],
];

const releaseReadiness = [
  [
    "Swift",
    "One package: RetrievalKit and RetrievalKitGraph products",
    "Pending public repository, v0.1.0 tag, and XCFramework release",
    "Authorized repository source checkout",
  ],
  [
    "Python",
    "retrievalkit or retrievalkit-graph",
    "Pending PyPI publication",
    "Public graph source bundle and authorized checkout",
  ],
  [
    "Node.js",
    "Choose one base or graph package; names unapproved",
    "Pending npm name approval and publication",
    "Authorized repository source checkout",
  ],
  [
    "Kotlin",
    "Choose one JVM/Android base or graph artifact; group unapproved",
    "Pending Maven coordinates and publication",
    "Authorized repository source checkout",
  ],
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
              section.code ?? "",
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
          <a href="#release-readiness">Release status</a>
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
            v0.1.0 publication pending
          </div>
          <p className="kicker">Fast, private retrieval for edge AI</p>
          <h1>Search locally.<br />Keep the evidence.</h1>
          <p className="hero-lede">
            Exact vectors, BM25 keyword evidence, metadata filters, and
            relationship-scoped retrieval. One Rust core with native Python,
            Swift, Kotlin, and Node.js APIs.
          </p>
          <div className="hero-actions">
            <a className="primary-button" href="#release-readiness">
              Check install status
            </a>
            <a className="secondary-button" href="#languages">
              Explore the APIs
            </a>
          </div>
          <p className="release-meta">
            Python preview revision <code>{release.sourceRevision}</code> · SHA-256{" "}
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

      <section className="release-section" id="release-readiness">
        <div className="release-heading">
          <p className="kicker">Release readiness</p>
          <h2>Source-qualified does not mean registry-published.</h2>
          <p>
            No SwiftPM, PyPI, npm, or Maven release is live. The shortest
            eventual commands are documented in the language sections below,
            while the available route remains source.
          </p>
        </div>
        <div
          className="release-table"
          role="table"
          aria-label="Package release readiness"
        >
          <div className="release-row release-header" role="row">
            <strong role="columnheader">SDK</strong>
            <span role="columnheader">Select</span>
            <span role="columnheader">Publication</span>
            <span role="columnheader">Available now</span>
          </div>
          {releaseReadiness.map(([sdk, selection, status, available]) => (
            <div className="release-row" role="row" key={sdk}>
              <strong role="cell">{sdk}</strong>
              <span role="cell">{selection}</span>
              <small role="cell">{status}</small>
              <span role="cell">{available}</span>
            </div>
          ))}
        </div>
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
