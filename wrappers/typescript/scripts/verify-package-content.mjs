import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const exec = promisify(execFile);
const here = dirname(fileURLToPath(import.meta.url));
const workspace = resolve(here, "../../..");

async function packFiles(packageDirectory) {
  const { stdout } = await exec("npm", ["pack", "--dry-run", "--json"], {
    cwd: resolve(here, "..", packageDirectory)
  });
  const report = JSON.parse(stdout);
  return report[0].files.map((file) => file.path);
}

const baseFiles = await packFiles("base");
const graphFiles = await packFiles("graph");
for (const required of ["LICENSE", "NOTICE", "README.md", "dist/index.js", "dist/index.d.ts", "native/retrievalkit.node"]) {
  if (!baseFiles.includes(required)) throw new Error(`base package is missing ${required}`);
  if (!graphFiles.includes(required)) throw new Error(`graph package is missing ${required}`);
}
if (baseFiles.some((file) => /graph/i.test(file))) {
  throw new Error(`base package unexpectedly contains a graph-named file: ${baseFiles.join(", ")}`);
}
const { stdout: tree } = await exec(
  "cargo",
  ["tree", "-p", "retrievalkit-node", "--no-default-features"],
  { cwd: workspace }
);
if (tree.includes("retrievalkit-graph")) {
  throw new Error("base native dependency tree includes retrievalkit-graph");
}
const { stdout: baseStrings } = await exec(
  "strings",
  [resolve(here, "../base/native/retrievalkit.node")]
);
if (/retrievalkit[_-]graph|NativeGraphHandle|GraphRetrievalDatabase/i.test(baseStrings)) {
  throw new Error("base native binary contains graph aggregate symbols");
}
const basePackage = JSON.parse(
  await readFile(resolve(here, "../base/package.json"), "utf8")
);
if (JSON.stringify(basePackage).includes("@gungorbasa/retrievalkit-graph")) {
  throw new Error("base package metadata depends on the graph package");
}
console.log("Package contents and graph-free base dependency tree verified.");
