import { cp, mkdir, stat } from "node:fs/promises";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const workspace = resolve(here, "../../..");
const target = process.argv[2] ?? "all";

if (process.platform !== "darwin" || process.arch !== "arm64") {
  throw new Error(
    `The initial repository package supports macOS arm64 only; got ${process.platform}-${process.arch}.`
  );
}

async function run(command, args) {
  await new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { cwd: workspace, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} exited with code ${String(code)}`));
    });
  });
}

async function build(packageDirectory, features = []) {
  const args = ["build", "--release", "-p", "retrievalkit-node"];
  if (features.length > 0) args.push("--features", features.join(","));
  await run("cargo", args);
  const source = join(workspace, "target/release/libretrievalkit_node_native.dylib");
  await stat(source);
  const nativeDirectory = join(here, "..", packageDirectory, "native");
  await mkdir(nativeDirectory, { recursive: true });
  await cp(source, join(nativeDirectory, "retrievalkit.node"));
}

if (target === "base" || target === "all") await build("base");
if (target === "graph" || target === "all") await build("graph", ["graph"]);
