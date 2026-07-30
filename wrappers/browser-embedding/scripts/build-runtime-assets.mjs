import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = resolve(packageRoot, "dist/runtime");
const runtimeRoot = resolve(packageRoot, "node_modules/onnxruntime-web");
const tokenizerRoot = resolve(packageRoot, "node_modules/@huggingface/tokenizers");

const assets = [
  {
    source: "dist/ort-wasm-simd-threaded.mjs",
    name: "ort-wasm-simd-threaded.mjs",
    bytes: 24_180,
    sha256: "0a1e718d99c41b22c21f2520ff4f9e883a6b5533856e398d21816ee8eb8185d3"
  },
  {
    source: "dist/ort-wasm-simd-threaded.wasm",
    name: "ort-wasm-simd-threaded.wasm",
    bytes: 13_479_978,
    sha256: "d1ab1b94b16a65b29d710d0b587b29e7bed336827577623913479b8afe8113e6"
  },
  {
    source: "dist/ort-wasm-simd-threaded.asyncify.mjs",
    name: "ort-wasm-simd-threaded.asyncify.mjs",
    bytes: 47_507,
    sha256: "7236653b8565da4046e459cd0e274123419a1d9f1f8f18fd36c28058346ca655"
  },
  {
    source: "dist/ort-wasm-simd-threaded.asyncify.wasm",
    name: "ort-wasm-simd-threaded.asyncify.wasm",
    bytes: 24_254_953,
    sha256: "7e83cd6cee77e478bc96a7e91b198144fb5e4126287daf1f9b54bb195ebcd55a"
  }
];

await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });
for (const asset of assets) {
  const source = resolve(runtimeRoot, asset.source);
  const bytes = await readFile(source);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (bytes.byteLength !== asset.bytes || digest !== asset.sha256) {
    throw new Error(`Pinned ONNX Runtime asset '${asset.name}' failed verification.`);
  }
  await copyFile(source, resolve(output, asset.name));
}
const legal = [
  {
    source: resolve(packageRoot, "vendor/ONNXRUNTIME-LICENSE"),
    installed: undefined,
    name: "ONNXRUNTIME-LICENSE",
    sha256: "2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c"
  },
  {
    source: resolve(packageRoot, "vendor/ONNXRUNTIME-ThirdPartyNotices.txt"),
    installed: undefined,
    name: "ONNXRUNTIME-ThirdPartyNotices.txt",
    sha256: "0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2"
  },
  {
    source: resolve(packageRoot, "vendor/HUGGINGFACE-TOKENIZERS-LICENSE"),
    installed: resolve(tokenizerRoot, "LICENSE"),
    name: "HUGGINGFACE-TOKENIZERS-LICENSE",
    sha256: "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4"
  }
];
for (const file of legal) {
  const bytes = await readFile(file.source);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== file.sha256) throw new Error(`Legal file '${file.name}' drifted.`);
  if (file.installed !== undefined) {
    const installed = await readFile(file.installed);
    const installedDigest = createHash("sha256").update(installed).digest("hex");
    if (installedDigest !== file.sha256) {
      throw new Error(`Installed dependency legal file '${file.name}' drifted.`);
    }
  }
  await copyFile(file.source, resolve(output, file.name));
}
