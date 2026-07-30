#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile
} from "node:fs/promises";
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep
} from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPOSITORY_ROOT = resolve(dirname(SCRIPT_PATH), "../..");
const DEFAULT_PACKAGE_DIST = join(
  REPOSITORY_ROOT,
  "wrappers/browser-embedding/dist"
);
const DEFAULT_TIMEOUT_MS = 180_000;
const DEFAULT_CHROME_CANDIDATES =
  process.platform === "darwin"
    ? [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary"
      ]
    : process.platform === "win32"
      ? [
          join(
            process.env["PROGRAMFILES"] ?? "C:\\Program Files",
            "Google/Chrome/Application/chrome.exe"
          ),
          join(
            process.env["LOCALAPPDATA"] ?? "",
            "Google/Chrome/Application/chrome.exe"
          )
        ]
      : [
          "/usr/bin/google-chrome",
          "/usr/bin/google-chrome-stable",
          "/usr/bin/chromium",
          "/usr/bin/chromium-browser"
        ];

const HELP = `Usage:
  node scripts/embedding/qualify-browser-embedding-webgpu.mjs \\
    --artifacts PATH \\
    --output PATH \\
    [--package-dist PATH] [--chrome PATH] [--timeout-ms INTEGER]

Launches installed Chromium directly through its DevTools protocol, serves only
loopback files, and runs the built browser embedding service in a real dedicated
module Worker with execution:'webgpu'. No Playwright, package install, artifact
download, CDN, or external model request is used.

Required:
  --artifacts PATH     Frozen local artifact root containing manifest-v1.json,
                       onnx/, and tokenizer/.
  --output PATH        Deterministic-schema WebGPU benchmark/validation JSON.

Optional:
  --package-dist PATH  Built wrappers/browser-embedding/dist directory.
                       Defaults to ${DEFAULT_PACKAGE_DIST}
  --chrome PATH        Google Chrome or Chromium executable. Auto-detected when
                       omitted.
  --timeout-ms INTEGER Overall browser result timeout in milliseconds.
                       Default: ${DEFAULT_TIMEOUT_MS}
  -h, --help           Show this help.

The Worker proves its benchmark input is exactly 32 BERT tokens, requires the
selected provider to equal "webgpu", validates one 384-value finite normalized
Float32 output, then runs exactly 50 warmups and 750 batch-one measurements.
If WebGPU is unavailable or initialization falls back/fails, qualification
fails rather than recording WASM numbers.
`;

const EXPECTED_ARTIFACT_PATHS = Object.freeze([
  "manifest-v1.json",
  "onnx/all-MiniLM-L6-v2-fp32.onnx",
  "tokenizer/tokenizer.json",
  "tokenizer/tokenizer_config.json",
  "tokenizer/special_tokens_map.json",
  "tokenizer/vocab.txt"
]);

export class WebGpuQualificationError extends Error {}

export function parseArguments(arguments_) {
  const argumentsList = [...arguments_];
  if (argumentsList.includes("--help") || argumentsList.includes("-h")) {
    return { help: true };
  }
  const allowed = new Set([
    "--artifacts",
    "--output",
    "--package-dist",
    "--chrome",
    "--timeout-ms"
  ]);
  const values = new Map();
  for (let index = 0; index < argumentsList.length; index += 1) {
    const option = argumentsList[index];
    if (!allowed.has(option)) {
      throw new WebGpuQualificationError(`Unknown option '${option}'.`);
    }
    if (values.has(option)) {
      throw new WebGpuQualificationError(`Option '${option}' was provided twice.`);
    }
    const value = argumentsList[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new WebGpuQualificationError(`Option '${option}' requires a value.`);
    }
    values.set(option, value);
    index += 1;
  }
  for (const required of ["--artifacts", "--output"]) {
    if (!values.has(required)) {
      throw new WebGpuQualificationError(`Missing required option '${required}'.`);
    }
  }
  const timeoutMs = Number(values.get("--timeout-ms") ?? DEFAULT_TIMEOUT_MS);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 10_000 || timeoutMs > 900_000) {
    throw new WebGpuQualificationError(
      "--timeout-ms must be an integer from 10000 through 900000."
    );
  }
  return {
    help: false,
    artifacts: resolve(values.get("--artifacts")),
    output: resolve(values.get("--output")),
    packageDist: resolve(values.get("--package-dist") ?? DEFAULT_PACKAGE_DIST),
    chrome:
      values.get("--chrome") === undefined
        ? undefined
        : resolve(values.get("--chrome")),
    timeoutMs
  };
}

