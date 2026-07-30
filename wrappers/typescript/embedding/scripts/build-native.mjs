import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(packageRoot, "../../..");
const cargo = spawnSync(
  "cargo",
  [
    "build",
    "--locked",
    "--release",
    "-p",
    "retrievalkit-node-embedding"
  ],
  { cwd: repositoryRoot, stdio: "inherit" }
);
if (cargo.status !== 0) {
  process.exit(cargo.status ?? 1);
}

const nativeDirectory = join(packageRoot, "native");
await mkdir(nativeDirectory, { recursive: true });
await copyFile(
  join(
    repositoryRoot,
    "target/release/libretrievalkit_node_embedding_native.dylib"
  ),
  join(nativeDirectory, "retrievalkit-embedding.node")
);

if (process.env["RETRIEVALKIT_BUNDLE_ONNX_RUNTIME"] === "1") {
  await bundleRuntime();
}

async function bundleRuntime() {
  const source = process.env["RETRIEVALKIT_ONNX_RUNTIME_LIBRARY"];
  if (source === undefined) {
    throw new Error(
      "RETRIEVALKIT_ONNX_RUNTIME_LIBRARY is required when RETRIEVALKIT_BUNDLE_ONNX_RUNTIME=1"
    );
  }
  const expectedName = "libonnxruntime.1.24.3.dylib";
  if (source.split("/").at(-1) !== expectedName) {
    throw new Error(`The bundled runtime must be named ${expectedName}.`);
  }
  const metadata = await stat(source);
  if (metadata.size !== 27_724_968) {
    throw new Error(`Unexpected ONNX Runtime size: ${metadata.size}.`);
  }
  const digest = createHash("sha256")
    .update(await readFile(source))
    .digest("hex");
  if (
    digest !==
    "b65e22247d3ce2976931cfc6be3929e6fb81cd55e2f202e95e0ab8c9de5fa729"
  ) {
    throw new Error("ONNX Runtime SHA-256 verification failed.");
  }
  const runtimeDirectory = join(packageRoot, "runtime");
  await mkdir(runtimeDirectory, { recursive: true });
  await copyFile(source, join(runtimeDirectory, expectedName));

  for (const name of ["LICENSE", "ThirdPartyNotices.txt"]) {
    let copied = false;
    for (const directory of [dirname(source), dirname(dirname(source))]) {
      const candidate = join(directory, name);
      try {
        await copyFile(candidate, join(runtimeDirectory, `ONNX-Runtime-${name}`));
        copied = true;
        break;
      } catch (error) {
        if (error?.code !== "ENOENT") {
          throw error;
        }
      }
    }
    if (!copied) {
      throw new Error(
        `Cannot bundle ONNX Runtime without its required ${name} legal file.`
      );
    }
  }
}
