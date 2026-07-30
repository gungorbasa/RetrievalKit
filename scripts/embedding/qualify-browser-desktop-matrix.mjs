#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  writeFile
} from "node:fs/promises";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPOSITORY_ROOT = resolve(dirname(SCRIPT_PATH), "../..");
const DEFAULT_EMBEDDING_DIST = join(
  REPOSITORY_ROOT,
  "wrappers/browser-embedding/dist"
);
const DEFAULT_RETRIEVAL_DIST = join(REPOSITORY_ROOT, "wrappers/browser/dist");
const DEFAULT_GENERATED_ROOT = join(
  REPOSITORY_ROOT,
  "target/browser-desktop-qualification"
);
const BROWSERS = Object.freeze(["chrome", "firefox", "safari"]);
const WARMUPS = 50;
const MEASURED = 750;
const DIMENSION = 384;

const HELP = `Usage:
  node scripts/embedding/qualify-browser-desktop-matrix.mjs \\
    --artifacts PATH --output PATH [options]

Runs the browser embedding package and the actual RetrievalKit WASM package in
separate dedicated module Workers. Each automated browser exercises real
  CacheStorage, local-only behavior, concurrent prefetch, corruption recovery,
  interrupted-acquisition cleanup, Unicode, 256-token truncation, lifecycle
  errors, provider selection, and a
same-page embedding + I8 vector-retrieval benchmark (50 warmups/750 measured).

Required:
  --artifacts PATH       Frozen six-file MiniLM artifact tree.
  --output PATH          Matrix JSON output.

Options:
  --browsers LIST        Comma-separated chrome,firefox,safari (default: all).
  --execution VALUE      auto, webgpu, or wasm (default: auto).
  --embedding-dist PATH  Built browser embedding dist directory.
  --retrieval-dist PATH  Built browser retrieval dist directory.
  --generated-root PATH  Root containing portable/ and simd128/ wasm-bindgen
                         web outputs (default: ${DEFAULT_GENERATED_ROOT}).
  --chunks INTEGER       I8 benchmark corpus size (default: 50000, minimum: 256).
  --timeout-ms INTEGER   Per-browser timeout (default: 900000).
  --require-all          Fail if any requested browser cannot be automated.
  --chrome PATH          Chrome executable override.
  --geckodriver PATH     geckodriver executable override.
  --safaridriver PATH    safaridriver executable override.
  -h, --help             Show this help.

Firefox requires geckodriver. Safari uses Apple's safaridriver and requires
Remote Automation to have been enabled by the machine owner. Missing or
disabled drivers are reported as explicit skipped gates unless --require-all
is supplied.
`;

const DEFAULT_EXECUTABLES = Object.freeze({
  chrome:
    process.platform === "darwin"
      ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
      : "/usr/bin/google-chrome",
  firefox:
    process.platform === "darwin"
      ? "/Applications/Firefox.app/Contents/MacOS/firefox"
      : "/usr/bin/firefox",
  geckodriver: "geckodriver",
  safaridriver:
    process.platform === "darwin"
      ? "/usr/bin/safaridriver"
      : "safaridriver"
});

export class DesktopMatrixError extends Error {}