export function rewriteBuiltModule(source) {
  return source
    .replaceAll(
      'from "@huggingface/tokenizers"',
      'from "/vendor/tokenizers.mjs"'
    )
    .replaceAll(
      'import("onnxruntime-web/webgpu")',
      'import("/vendor/ort.webgpu.bundle.min.mjs")'
    )
    .replaceAll(
      'import("onnxruntime-web/wasm")',
      'import("/vendor/ort.wasm.bundle.min.mjs")'
    );
}

export function nearestRankPercentile(sortedValues, fraction) {
  if (
    !Array.isArray(sortedValues) ||
    sortedValues.length === 0 ||
    !Number.isFinite(fraction) ||
    fraction <= 0 ||
    fraction > 1
  ) {
    throw new WebGpuQualificationError("Percentile input is invalid.");
  }
  return sortedValues[
    Math.min(
      sortedValues.length - 1,
      Math.max(0, Math.ceil(sortedValues.length * fraction) - 1)
    )
  ];
}

export function validateWorkerResult(result) {
  if (result === null || typeof result !== "object" || Array.isArray(result)) {
    throw new WebGpuQualificationError("Worker result must be an object.");
  }
  if (result.ok !== true) {
    throw new WebGpuQualificationError(
      `Chromium WebGPU Worker failed: ${String(result.error ?? "unknown error")}`
    );
  }
  const benchmark = result.benchmark;
  if (
    benchmark?.provider !== "webgpu" ||
    benchmark?.dimension !== 384 ||
    benchmark?.normalized !== true ||
    benchmark?.input_tokens !== 32 ||
    benchmark?.batch_size !== 1 ||
    benchmark?.warmups !== 50 ||
    benchmark?.measured !== 750 ||
    !isFinitePositive(benchmark.cached_initialization_ms) ||
    !isFinitePositive(benchmark.first_inference_ms) ||
    !Array.isArray(benchmark.samples_ms) ||
    benchmark.samples_ms.length !== 750 ||
    benchmark.samples_ms.some((value) => !isFinitePositive(value))
  ) {
    throw new WebGpuQualificationError(
      "Worker result does not satisfy the fixed WebGPU qualification contract."
    );
  }
  return benchmark;
}

