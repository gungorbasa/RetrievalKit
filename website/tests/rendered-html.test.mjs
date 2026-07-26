import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

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
    /Coordinates selected; pending Central verification and publication/,
  );
  assert.match(
    html,
    /Scoped names reserved; trusted publishing configured; v0\.1\.0 unpublished/,
  );
  assert.match(html, /Public source; pending v0\.1\.0 tag and XCFramework release/);
  assert.match(html, /Public repository source checkout/);
  assert.match(html, /Public graph source bundle and authorized checkout/);
  assert.match(html, /Swift, Kotlin, and Node\.js APIs/);
  assert.match(html, /Swift \/ Apple platforms/);
  assert.match(html, /One package, selectable retrieval and graph products/);
  assert.match(html, /macOS 14\+ arm64/);
  assert.match(html, /iOS 15\+ arm64 device \/ simulator/);
  assert.match(html, /TypeScript \/ Node\.js/);
  assert.match(html, /Node\.js 22\.13\+ LTS or Node\.js 24 LTS/);
  assert.match(html, /Kotlin \/ Android/);
  assert.match(html, /schema = GraphSchema/);
  assert.match(html, /import ai\.retrievalkit\.Document/);
  assert.match(html, /Python preview revision/);
  assert.match(html, /Search documentation/);
  assert.match(html, /retrievalkit-python-source-preview\.tar\.gz/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape/);
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
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);

  await assert.rejects(
    access(new URL("../app/_sites-preview", templateRoot)),
  );
});
