import { execFileSync } from "node:child_process";

function fail(message) {
  console.error(`TypeScript wrapper preflight failed: ${message}`);
  process.exit(1);
}

const nodeMajor = Number.parseInt(process.versions.node.split(".", 1)[0], 10);
if (!Number.isInteger(nodeMajor) || nodeMajor < 20) {
  fail(`Node.js 20 or newer is required; detected Node.js ${process.versions.node}.`);
}

if (process.platform !== "darwin" || process.arch !== "arm64") {
  fail(
    `the initial Node.js package requires macOS arm64; detected ${process.platform}-${process.arch}.`
  );
}

let cargoVersion;
try {
  cargoVersion = execFileSync("cargo", ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"]
  }).trim();
} catch {
  fail("Rust cargo is required on PATH; install Rust with rustup and retry.");
}

console.log("TypeScript wrapper preflight passed");
console.log(`  Node.js: required >=20; detected ${process.versions.node}`);
console.log(`  Rust: required cargo on PATH; detected ${cargoVersion}`);
console.log(`  Host: required darwin-arm64; detected ${process.platform}-${process.arch}`);