export function qualificationWorkerSource() {
  return `import { EmbeddingWorkerService } from "/package/service.js";
import { MemoryArtifactStore } from "/package/store.js";
import { PINNED_ARTIFACTS } from "/package/constants.js";
import { PinnedMiniLmTokenizer } from "/package/tokenizer.js";

const DIMENSION = 384;
const WARMUPS = 50;
const MEASURED = 750;
const INPUT_TOKENS = 32;

void qualify().then(
  (benchmark) => postMessage({ ok: true, benchmark }),
  (error) => postMessage({ ok: false, error: formatError(error) })
);

async function qualify() {
  if (
    typeof DedicatedWorkerGlobalScope === "undefined" ||
    !(self instanceof DedicatedWorkerGlobalScope)
  ) {
    throw new Error("Qualification is not running in a dedicated Worker.");
  }
  if (!("gpu" in navigator)) {
    throw new Error("WebGPU is unavailable in this dedicated Worker.");
  }
  const byUrl = new Map(PINNED_ARTIFACTS.map((artifact) => [
    artifact.url,
    artifact.path
  ]));
  let artifactRequests = 0;
  const fetcher = async (url, signal) => {
    if (signal?.aborted === true) throw new Error("Artifact fetch was cancelled.");
    const path = byUrl.get(url);
    if (path === undefined) throw new Error("Unexpected artifact URL: " + url);
    artifactRequests += 1;
    const response = await fetch("/artifacts/" + encodeURIComponent(path), {
      cache: "no-store",
      signal
    });
    if (!response.ok) {
      throw new Error("Local artifact request failed with HTTP " + response.status);
    }
    return new Uint8Array(await response.arrayBuffer());
  };
  const store = new MemoryArtifactStore("memory:chromium-webgpu-qualification");
  const dependencies = {
    artifacts: PINNED_ARTIFACTS,
    fetcher,
    createStore: () => store
  };

  const prepare = new EmbeddingWorkerService(dependencies);
  await prepare.prefetch({ localOnly: false });
  await prepare.close();

  const service = new EmbeddingWorkerService(dependencies);
  const initializationStart = performance.now();
  await service.load({ localOnly: true, execution: "webgpu" });
  const cachedInitializationMs = performance.now() - initializationStart;
  if (service.provider !== "webgpu") {
    throw new Error("Expected WebGPU provider, selected " + service.provider);
  }

  try {
    const tokenizerJson = await localArtifact("tokenizer/tokenizer.json");
    const tokenizerConfig = await localArtifact(
      "tokenizer/tokenizer_config.json"
    );
    const tokenizer = new PinnedMiniLmTokenizer(tokenizerJson, tokenizerConfig);
    const text = "local ".repeat(INPUT_TOKENS - 2).trim();
    const encoded = tokenizer.tokenize([text]);
    if (encoded.batchSize !== 1 || encoded.sequenceLength !== INPUT_TOKENS) {
      throw new Error(
        "Benchmark input tokenized to " + encoded.sequenceLength +
        " tokens; expected " + INPUT_TOKENS
      );
    }

    const firstStart = performance.now();
    validateVector(await service.embed(text));
    const firstInferenceMs = performance.now() - firstStart;
    for (let index = 0; index < WARMUPS; index += 1) {
      validateVector(await service.embed(text));
    }
    const samples = [];
    for (let index = 0; index < MEASURED; index += 1) {
      const start = performance.now();
      validateVector(await service.embed(text));
      samples.push(performance.now() - start);
    }
    return {
      provider: service.provider,
      dimension: DIMENSION,
      normalized: true,
      input_tokens: INPUT_TOKENS,
      batch_size: 1,
      warmups: WARMUPS,
      measured: MEASURED,
      artifact_requests: artifactRequests,
      cached_initialization_ms: cachedInitializationMs,
      first_inference_ms: firstInferenceMs,
      samples_ms: samples
    };
  } finally {
    await service.close();
  }
}

async function localArtifact(path) {
  const response = await fetch("/artifacts/" + encodeURIComponent(path), {
    cache: "no-store"
  });
  if (!response.ok) throw new Error("Cannot load local tokenizer artifact.");
  return new Uint8Array(await response.arrayBuffer());
}

function validateVector(vector) {
  if (!(vector instanceof Float32Array) || vector.length !== DIMENSION) {
    throw new Error("Embedding is not a 384-value Float32Array.");
  }
  let squaredNorm = 0;
  for (const value of vector) {
    if (!Number.isFinite(value)) throw new Error("Embedding is not finite.");
    squaredNorm += value * value;
  }
  const norm = Math.sqrt(squaredNorm);
  if (!Number.isFinite(norm) || Math.abs(norm - 1) > 1e-4) {
    throw new Error("Embedding norm is invalid: " + norm);
  }
}

function formatError(error) {
  const messages = [];
  const seen = new Set();
  let current = error;
  while (current !== undefined && current !== null && !seen.has(current)) {
    seen.add(current);
    messages.push(current instanceof Error ? current.message : String(current));
    current = current instanceof Error ? current.cause : undefined;
  }
  return messages.join(" Caused by: ");
}
`;
}

function harnessPageSource() {
  return `const worker = new Worker("/qualification-worker.mjs", { type: "module" });
globalThis.__qualificationResult = null;
worker.addEventListener("message", (event) => {
  globalThis.__qualificationResult = event.data;
  worker.terminate();
}, { once: true });
worker.addEventListener("error", (event) => {
  globalThis.__qualificationResult = {
    ok: false,
    error: event.message || "Dedicated Worker crashed."
  };
  worker.terminate();
}, { once: true });
`;
}

