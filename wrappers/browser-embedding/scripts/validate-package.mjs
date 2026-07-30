import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packed = spawnSync(
  "npm",
  ["pack", "--json", "--dry-run", "--ignore-scripts"],
  { cwd: packageRoot, encoding: "utf8" }
);
if (packed.status !== 0) throw new Error(packed.stderr || "npm pack failed");
const report = JSON.parse(packed.stdout);
const names = new Set(report[0].files.map((file) => file.path));

const required = [
  "LICENSE",
  "NOTICE",
  "README.md",
  "THIRD_PARTY_NOTICES.md",
  "package.json",
  "dist/index.js",
  "dist/index.d.ts",
  "dist/worker.js",
  "dist/worker.d.ts",
  "dist/runtime/ort-wasm-simd-threaded.mjs",
  "dist/runtime/ort-wasm-simd-threaded.wasm",
  "dist/runtime/ort-wasm-simd-threaded.asyncify.mjs",
  "dist/runtime/ort-wasm-simd-threaded.asyncify.wasm",
  "dist/runtime/ONNXRUNTIME-LICENSE",
  "dist/runtime/ONNXRUNTIME-ThirdPartyNotices.txt",
  "dist/runtime/HUGGINGFACE-TOKENIZERS-LICENSE"
];
for (const name of required) {
  if (!names.has(name)) throw new Error(`Package is missing '${name}'.`);
}
for (const name of names) {
  if (name.endsWith(".node") || /all-MiniLM|manifest-v1|tokenizer\.json/.test(name)) {
    throw new Error(`Package unexpectedly contains model/native artifact '${name}'.`);
  }
}

const identities = [
  ["LICENSE", resolve(packageRoot, "../../LICENSE")],
  ["NOTICE", resolve(packageRoot, "../../NOTICE")]
];
for (const [local, root] of identities) {
  const localBytes = await readFile(resolve(packageRoot, local));
  const rootBytes = await readFile(root);
  if (!localBytes.equals(rootBytes)) throw new Error(`${local} differs from repository root.`);
}

const runtimeAssets = [
  [
    "dist/runtime/ort-wasm-simd-threaded.mjs",
    24_180,
    "0a1e718d99c41b22c21f2520ff4f9e883a6b5533856e398d21816ee8eb8185d3"
  ],
  [
    "dist/runtime/ort-wasm-simd-threaded.wasm",
    13_479_978,
    "d1ab1b94b16a65b29d710d0b587b29e7bed336827577623913479b8afe8113e6"
  ],
  [
    "dist/runtime/ort-wasm-simd-threaded.asyncify.mjs",
    47_507,
    "7236653b8565da4046e459cd0e274123419a1d9f1f8f18fd36c28058346ca655"
  ],
  [
    "dist/runtime/ort-wasm-simd-threaded.asyncify.wasm",
    24_254_953,
    "7e83cd6cee77e478bc96a7e91b198144fb5e4126287daf1f9b54bb195ebcd55a"
  ]
];
const wasmFiles = [...names].filter((name) => name.endsWith(".wasm")).sort();
const expectedWasm = runtimeAssets
  .map(([name]) => name)
  .filter((name) => name.endsWith(".wasm"))
  .sort();
if (JSON.stringify(wasmFiles) !== JSON.stringify(expectedWasm)) {
  throw new Error(`Packaged WASM inventory is invalid: ${wasmFiles.join(", ")}`);
}
for (const [name, size, digest] of runtimeAssets) {
  const bytes = await readFile(resolve(packageRoot, name));
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (bytes.byteLength !== size || actual !== digest) {
    throw new Error(`Packaged runtime asset '${name}' failed verification.`);
  }
}

process.stdout.write(`validated ${names.size} package files\n`);
