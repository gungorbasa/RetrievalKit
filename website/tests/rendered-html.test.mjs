import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const templateRoot = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("http://localhost/", {
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
  assert.match(html, /Install the Python source preview/);
  assert.match(html, /Swift, Kotlin, and Node\.js APIs/);
  assert.match(html, /TypeScript \/ Node\.js/);
  assert.match(html, /Kotlin \/ Android/);
  assert.match(html, /Search documentation/);
  assert.match(html, /retrievalkit-python-source-preview\.tar\.gz/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape/);
});

test("removes starter-only files and dependencies", async () => {
  const [page, layout, packageJson] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
  ]);

  assert.match(page, /RetrievalKit/);
  assert.match(page, /type="search"/);
  assert.match(layout, /RetrievalKit Docs/);
  assert.doesNotMatch(layout, /codex-preview|Starter Project/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);

  await assert.rejects(
    access(new URL("../app/_sites-preview", templateRoot)),
  );
});