async function run(options) {
  const chrome = await resolveChrome(options.chrome);
  const packageRoot = resolve(options.packageDist, "..");
  await verifyPackageMetadata(packageRoot);
  await verifyArtifactTree(options.artifacts);

  const temporaryRoot = await createTargetTemporaryDirectory();
  const userDataDirectory = join(temporaryRoot, "chrome-profile");
  const generatedRoot = join(temporaryRoot, "served");
  await mkdir(userDataDirectory, { recursive: true });
  await mkdir(generatedRoot, { recursive: true });
  await writeFile(
    join(generatedRoot, "harness.html"),
    '<!doctype html><meta charset="utf-8"><title>RetrievalKit WebGPU Qualification</title><script type="module" src="/harness-page.mjs"></script>\n',
    "utf8"
  );
  await writeFile(
    join(generatedRoot, "harness-page.mjs"),
    harnessPageSource(),
    "utf8"
  );
  await writeFile(
    join(generatedRoot, "qualification-worker.mjs"),
    qualificationWorkerSource(),
    "utf8"
  );

  let server;
  let chromeProcess;
  try {
    const routeContext = {
      generatedRoot,
      packageDist: options.packageDist,
      packageRoot,
      artifactRoot: options.artifacts
    };
    server = await startLoopbackServer(routeContext);
    const address = server.address();
    if (address === null || typeof address === "string") {
      throw new WebGpuQualificationError("Loopback server has no TCP address.");
    }
    const pageUrl = `http://127.0.0.1:${address.port}/harness.html`;
    const launched = await launchChrome(
      chrome,
      userDataDirectory,
      pageUrl,
      options.timeoutMs
    );
    chromeProcess = launched.process;
    const cdp = await CdpConnection.connect(launched.webSocketUrl);
    try {
      const browserVersion = await cdp.call("Browser.getVersion");
      const target = await cdp.call("Target.createTarget", { url: pageUrl });
      const attached = await cdp.call("Target.attachToTarget", {
        targetId: target.targetId,
        flatten: true
      });
      await cdp.call("Runtime.enable", {}, attached.sessionId);
      const workerResult = await pollWorkerResult(
        cdp,
        attached.sessionId,
        options.timeoutMs
      );
      const fixed = validateWorkerResult(workerResult);
      fixed.samples_ms.sort((left, right) => left - right);
      const mean =
        fixed.samples_ms.reduce((total, value) => total + value, 0) /
        fixed.samples_ms.length;
      const report = {
        schema_version: 1,
        kind: "retrievalkit_browser_embedding_chromium_webgpu_qualification",
        browser: {
          product: browserVersion.product,
          protocol_version: browserVersion.protocolVersion,
          user_agent: browserVersion.userAgent,
          javascript_version: browserVersion.jsVersion,
          headless: true,
          origin: "loopback"
        },
        model: {
          identifier: "sentence-transformers/all-MiniLM-L6-v2",
          revision: "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
          profile: "fp32",
          dtype: "float32",
          dimension: 384,
          max_input_tokens: 256,
          normalized: true
        },
        runtime: {
          package: "onnxruntime-web",
          version: "1.27.0",
          execution_provider: fixed.provider,
          dedicated_module_worker: true,
          external_network_allowed: false,
          artifact_source: "loopback_local_files"
        },
        validation: {
          output_dimension: fixed.dimension,
          finite: true,
          normalized: fixed.normalized,
          artifact_requests: fixed.artifact_requests
        },
        benchmark: {
          input_tokens: fixed.input_tokens,
          batch_size: fixed.batch_size,
          warmups: fixed.warmups,
          measured: fixed.measured,
          cached_initialization_ms: fixed.cached_initialization_ms,
          first_inference_ms: fixed.first_inference_ms,
          warm_inference_ms: {
            minimum: fixed.samples_ms[0],
            mean,
            p50: nearestRankPercentile(fixed.samples_ms, 0.5),
            p95: nearestRankPercentile(fixed.samples_ms, 0.95),
            p99: nearestRankPercentile(fixed.samples_ms, 0.99),
            maximum: fixed.samples_ms.at(-1)
          }
        }
      };
      await writeJsonAtomically(options.output, report);
      process.stdout.write(
        `${JSON.stringify({
          output: options.output,
          browser: report.browser.product,
          provider: fixed.provider,
          cached_initialization_ms: fixed.cached_initialization_ms,
          first_inference_ms: fixed.first_inference_ms,
          warm_p95_ms: report.benchmark.warm_inference_ms.p95
        })}\n`
      );
    } finally {
      cdp.close();
    }
  } finally {
    if (chromeProcess !== undefined) {
      chromeProcess.kill("SIGTERM");
      await waitForExit(chromeProcess, 5_000).catch(() => {
        chromeProcess.kill("SIGKILL");
      });
    }
    if (server !== undefined) {
      await new Promise((resolveClose) => server.close(resolveClose));
    }
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function resolveChrome(explicit) {
  const candidates = explicit === undefined ? DEFAULT_CHROME_CANDIDATES : [explicit];
  for (const candidate of candidates) {
    if (candidate === "") continue;
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Continue to the next installed-Chromium candidate.
    }
  }
  throw new WebGpuQualificationError(
    explicit === undefined
      ? "No installed Google Chrome or Chromium executable was found; pass --chrome PATH."
      : `Chromium executable does not exist: ${explicit}`
  );
}

async function verifyPackageMetadata(packageRoot) {
  const packageDocument = JSON.parse(
    await readFile(join(packageRoot, "package.json"), "utf8")
  );
  if (
    packageDocument.dependencies?.["onnxruntime-web"] !== "1.27.0" ||
    packageDocument.dependencies?.["@huggingface/tokenizers"] !== "0.1.3"
  ) {
    throw new WebGpuQualificationError(
      "Browser package dependency pins do not match the qualification contract."
    );
  }
}

async function verifyArtifactTree(root) {
  await Promise.all(
    EXPECTED_ARTIFACT_PATHS.map(async (path) => {
      await access(resolveSafe(root, path));
    })
  );
}

async function createTargetTemporaryDirectory() {
  const targetRoot = join(REPOSITORY_ROOT, "target");
  await mkdir(targetRoot, { recursive: true });
  return await mkdtemp(join(targetRoot, "browser-embedding-webgpu-"));
}

async function startLoopbackServer(context) {
  const server = createServer((request, response) => {
    void serveRequest(request, response, context).catch((error) => {
      response.writeHead(500, { "content-type": "text/plain; charset=utf-8" });
      response.end(error instanceof Error ? error.message : String(error));
    });
  });
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  return server;
}

async function serveRequest(request, response, context) {
  const requestUrl = new URL(request.url ?? "/", "http://127.0.0.1");
  let path;
  let rewrite = false;
  if (requestUrl.pathname === "/harness.html") {
    path = join(context.generatedRoot, "harness.html");
  } else if (requestUrl.pathname === "/harness-page.mjs") {
    path = join(context.generatedRoot, "harness-page.mjs");
  } else if (requestUrl.pathname === "/qualification-worker.mjs") {
    path = join(context.generatedRoot, "qualification-worker.mjs");
  } else if (requestUrl.pathname.startsWith("/package/")) {
    const relativePath = decodeURIComponent(
      requestUrl.pathname.slice("/package/".length)
    );
    path = resolveSafe(context.packageDist, relativePath);
    rewrite = path.endsWith(".js");
  } else if (requestUrl.pathname.startsWith("/artifacts/")) {
    const relativePath = decodeURIComponent(
      requestUrl.pathname.slice("/artifacts/".length)
    );
    if (!EXPECTED_ARTIFACT_PATHS.includes(relativePath)) {
      return notFound(response);
    }
    path = resolveSafe(context.artifactRoot, relativePath);
  } else if (requestUrl.pathname === "/vendor/tokenizers.mjs") {
    path = join(
      context.packageRoot,
      "node_modules/@huggingface/tokenizers/dist/tokenizers.mjs"
    );
  } else if (requestUrl.pathname === "/vendor/ort.webgpu.bundle.min.mjs") {
    path = join(
      context.packageRoot,
      "node_modules/onnxruntime-web/dist/ort.webgpu.bundle.min.mjs"
    );
  } else if (requestUrl.pathname === "/vendor/ort.wasm.bundle.min.mjs") {
    path = join(
      context.packageRoot,
      "node_modules/onnxruntime-web/dist/ort.wasm.bundle.min.mjs"
    );
  } else {
    return notFound(response);
  }
  let body = await readFile(path);
  if (rewrite) {
    body = Buffer.from(rewriteBuiltModule(body.toString("utf8")), "utf8");
  }
  response.writeHead(200, {
    "content-type": contentType(path),
    "content-length": body.byteLength,
    "cache-control": "no-store",
    "content-security-policy":
      "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'"
  });
  response.end(body);
}

function notFound(response) {
  response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
  response.end("Not found");
}

function contentType(path) {
  if (path.endsWith(".html")) return "text/html; charset=utf-8";
  if (path.endsWith(".js") || path.endsWith(".mjs")) {
    return "text/javascript; charset=utf-8";
  }
  if (path.endsWith(".wasm")) return "application/wasm";
  if (path.endsWith(".json")) return "application/json";
  return "application/octet-stream";
}

function resolveSafe(root, untrustedPath) {
  const resolvedRoot = resolve(root);
  const path = resolve(resolvedRoot, untrustedPath);
  const pathRelative = relative(resolvedRoot, path);
  if (
    pathRelative === "" ||
    pathRelative.startsWith(`..${sep}`) ||
    pathRelative === ".." ||
    isAbsolute(pathRelative)
  ) {
    throw new WebGpuQualificationError(`Unsafe path '${untrustedPath}'.`);
  }
  return path;
}

async function launchChrome(
  executable,
  userDataDirectory,
  pageUrl,
  timeoutMs
) {
  const child = spawn(
    executable,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDirectory}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-default-apps",
      "--disable-domain-reliability",
      "--disable-sync",
      "--metrics-recording-only",
      "--enable-unsafe-webgpu",
      "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1, EXCLUDE localhost",
      "about:blank"
    ],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  let stderr = "";
  const webSocketUrl = await new Promise((resolveUrl, rejectUrl) => {
    const timer = setTimeout(() => {
      rejectUrl(
        new WebGpuQualificationError(
          `Timed out waiting for Chromium DevTools. ${stderr.slice(-2000)}`
        )
      );
    }, Math.min(timeoutMs, 30_000));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      stderr = `${stderr}${chunk}`.slice(-20_000);
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match !== null) {
        clearTimeout(timer);
        resolveUrl(match[1]);
      }
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      rejectUrl(
        new WebGpuQualificationError(
          `Chromium exited before DevTools was ready (${String(code)}/${String(signal)}). ${stderr.slice(-2000)}`
        )
      );
    });
  });
  void pageUrl;
  return { process: child, webSocketUrl };
}

