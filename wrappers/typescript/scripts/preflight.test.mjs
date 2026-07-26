import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  NODE_ENGINES,
  SUPPORTED_NODE_RANGES,
  isSupportedNodeVersion,
  unsupportedNodeMessage
} from "./node-support.mjs";

test("supported Node.js ranges explicitly track maintained LTS majors", () => {
  assert.deepEqual(
    SUPPORTED_NODE_RANGES.map(({ major }) => major),
    [22, 24]
  );
  assert.equal(isSupportedNodeVersion("22.13.0"), true);
  assert.equal(isSupportedNodeVersion("22.20.1"), true);
  assert.equal(isSupportedNodeVersion("24.0.0"), true);
  assert.equal(isSupportedNodeVersion("24.18.0"), true);
});

test("preflight rejects EOL, too-old, current, and malformed versions", () => {
  assert.equal(isSupportedNodeVersion("20.20.0"), false);
  assert.equal(isSupportedNodeVersion("22.12.9"), false);
  assert.equal(isSupportedNodeVersion("25.9.0"), false);
  assert.equal(isSupportedNodeVersion("26.0.0"), false);
  assert.equal(isSupportedNodeVersion("unknown"), false);
});

test("unsupported-version recovery recommends an LTS runtime", () => {
  const message = unsupportedNodeMessage("25.9.0");
  assert.match(message, /Node\.js 22\.13\+ LTS or Node\.js 24 LTS/u);
  assert.match(message, /detected Node\.js 25\.9\.0/u);
  assert.match(message, /Node\.js 24 LTS release \(recommended\)/u);
  assert.match(message, /nvm install 24 && nvm use 24/u);
});

test("workspace and distributable package engines match the preflight policy", async () => {
  const packagePaths = [
    new URL("../package.json", import.meta.url),
    new URL("../base/package.json", import.meta.url),
    new URL("../graph/package.json", import.meta.url)
  ];

  for (const packagePath of packagePaths) {
    const packageJson = JSON.parse(await readFile(packagePath, "utf8"));
    assert.equal(packageJson.engines?.node, NODE_ENGINES, packagePath.pathname);
  }
});
