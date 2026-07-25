import { execFile } from "node:child_process";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const exec = promisify(execFile);
const here = dirname(fileURLToPath(import.meta.url));
const temporary = await mkdtemp(join(tmpdir(), "retrievalkit-node-install-"));
const generatedTarballs = [];
try {
  const basePack = await exec("npm", ["pack", "--json"], { cwd: resolve(here, "../base") });
  const graphPack = await exec("npm", ["pack", "--json"], { cwd: resolve(here, "../graph") });
  const baseName = JSON.parse(basePack.stdout)[0].filename;
  const graphName = JSON.parse(graphPack.stdout)[0].filename;
  const baseTar = resolve(here, "../base", baseName);
  const graphTar = resolve(here, "../graph", graphName);
  generatedTarballs.push(baseTar, graphTar);
  for (const [name, tar, source] of [
    [
      "base",
      baseTar,
      `import { RetrievalDatabaseBuilder } from "retrievalkit-node-local";
const b = new RetrievalDatabaseBuilder({ corpusId: "smoke", metric: "dotProduct", encoding: "f32" });
await b.add([{ id: "one", text: "local", embedding: new Float32Array([1, 0]) }]);
await using db = await b.build();
if ((await db.search({ mode: "vector", embedding: new Float32Array([1, 0]) }))[0]?.documentId !== "one") process.exit(2);`
    ],
    [
      "graph",
      graphTar,
      `import { GraphDatabaseBuilder } from "retrievalkit-node-graph-local";
const b = new GraphDatabaseBuilder({ corpusId: "smoke", schema: { recordNodes: [{ recordType: "Topic", nodeType: "Topic", queryableFields: [["title"]] }] } });
await b.add([{ id: "one", type: "Topic", fields: { title: "One" }, content: "one" }]);
await using db = await b.build();
await using selection = await db.graph.query({ seed: { kind: "equals", nodeType: "Topic", field: ["title"], values: ["One"] } });
if (selection.matches[0]?.node.recordId !== "one") process.exit(2);`
    ]
  ]) {
    const project = join(temporary, name);
    await exec("mkdir", ["-p", project]);
    await exec("npm", ["init", "-y"], { cwd: project });
    await exec("npm", ["install", tar], { cwd: project });
    await writeFile(join(project, "smoke.mjs"), source);
    await exec("node", ["smoke.mjs"], { cwd: project });
  }
  console.log("Clean local-package install smoke tests passed.");
} finally {
  await rm(temporary, { recursive: true, force: true });
  await Promise.all(generatedTarballs.map((tarball) => rm(tarball, { force: true })));
}