class CdpConnection {
  #socket;
  #nextId = 1;
  #pending = new Map();

  static async connect(url) {
    const socket = new WebSocket(url);
    await new Promise((resolveOpen, rejectOpen) => {
      socket.addEventListener("open", resolveOpen, { once: true });
      socket.addEventListener(
        "error",
        () => rejectOpen(new WebGpuQualificationError("Cannot connect to Chromium DevTools.")),
        { once: true }
      );
    });
    return new CdpConnection(socket);
  }

  constructor(socket) {
    this.#socket = socket;
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id === undefined) return;
      const pending = this.#pending.get(message.id);
      if (pending === undefined) return;
      this.#pending.delete(message.id);
      if (message.error !== undefined) {
        pending.reject(
          new WebGpuQualificationError(
            `DevTools ${pending.method} failed: ${message.error.message}`
          )
        );
      } else {
        pending.resolve(message.result);
      }
    });
  }

  call(method, params = {}, sessionId = undefined) {
    const id = this.#nextId;
    this.#nextId += 1;
    return new Promise((resolveCall, rejectCall) => {
      this.#pending.set(id, { resolve: resolveCall, reject: rejectCall, method });
      this.#socket.send(
        JSON.stringify({
          id,
          method,
          params,
          ...(sessionId === undefined ? {} : { sessionId })
        })
      );
    });
  }

  close() {
    this.#socket.close();
  }
}