export function parseArguments(arguments_) {
  if (arguments_.includes("--help") || arguments_.includes("-h")) {
    return { help: true };
  }
  const flags = new Set(["--require-all"]);
  const values = new Set([
    "--artifacts",
    "--output",
    "--browsers",
    "--execution",
    "--embedding-dist",
    "--retrieval-dist",
    "--generated-root",
    "--chunks",
    "--timeout-ms",
    "--chrome",
    "--geckodriver",
    "--safaridriver"
  ]);
  const parsed = new Map();
  for (let index = 0; index < arguments_.length; index += 1) {
    const option = arguments_[index];
    if (flags.has(option)) {
      if (parsed.has(option)) throw new DesktopMatrixError(`Duplicate '${option}'.`);
      parsed.set(option, true);
      continue;
    }
    if (!values.has(option)) throw new DesktopMatrixError(`Unknown option '${option}'.`);
    if (parsed.has(option)) throw new DesktopMatrixError(`Duplicate '${option}'.`);
    const value = arguments_[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new DesktopMatrixError(`Option '${option}' requires a value.`);
    }
    parsed.set(option, value);
    index += 1;
  }
  for (const required of ["--artifacts", "--output"]) {
    if (!parsed.has(required)) {
      throw new DesktopMatrixError(`Missing required option '${required}'.`);
    }
  }
  const browsers = String(parsed.get("--browsers") ?? BROWSERS.join(","))
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  if (
    browsers.length === 0 ||
    new Set(browsers).size !== browsers.length ||
    browsers.some((browser) => !BROWSERS.includes(browser))
  ) {
    throw new DesktopMatrixError(
      "--browsers must be a unique comma-separated subset of chrome,firefox,safari."
    );
  }
  const execution = String(parsed.get("--execution") ?? "auto");
  if (!["auto", "webgpu", "wasm"].includes(execution)) {
    throw new DesktopMatrixError("--execution must be auto, webgpu, or wasm.");
  }
  const timeoutMs = Number(parsed.get("--timeout-ms") ?? 900_000);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 30_000 || timeoutMs > 1_800_000) {
    throw new DesktopMatrixError(
      "--timeout-ms must be an integer from 30000 through 1800000."
    );
  }
  const chunks = Number(parsed.get("--chunks") ?? 50_000);
  if (!Number.isSafeInteger(chunks) || chunks < 256 || chunks > 50_000) {
    throw new DesktopMatrixError("--chunks must be an integer from 256 through 50000.");
  }
  return {
    help: false,
    artifacts: resolve(String(parsed.get("--artifacts"))),
    output: resolve(String(parsed.get("--output"))),
    browsers,
    execution,
    embeddingDist: resolve(
      String(parsed.get("--embedding-dist") ?? DEFAULT_EMBEDDING_DIST)
    ),
    retrievalDist: resolve(
      String(parsed.get("--retrieval-dist") ?? DEFAULT_RETRIEVAL_DIST)
    ),
    generatedRoot: resolve(
      String(parsed.get("--generated-root") ?? DEFAULT_GENERATED_ROOT)
    ),
    chunks,
    timeoutMs,
    requireAll: parsed.has("--require-all"),
    chrome: parsed.has("--chrome") ? resolve(String(parsed.get("--chrome"))) : undefined,
    geckodriver: parsed.has("--geckodriver")
      ? resolve(String(parsed.get("--geckodriver")))
      : undefined,
    safaridriver: parsed.has("--safaridriver")
      ? resolve(String(parsed.get("--safaridriver")))
      : undefined
  };
}

