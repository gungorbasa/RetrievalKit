import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";
import { matchesDocumentationSection } from "../app/search.js";

const templateRoot = new URL("../", import.meta.url);

async function render(pathname = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request(new URL(pathname, "http://localhost"), {
      headers: { accept: "text/html" },
    }),
    {
      ASSETS: {
        fetch: async () => new Response("Not found", { status: 404 }),
      },
    },
    {
      waitUntil() {},
      passThroughOnException() {},
    },
  );
}

test("server-renders the public RetrievalKit documentation", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>RetrievalKit Docs · Local retrieval SDK<\/title>/i);
  assert.match(html, /Search locally\./);
  assert.match(html, /Public installs are pending; source paths are available/);
  assert.match(html, /Source-qualified does not mean registry-published/);
  assert.match(html, /Package release readiness/);
  assert.match(html, /No v0\.1\.0 SwiftPM, PyPI, npm, or Maven release is live/);
  assert.match(html, /bootstrap-only placeholders/);
  assert.match(html, /npm install @gungorbasa\/retrievalkit-graph/);
  assert.match(
    html,
    /io\.github\.gungorbasa:retrievalkit-graph:0\.1\.0/,
  );
  assert.match(
    html,
    /Central namespace and protected credentials configured; v0\.1\.0 unpublished/,
  );
  assert.match(
    html,
    /Scoped names reserved; trusted publishing configured; v0\.1\.0 unpublished/,
  );
  assert.match(
    html,
    /Names reserved; trusted publishing configured; v0\.1\.0 unpublished/,
  );
  assert.match(html, /0\.0\.0a0 non-SDK placeholders/);
  assert.match(html, /Public source; pending v0\.1\.0 tag and XCFramework release/);
  assert.match(html, /Public repository source checkout/);
  assert.match(html, /Public graph source bundle and repository checkout/);
  assert.match(
    html,
    /href="https:\/\/github\.com\/gungorbasa\/RetrievalKit"/,
  );
  assert.match(html, /View source on GitHub/);
  assert.match(html, /Swift, Kotlin, and Node\.js APIs/);
  assert.match(html, /Swift \/ Apple platforms/);
  assert.match(html, /One package, selectable retrieval and graph products/);
  assert.match(html, /macOS 14\+ arm64/);
  assert.match(html, /iOS 15\+ arm64 device \/ simulator/);
  assert.match(html, /TypeScript \/ Node\.js/);
  assert.match(html, /Node\.js 22\.13\+ LTS or Node\.js 24 LTS/);
  assert.match(html, /Kotlin \/ Android/);
  assert.match(html, /schema = GraphSchema/);
  assert.match(html, /GraphRelationship/);
  assert.match(html, /database\.graph\.query/);
  assert.match(html, /GraphTraversal\(&quot;contains&quot;\)/);
  assert.match(html, /within=selection/);
  assert.match(html, /import ai\.retrievalkit\.Document/);
  assert.match(
    html,
    /git clone https:\/\/github\.com\/gungorbasa\/RetrievalKit\.git/,
  );
  assert.match(html, /scripts\/run-swift-quickstart\.sh base-retrieval/);
  assert.match(html, /scripts\/check-python-graph-wrapper\.sh/);
  assert.match(html, /node base\/examples\/retrieval\.mjs/);
  assert.match(html, /\.\/gradlew :example-retrieval:run/);
  assert.match(html, /Expected:/);
  assert.match(html, /graph-hybrid=decision-swift/);
  assert.match(html, /Python preview revision/);
  assert.match(html, /Search documentation/);
  assert.match(html, /retrievalkit-python-source-preview\.tar\.gz/);
  assert.doesNotMatch(html, /\/Users\/|\.vinext\/fonts|file:/);
  assert.doesNotMatch(
    html,
    /<link[^>]+rel="preload"[^>]+as="font"/i,
  );
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape/);
});

test("documentation search matches release-audit queries by term", async () => {
  const page = await readFile(new URL("../app/page.tsx", import.meta.url), "utf8");
  for (const phrase of [
    "hybrid alpha",
    "graph scoped search",
    "embedding dimension",
  ]) {
    assert.match(page.toLowerCase(), new RegExp(phrase));
  }

  const python = {
    eyebrow: "Python",
    title: "Scoped retrieval",
    summary: "Graph scoped search",
    body: "Tune hybrid ranking with query-time alpha.",
    tags: ["python"],
  };
  const errors = {
    eyebrow: "Debugging",
    title: "Embedding errors",
    summary: "Correct the query",
    body: "An embedding dimension mismatch reports expected and actual values.",
    tags: ["errors"],
  };

  assert.equal(matchesDocumentationSection(python, "hybrid alpha"), true);
  assert.equal(matchesDocumentationSection(python, "graph scoped search"), true);
  assert.equal(matchesDocumentationSection(errors, "embedding dimension"), true);
  assert.equal(matchesDocumentationSection(python, "android jni"), false);
});

test("server-renders a useful custom not-found page", async () => {
  const response = await render("/missing-documentation-route");
  assert.equal(response.status, 404);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /This route is not in the corpus\./);
  assert.match(html, /Back to documentation/);
  assert.match(html, /Browse SDK guides/);
  assert.match(html, /href="\/#languages"/);
  assert.doesNotMatch(html, /^Not found$/i);
});

test("removes starter-only files and dependencies", async () => {
  const [page, layout, notFound, packageJson] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/not-found.tsx", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
  ]);

  assert.match(page, /RetrievalKit/);
  assert.match(page, /type="search"/);
  assert.match(layout, /RetrievalKit Docs/);
  assert.match(notFound, /Page not found/);
  assert.doesNotMatch(layout, /codex-preview|Starter Project/);
  assert.doesNotMatch(layout, /next\/font|Geist/);
  assert.doesNotMatch(page, /authorized repository|authorized checkout/i);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);

  await assert.rejects(
    access(new URL("../app/_sites-preview", templateRoot)),
  );
});