async function pollWorkerResult(cdp, sessionId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const evaluation = await cdp.call(
      "Runtime.evaluate",
      {
        expression: "globalThis.__qualificationResult",
        returnByValue: true
      },
      sessionId
    );
    const value = evaluation.result?.value;
    if (value !== null && value !== undefined) return value;
    await delay(250);
  }
  throw new WebGpuQualificationError(
    `Timed out after ${timeoutMs} ms waiting for Chromium WebGPU Worker result.`
  );
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function waitForExit(child, timeoutMs) {
  return new Promise((resolveExit, rejectExit) => {
    if (child.exitCode !== null || child.signalCode !== null) {
      resolveExit();
      return;
    }
    const timer = setTimeout(
      () => rejectExit(new WebGpuQualificationError("Chromium did not exit.")),
      timeoutMs
    );
    child.once("exit", () => {
      clearTimeout(timer);
      resolveExit();
    });
  });
}

function isFinitePositive(value) {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

async function writeJsonAtomically(path, document) {
  await mkdir(dirname(path), { recursive: true });
  const temporary = join(
    dirname(path),
    `.${basename(path)}.${process.pid}.temporary`
  );
  await writeFile(temporary, `${JSON.stringify(document, null, 2)}\n`, {
    encoding: "utf8",
    flag: "wx"
  });
  await rename(temporary, path);
}

function formatErrorChain(error) {
  const messages = [];
  const seen = new Set();
  let current = error;
  while (current !== undefined && current !== null && !seen.has(current)) {
    seen.add(current);
    messages.push(current instanceof Error ? current.message : String(current));
    current =
      current instanceof Error && "cause" in current ? current.cause : undefined;
  }
  return messages.join(" Caused by: ");
}

async function main() {
  try {
    const options = parseArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(HELP);
      return 0;
    }
    await run(options);
    return 0;
  } catch (error) {
    process.stderr.write(
      `Chromium WebGPU qualification failed: ${formatErrorChain(error)}\n`
    );
    return 2;
  }
}

if (
  process.argv[1] !== undefined &&
  resolve(process.argv[1]) === resolve(SCRIPT_PATH)
) {
  process.exitCode = await main();
}