export function rewriteEmbeddingModule(source) {
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

export function validateBrowserResult(result) {
  if (result === null || typeof result !== "object" || Array.isArray(result)) {
    throw new DesktopMatrixError("Browser result must be an object.");
  }
  if (result.ok !== true) {
    throw new DesktopMatrixError(String(result.error ?? "Browser qualification failed."));
  }
  const benchmark = result.benchmark;
  const checks = result.checks;
  if (
    !["webgpu", "wasm"].includes(result.provider) ||
    !["portable", "simd128"].includes(result.retrieval_tier) ||
    result.dimension !== DIMENSION ||
    !Number.isSafeInteger(result.corpus_chunks) ||
    result.corpus_chunks < 256 ||
    result.corpus_chunks > 50_000 ||
    result.finite !== true ||
    result.normalized !== true ||
    checks?.dedicated_workers !== true ||
    checks?.local_only_missing_rejected !== true ||
    checks?.concurrent_prefetch_deduplicated !== true ||
    checks?.interrupted_acquisition_cleaned !== true ||
    result.artifact_requests !== 6 ||
    result.input_tokens !== 32 ||
    checks?.cached_local_only_load !== true ||
    checks?.corruption_rejected_and_recovered !== true ||
    checks?.unicode !== true ||
    checks?.truncation_256 !== true ||
    checks?.empty_input_rejected !== true ||
    checks?.lifecycle_after_close_rejected !== true ||
    benchmark?.warmups !== WARMUPS ||
    benchmark?.measured !== MEASURED ||
    !finitePositive(benchmark?.cached_initialization_ms) ||
    !finitePositive(benchmark?.first_inference_ms) ||
    !finitePositive(benchmark?.ingestion_ms) ||
    !Array.isArray(benchmark?.embedding_samples_ms) ||
    benchmark.embedding_samples_ms.length !== MEASURED ||
    benchmark.embedding_samples_ms.some((value) => !finitePositive(value)) ||
    !Array.isArray(benchmark?.end_to_end_samples_ms) ||
    benchmark.end_to_end_samples_ms.length !== MEASURED ||
    benchmark.end_to_end_samples_ms.some((value) => !finitePositive(value)) ||
    !Array.isArray(benchmark?.retrieval_samples_ms) ||
    benchmark.retrieval_samples_ms.length !== MEASURED ||
    benchmark.retrieval_samples_ms.some((value) => !finitePositive(value))
  ) {
    throw new DesktopMatrixError(
      "Browser result does not satisfy the desktop qualification contract."
    );
  }
  return result;
}

export function percentileSummary(samples) {
  if (!Array.isArray(samples) || samples.length === 0) {
    throw new DesktopMatrixError("Cannot summarize an empty sample set.");
  }
  const sorted = [...samples].sort((left, right) => left - right);
  const at = (fraction) =>
    sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
  return {
    minimum: sorted[0],
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
    p50: at(0.5),
    p95: at(0.95),
    p99: at(0.99),
    maximum: sorted.at(-1)
  };
}

export function embeddingWorkerSource() {
  return `import { installBrowserEmbeddingWorker } from "/embedding/worker.js";
import { PINNED_ARTIFACTS } from "/embedding/constants.js";

const paths = new Map(PINNED_ARTIFACTS.map((artifact) => [artifact.url, artifact.path]));
let artifactFetches = 0;
const fetcher = async (url, signal) => {
  const path = paths.get(url);
  if (path === undefined) throw new Error("Unexpected artifact URL: " + url);
  artifactFetches += 1;
  const response = await fetch("/artifacts/" + encodeURIComponent(path), {
    cache: "no-store",
    signal
  });
  if (!response.ok) throw new Error("Local artifact HTTP " + response.status);
  return new Uint8Array(await response.arrayBuffer());
};
installBrowserEmbeddingWorker({ fetcher });
self.__retrievalkitQualification = {
  dedicated: typeof DedicatedWorkerGlobalScope !== "undefined" &&
    self instanceof DedicatedWorkerGlobalScope,
  fetches: () => artifactFetches
};
`;
}

export function retrievalWorkerSource() {
  return `import { installRetrievalKitWorker } from "/retrieval/worker.js";
import { createAdaptiveGeneratedWasmAdapter } from "/retrieval/generated-adapter.js";

async function load(path) {
  const generated = await import(path + "/retrievalkit_wasm.js");
  await generated.default(path + "/retrievalkit_wasm_bg.wasm");
  return generated;
}
installRetrievalKitWorker(createAdaptiveGeneratedWasmAdapter({
  portable: () => load("/generated/portable"),
  simd128: () => load("/generated/simd128")
}));
`;
}

export function harnessPageSource(execution, chunks = 50_000) {
  return `import { BrowserEmbedder } from "/embedding/index.js";
import { PinnedMiniLmTokenizer } from "/embedding/tokenizer.js";
import { RetrievalKitBrowser } from "/retrieval/index.js";

const EXECUTION = ${JSON.stringify(execution)};
const DIMENSION = ${DIMENSION};
const WARMUPS = ${WARMUPS};
const MEASURED = ${MEASURED};
const CHUNKS = ${chunks};
const cachePrefix = "retrievalkit-desktop-" + crypto.randomUUID();
const embeddingWorker = () => new Worker("/embedding-worker.mjs", { type: "module" });
const retrievalWorker = () => new Worker("/retrieval-worker.mjs", { type: "module" });

globalThis.__qualificationResult = null;
void qualify().then(
  (value) => { globalThis.__qualificationResult = { ok: true, ...value }; },
  (error) => {
    globalThis.__qualificationResult = {
      ok: false,
      error: formatError(error)
    };
  }
);

async function qualify() {
  if (!("caches" in globalThis)) throw new Error("CacheStorage is unavailable.");
  const interruptedCache = cachePrefix + "-interrupted";
  await fetch("/qualification-delay?ms=150", { method: "POST" });
  const interruption = new AbortController();
  const interruptedPrefetch = BrowserEmbedder.prefetch({
    worker: embeddingWorker,
    cacheName: interruptedCache,
    signal: interruption.signal
  });
  setTimeout(() => interruption.abort(), 20);
  let interruptionRejected = false;
  try { await interruptedPrefetch; } catch { interruptionRejected = true; }
  await fetch("/qualification-delay?ms=0", { method: "POST" });
  const interruptedKeys = await (await caches.open(interruptedCache)).keys();
  const interruptedAcquisitionCleaned =
    interruptionRejected && interruptedKeys.length === 0;
  await caches.delete(interruptedCache);

  await fetch("/qualification-metrics", { method: "DELETE" });
  const missingCache = cachePrefix + "-missing";
  let localOnlyMissingRejected = false;
  try {
    await BrowserEmbedder.load({
      worker: embeddingWorker,
      cacheName: missingCache,
      localOnly: true,
      execution: EXECUTION
    });
  } catch {
    localOnlyMissingRejected = true;
  }

  const sharedCache = cachePrefix + "-shared";
  await Promise.all([
    BrowserEmbedder.prefetch({ worker: embeddingWorker, cacheName: sharedCache }),
    BrowserEmbedder.prefetch({ worker: embeddingWorker, cacheName: sharedCache })
  ]);
  const artifactRequests = Number(
    (await (await fetch("/qualification-metrics", { cache: "no-store" })).json())
      .artifact_requests
  );
  const cache = await caches.open(sharedCache);
  const initialKeys = await cache.keys();
  const concurrentPrefetchDeduplicated =
    initialKeys.length === 7 && artifactRequests === 6;

  const queryText = "local ".repeat(30).trim();
  const tokenizer = new PinnedMiniLmTokenizer(
    new Uint8Array(
      await (
        await fetch("/artifacts/" + encodeURIComponent("tokenizer/tokenizer.json"))
      ).arrayBuffer()
    ),
    new Uint8Array(
      await (
        await fetch(
          "/artifacts/" + encodeURIComponent("tokenizer/tokenizer_config.json")
        )
      ).arrayBuffer()
    )
  );
  const benchmarkEncoding = tokenizer.tokenize([queryText]);
  if (
    benchmarkEncoding.batchSize !== 1 ||
    benchmarkEncoding.sequenceLength !== 32
  ) {
    throw new Error(
      "Benchmark input tokenized to " + benchmarkEncoding.sequenceLength +
      " tokens; expected 32."
    );
  }

  const initializationStart = performance.now();
  let embedder = await BrowserEmbedder.load({
    worker: embeddingWorker,
    cacheName: sharedCache,
    localOnly: true,
    execution: EXECUTION
  });
  const cachedInitializationMs = performance.now() - initializationStart;
  const provider = embedder.provider;
  if (EXECUTION !== "auto" && provider !== EXECUTION) {
    throw new Error("Strict provider mismatch: requested " + EXECUTION + ", got " + provider);
  }

  const firstInferenceStart = performance.now();
  validateVector(await embedder.embed(queryText));
  const firstInferenceMs = performance.now() - firstInferenceStart;
  const unicode = await embedder.embed("Merhaba İstanbul — こんにちは世界 🌍");
  validateVector(unicode);
  const longText = "local ".repeat(400).trim();
  const exactlyTruncated = "local ".repeat(254).trim();
  const longVector = await embedder.embed(longText);
  const truncatedVector = await embedder.embed(exactlyTruncated);
  const truncation256 = cosine(longVector, truncatedVector) > 0.999999;

  let emptyRejected = false;
  try { await embedder.embed(""); } catch { emptyRejected = true; }
  await embedder.close();
  let closedRejected = false;
  try { await embedder.embed("closed"); } catch { closedRejected = true; }

  const corruptible = initialKeys.find((request) =>
    decodeURIComponent(new URL(request.url).pathname).includes("all-MiniLM-L6-v2-fp32.onnx")
  );
  if (corruptible === undefined) throw new Error("Cannot find cached ONNX artifact.");
  await cache.put(corruptible, new Response(new Uint8Array([1, 2, 3])));
  let corruptionRejected = false;
  try {
    await BrowserEmbedder.load({
      worker: embeddingWorker,
      cacheName: sharedCache,
      localOnly: true,
      execution: EXECUTION
    });
  } catch {
    corruptionRejected = true;
  }
  await BrowserEmbedder.prefetch({ worker: embeddingWorker, cacheName: sharedCache });
  embedder = await BrowserEmbedder.load({
    worker: embeddingWorker,
    cacheName: sharedCache,
    localOnly: true,
    execution: EXECUTION
  });
  const query = await embedder.embed(queryText);
  validateVector(query);

  const kit = await RetrievalKitBrowser.create({ worker: retrievalWorker });
  const builder = kit.retrievalDatabase({
    corpusId: "desktop-browser-matrix",
    metric: "cosine",
    encoding: "i8"
  });
  const documents = [];
  for (let index = 0; index < CHUNKS; index += 1) {
    const vector = new Float32Array(query);
    vector[index % DIMENSION] += index * 0.00001;
    documents.push({
      id: "document-" + index,
      text: index === 0 ? queryText : "local document " + index,
      embedding: vector
    });
  }
  const ingestionStart = performance.now();
  await builder.add(documents);
  const database = await builder.build();
  const ingestionMs = performance.now() - ingestionStart;

  let expectedResultIds;
  const search = async (vector) => {
    const results = await database.search({
      mode: "vector",
      embedding: vector,
      limit: 10
    });
    const ids = results.map((result) => result.documentId);
    if (
      results.length !== 10 ||
      ids.some((id) => !id.startsWith("document-")) ||
      (expectedResultIds !== undefined &&
        ids.some((id, index) => id !== expectedResultIds[index]))
    ) {
      throw new Error("Actual I8 retrieval returned unexpected results.");
    }
    expectedResultIds ??= ids;
  };
  await search(query);
  for (let index = 0; index < WARMUPS; index += 1) {
    await search(await embedder.embed(queryText));
  }
  const endToEnd = [];
  const embedding = [];
  const retrieval = [];
  for (let index = 0; index < MEASURED; index += 1) {
    const totalStart = performance.now();
    const vector = await embedder.embed(queryText);
    const embeddingEnd = performance.now();
    const retrievalStart = performance.now();
    await search(vector);
    const end = performance.now();
    retrieval.push(end - retrievalStart);
    embedding.push(embeddingEnd - totalStart);
    endToEnd.push(end - totalStart);
  }

  await database.close();
  kit.close();
  await embedder.close();
  await Promise.all([
    caches.delete(missingCache),
    caches.delete(sharedCache)
  ]);

  return {
    provider,
    retrieval_tier: kit.capabilities.performanceTier,
    dimension: DIMENSION,
    finite: true,
    normalized: true,
    corpus_chunks: CHUNKS,
    artifact_requests: artifactRequests,
    input_tokens: 32,
    user_agent: navigator.userAgent,
    checks: {
      dedicated_workers: true,
      local_only_missing_rejected: localOnlyMissingRejected,
      concurrent_prefetch_deduplicated: concurrentPrefetchDeduplicated,
      interrupted_acquisition_cleaned: interruptedAcquisitionCleaned,
      cached_local_only_load: true,
      corruption_rejected_and_recovered: corruptionRejected,
      unicode: true,
      truncation_256: truncation256,
      empty_input_rejected: emptyRejected,
      lifecycle_after_close_rejected: closedRejected
    },
    benchmark: {
      warmups: WARMUPS,
      measured: MEASURED,
      cached_initialization_ms: cachedInitializationMs,
      first_inference_ms: firstInferenceMs,
      ingestion_ms: ingestionMs,
      embedding_samples_ms: embedding,
      end_to_end_samples_ms: endToEnd,
      retrieval_samples_ms: retrieval
    }
  };
}

function validateVector(vector) {
  if (!(vector instanceof Float32Array) || vector.length !== DIMENSION) {
    throw new Error("Expected a 384-value Float32Array.");
  }
  let squared = 0;
  for (const value of vector) {
    if (!Number.isFinite(value)) throw new Error("Embedding is not finite.");
    squared += value * value;
  }
  if (Math.abs(Math.sqrt(squared) - 1) > 0.0001) {
    throw new Error("Embedding is not L2-normalized.");
  }
}

function cosine(left, right) {
  let dot = 0;
  let leftSquared = 0;
  let rightSquared = 0;
  for (let index = 0; index < left.length; index += 1) {
    dot += left[index] * right[index];
    leftSquared += left[index] * left[index];
    rightSquared += right[index] * right[index];
  }
  return dot / Math.sqrt(leftSquared * rightSquared);
}

function formatError(error) {
  const messages = [];
  const seen = new Set();
  let current = error;
  while (current != null && !seen.has(current)) {
    seen.add(current);
    messages.push(current instanceof Error ? current.message : String(current));
    current = current instanceof Error ? current.cause : undefined;
  }
  return messages.join(" Caused by: ");
}
`;
}

async function run(options) {
  await verifyInputs(options);
  const temporaryRoot = await mkdtemp(
    join(await ensureDirectory(join(REPOSITORY_ROOT, "target")), "browser-matrix-")
  );
  const generated = join(temporaryRoot, "generated");
  await mkdir(generated, { recursive: true });
  await Promise.all([
    writeFile(
      join(generated, "index.html"),
      '<!doctype html><meta charset="utf-8"><title>RetrievalKit desktop browser qualification</title><script type="module" src="/harness.mjs"></script>\n'
    ),
    writeFile(
      join(generated, "harness.mjs"),
      harnessPageSource(options.execution, options.chunks)
    ),
    writeFile(join(generated, "embedding-worker.mjs"), embeddingWorkerSource()),
    writeFile(join(generated, "retrieval-worker.mjs"), retrievalWorkerSource())
  ]);

  let server;
  try {
    server = await startServer({ ...options, generated });
    const address = server.address();
    if (address === null || typeof address === "string") {
      throw new DesktopMatrixError("Loopback server has no TCP address.");
    }
    const pageUrl = `http://127.0.0.1:${address.port}/`;
    const results = [];
    for (const browser of options.browsers) {
      try {
        const raw =
          browser === "chrome"
            ? await runChrome(pageUrl, options, temporaryRoot)
            : await runWebDriver(browser, pageUrl, options);
        const fixed = validateBrowserResult(raw);
        results.push({
          browser,
          status: "passed",
          user_agent: fixed.user_agent,
          provider: fixed.provider,
          retrieval_tier: fixed.retrieval_tier,
          corpus_chunks: fixed.corpus_chunks,
          artifact_requests: fixed.artifact_requests,
          input_tokens: fixed.input_tokens,
          checks: fixed.checks,
          benchmark: {
            warmups: WARMUPS,
            measured: MEASURED,
            cached_initialization_ms: fixed.benchmark.cached_initialization_ms,
            first_inference_ms: fixed.benchmark.first_inference_ms,
            ingestion_ms: fixed.benchmark.ingestion_ms,
            embedding_ms: percentileSummary(fixed.benchmark.embedding_samples_ms),
            end_to_end_ms: percentileSummary(
              fixed.benchmark.end_to_end_samples_ms
            ),
            retrieval_ms: percentileSummary(fixed.benchmark.retrieval_samples_ms)
          }
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (isUnavailable(browser, message) && !options.requireAll) {
          results.push({ browser, status: "skipped", reason: message });
        } else {
          results.push({ browser, status: "failed", reason: message });
        }
      }
    }
    const report = {
      schema_version: 1,
      kind: "retrievalkit_browser_desktop_embedding_retrieval_matrix",
      execution_requested: options.execution,
      dedicated_module_workers: true,
      cache_storage: "browser CacheStorage",
      benchmark_contract: {
        warmups: WARMUPS,
        measured: MEASURED,
        dimension: DIMENSION,
        corpus_chunks: options.chunks,
        input_tokens: 32,
        retrieval_encoding: "i8"
      },
      browsers: results
    };
    await writeJsonAtomic(options.output, report);
    const failures = results.filter((result) => result.status === "failed");
    if (failures.length > 0 || (options.requireAll && results.some((r) => r.status !== "passed"))) {
      throw new DesktopMatrixError(
        `Desktop matrix did not pass: ${results
          .filter((result) => result.status !== "passed")
          .map((result) => `${result.browser}: ${result.reason}`)
          .join("; ")}`
      );
    }
    process.stdout.write(`${JSON.stringify(report)}\n`);
  } finally {
    if (server !== undefined) {
      await new Promise((resolveClose) => server.close(resolveClose));
    }
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

async function verifyInputs(options) {
  const requiredArtifacts = [
    "manifest-v1.json",
    "onnx/all-MiniLM-L6-v2-fp32.onnx",
    "tokenizer/tokenizer.json",
    "tokenizer/tokenizer_config.json",
    "tokenizer/special_tokens_map.json",
    "tokenizer/vocab.txt"
  ];
  await Promise.all([
    ...requiredArtifacts.map((path) => access(safeResolve(options.artifacts, path))),
    access(join(options.embeddingDist, "index.js")),
    access(join(options.retrievalDist, "index.js")),
    access(join(options.generatedRoot, "portable/retrievalkit_wasm.js")),
    access(join(options.generatedRoot, "portable/retrievalkit_wasm_bg.wasm")),
    access(join(options.generatedRoot, "simd128/retrievalkit_wasm.js")),
    access(join(options.generatedRoot, "simd128/retrievalkit_wasm_bg.wasm"))
  ]);
}

async function startServer(context) {
  context.artifactRequestCount = 0;
  context.artifactDelayMs = 0;
  const server = createServer((request, response) => {
    void serve(request, response, context).catch((error) => {
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

async function serve(request, response, context) {
  const url = new URL(request.url ?? "/", "http://127.0.0.1");
  let path;
  let rewrite = false;
  if (url.pathname === "/qualification-metrics") {
    if (request.method === "DELETE") context.artifactRequestCount = 0;
    response.writeHead(200, {
      "content-type": "application/json",
      "cache-control": "no-store"
    });
    response.end(
      JSON.stringify({ artifact_requests: context.artifactRequestCount })
    );
    return;
  }
  if (url.pathname === "/qualification-delay") {
    const milliseconds = Number(url.searchParams.get("ms") ?? "0");
    if (
      request.method !== "POST" ||
      !Number.isSafeInteger(milliseconds) ||
      milliseconds < 0 ||
      milliseconds > 1_000
    ) {
      response.writeHead(400);
      response.end("invalid delay");
      return;
    }
    context.artifactDelayMs = milliseconds;
    response.writeHead(204);
    response.end();
    return;
  }
  if (url.pathname === "/") path = join(context.generated, "index.html");
  else if (url.pathname === "/harness.mjs") path = join(context.generated, "harness.mjs");
  else if (url.pathname === "/embedding-worker.mjs") {
    path = join(context.generated, "embedding-worker.mjs");
  } else if (url.pathname === "/retrieval-worker.mjs") {
    path = join(context.generated, "retrieval-worker.mjs");
  } else if (url.pathname.startsWith("/embedding/")) {
    path = safeResolve(context.embeddingDist, url.pathname.slice("/embedding/".length));
    rewrite = path.endsWith(".js");
  } else if (url.pathname.startsWith("/retrieval/")) {
    path = safeResolve(context.retrievalDist, url.pathname.slice("/retrieval/".length));
  } else if (url.pathname.startsWith("/generated/")) {
    path = safeResolve(context.generatedRoot, url.pathname.slice("/generated/".length));
  } else if (url.pathname.startsWith("/artifacts/")) {
    context.artifactRequestCount += 1;
    if (context.artifactDelayMs > 0) await delay(context.artifactDelayMs);
    path = safeResolve(
      context.artifacts,
      decodeURIComponent(url.pathname.slice("/artifacts/".length))
    );
  } else if (url.pathname === "/vendor/tokenizers.mjs") {
    path = resolve(
      context.embeddingDist,
      "../node_modules/@huggingface/tokenizers/dist/tokenizers.mjs"
    );
  } else if (url.pathname === "/vendor/ort.webgpu.bundle.min.mjs") {
    path = resolve(
      context.embeddingDist,
      "../node_modules/onnxruntime-web/dist/ort.webgpu.bundle.min.mjs"
    );
  } else if (url.pathname === "/vendor/ort.wasm.bundle.min.mjs") {
    path = resolve(
      context.embeddingDist,
      "../node_modules/onnxruntime-web/dist/ort.wasm.bundle.min.mjs"
    );
  } else if (url.pathname.startsWith("/runtime/")) {
    path = safeResolve(context.embeddingDist, url.pathname.slice(1));
  } else {
    response.writeHead(404);
    response.end("not found");
    return;
  }
  let body = await readFile(path);
  if (rewrite) body = Buffer.from(rewriteEmbeddingModule(body.toString("utf8")));
  response.writeHead(200, {
    "content-type": contentType(path),
    "cache-control": "no-store",
    "cross-origin-opener-policy": "same-origin",
    "cross-origin-embedder-policy": "require-corp"
  });
  response.end(body);
}

async function runChrome(pageUrl, options, temporaryRoot) {
  const executable = options.chrome ?? DEFAULT_EXECUTABLES.chrome;
  await access(executable).catch(() => {
    throw new DesktopMatrixError(`Chrome executable unavailable: ${executable}`);
  });
  const profile = join(temporaryRoot, "chrome-profile");
  await mkdir(profile, { recursive: true });
  const process_ = spawn(
    executable,
    [
      "--headless=new",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank"
    ],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  try {
    const socketUrl = await chromeSocket(process_, options.timeoutMs);
    const cdp = await CdpConnection.connect(socketUrl);
    try {
      const target = await cdp.call("Target.createTarget", { url: pageUrl });
      const attached = await cdp.call("Target.attachToTarget", {
        targetId: target.targetId,
        flatten: true
      });
      await cdp.call("Runtime.enable", {}, attached.sessionId);
      return await pollResult(
        async () => {
          const evaluated = await cdp.call(
            "Runtime.evaluate",
            {
              expression: "globalThis.__qualificationResult",
              returnByValue: true,
              awaitPromise: true
            },
            attached.sessionId
          );
          return evaluated.result?.value ?? null;
        },
        options.timeoutMs
      );
    } finally {
      cdp.close();
    }
  } finally {
    process_.kill("SIGTERM");
    await waitExit(process_, 5_000).catch(() => process_.kill("SIGKILL"));
  }
}

async function runWebDriver(browser, pageUrl, options) {
  const port = await reservePort();
  const driver =
    browser === "firefox"
      ? options.geckodriver ?? DEFAULT_EXECUTABLES.geckodriver
      : options.safaridriver ?? DEFAULT_EXECUTABLES.safaridriver;
  if (driver.includes(sep)) {
    await access(driver).catch(() => {
      throw new DesktopMatrixError(`${browser} WebDriver unavailable: ${driver}`);
    });
  } else if (!(await commandAvailable(driver))) {
    throw new DesktopMatrixError(`${browser} WebDriver unavailable: ${driver}`);
  }
  const arguments_ =
    browser === "firefox" ? ["--port", String(port)] : ["-p", String(port)];
  const process_ = spawn(driver, arguments_, { stdio: ["ignore", "ignore", "pipe"] });
  let sessionId;
  try {
    await waitWebDriver(port, process_, 10_000);
    const capabilities =
      browser === "firefox"
        ? {
            capabilities: {
              alwaysMatch: {
                browserName: "firefox",
                "moz:firefoxOptions": {
                  binary: DEFAULT_EXECUTABLES.firefox,
                  args: ["-headless"]
                }
              }
            }
          }
        : { capabilities: { alwaysMatch: { browserName: "safari" } } };
    const created = await webdriver(port, "POST", "/session", capabilities);
    sessionId = created.sessionId ?? created.value?.sessionId;
    if (typeof sessionId !== "string") {
      throw new DesktopMatrixError(`${browser} WebDriver did not return a session.`);
    }
    await webdriver(port, "POST", `/session/${sessionId}/url`, { url: pageUrl });
    return await pollResult(async () => {
      const result = await webdriver(
        port,
        "POST",
        `/session/${sessionId}/execute/sync`,
        {
          script: "return globalThis.__qualificationResult;",
          args: []
        }
      );
      return result.value ?? null;
    }, options.timeoutMs);
  } finally {
    if (sessionId !== undefined) {
      await webdriver(port, "DELETE", `/session/${sessionId}`, undefined).catch(
        () => undefined
      );
    }
    process_.kill("SIGTERM");
    await waitExit(process_, 3_000).catch(() => process_.kill("SIGKILL"));
  }
}

async function webdriver(port, method, path, body) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "content-type": "application/json; charset=utf-8" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) })
  });
  const document = await response.json().catch(() => ({}));
  if (!response.ok || document.value?.error !== undefined) {
    throw new DesktopMatrixError(
      String(document.value?.message ?? `WebDriver HTTP ${response.status}`)
    );
  }
  return document;
}

async function waitWebDriver(port, process_, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (process_.exitCode !== null) {
      throw new DesktopMatrixError(`WebDriver exited with ${process_.exitCode}.`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/status`);
      if (response.ok) return;
    } catch {
      // Driver has not bound its socket yet.
    }
    await delay(100);
  }
  throw new DesktopMatrixError("Timed out waiting for WebDriver.");
}

async function pollResult(read, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const result = await read();
    if (result !== null && result !== undefined) return result;
    await delay(250);
  }
  throw new DesktopMatrixError("Timed out waiting for browser qualification.");
}

async function chromeSocket(process_, timeoutMs) {
  const deadline = Date.now() + Math.min(timeoutMs, 30_000);
  let stderr = "";
  process_.stderr.setEncoding("utf8");
  process_.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  while (Date.now() < deadline) {
    const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
    if (match !== null) return match[1];
    if (process_.exitCode !== null) {
      throw new DesktopMatrixError(`Chrome exited with ${process_.exitCode}.`);
    }
    await delay(50);
  }
  throw new DesktopMatrixError("Timed out waiting for Chrome DevTools.");
}

class CdpConnection {
  #socket;
  #nextId = 1;
  #pending = new Map();

  static async connect(url) {
    const socket = new WebSocket(url);
    await new Promise((resolveOpen, rejectOpen) => {
      socket.addEventListener("open", resolveOpen, { once: true });
      socket.addEventListener("error", rejectOpen, { once: true });
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
      if (message.error !== undefined) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }

  call(method, params = {}, sessionId) {
    const id = this.#nextId++;
    return new Promise((resolveCall, rejectCall) => {
      this.#pending.set(id, { resolve: resolveCall, reject: rejectCall });
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
    for (const pending of this.#pending.values()) {
      pending.reject(new Error("CDP connection closed."));
    }
    this.#pending.clear();
  }
}

function safeResolve(root, relativePath) {
  const resolvedRoot = resolve(root);
  const resolvedPath = resolve(resolvedRoot, relativePath);
  if (resolvedPath !== resolvedRoot && !resolvedPath.startsWith(`${resolvedRoot}${sep}`)) {
    throw new DesktopMatrixError(`Unsafe path '${relativePath}'.`);
  }
  return resolvedPath;
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

function finitePositive(value) {
  return Number.isFinite(value) && value > 0;
}

function isUnavailable(browser, message) {
  const lowercase = message.toLowerCase();
  return (
    message.includes("unavailable") ||
    message.includes("not found") ||
    lowercase.includes("remote automation") ||
    (browser === "safari" && message.includes("WebDriver exited"))
  );
}

async function ensureDirectory(path) {
  await mkdir(path, { recursive: true });
  return path;
}

async function writeJsonAtomic(path, document) {
  await mkdir(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  await writeFile(temporary, `${JSON.stringify(document, null, 2)}\n`);
  await import("node:fs/promises").then(({ rename }) => rename(temporary, path));
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new DesktopMatrixError("Cannot reserve a WebDriver port.");
  }
  await new Promise((resolveClose) => server.close(resolveClose));
  return address.port;
}

async function commandAvailable(command) {
  return await new Promise((resolveAvailable) => {
    const child = spawn("sh", ["-c", "command -v \"$1\" >/dev/null 2>&1", "sh", command]);
    child.once("exit", (code) => resolveAvailable(code === 0));
    child.once("error", () => resolveAvailable(false));
  });
}

function waitExit(process_, timeoutMs) {
  if (process_.exitCode !== null) return Promise.resolve();
  return new Promise((resolveExit, rejectExit) => {
    const timer = setTimeout(
      () => rejectExit(new DesktopMatrixError("Process did not exit.")),
      timeoutMs
    );
    process_.once("exit", () => {
      clearTimeout(timer);
      resolveExit();
    });
  });
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === SCRIPT_PATH) {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(HELP);
    } else {
      await run(options);
    }
  } catch (error) {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`
    );
    process.exitCode = 1;
  }
}
