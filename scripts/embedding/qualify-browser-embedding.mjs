#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPOSITORY_ROOT = resolve(dirname(SCRIPT_PATH), "../..");
const DEFAULT_PACKAGE_DIST = join(
  REPOSITORY_ROOT,
  "wrappers/browser-embedding/dist"
);

const SCHEMA_VERSION = 1;
const EXPECTED_ROLE_COUNTS = Object.freeze({
  corpus: 48,
  query: 42,
  diagnostic: 4
});
const EXPECTED_ITEM_COUNT = Object.values(EXPECTED_ROLE_COUNTS).reduce(
  (total, count) => total + count,
  0
);
const DIMENSION = 384;
const WARMUPS = 50;
const MEASURED = 750;
const BENCHMARK_INPUT_TOKENS = 32;
const NORM_TOLERANCE = 1e-4;

const HELP = `Usage:
  node scripts/embedding/qualify-browser-embedding.mjs \\
    --input PATH \\
    --artifacts PATH \\
    --output PATH \\
    --benchmark-output PATH [--package-dist PATH]

Runs the built browser embedding Worker's internal service with actual
onnxruntime-web WASM inference. No network request is allowed: --artifacts must
be the root of the frozen artifact tree containing manifest-v1.json, onnx/, and
tokenizer/.

Required options:
  --input PATH             Versioned 94-item role-aware conformance input.
  --artifacts PATH         Local frozen artifact root; files are still checked
                           by the package's exact size and SHA-256 contract.
  --output PATH            Versioned candidate vectors for
                           validate-python-node-wrapper-conformance.py.
  --benchmark-output PATH  Separate cached-load and 50/750 latency evidence.

Optional:
  --package-dist PATH      Built wrappers/browser-embedding/dist directory.
                           Defaults to ${DEFAULT_PACKAGE_DIST}
  -h, --help               Show this help.

The benchmark uses one batch-one input that the built pinned tokenizer proves
is exactly 32 tokens including BERT special tokens. Latencies use monotonic
performance.now() and nearest-rank percentiles.
`;

export class QualificationInputError extends Error {}

export function parseArguments(arguments_) {
  const argumentsList = [...arguments_];
  if (argumentsList.includes("--help") || argumentsList.includes("-h")) {
    return { help: true };
  }
  const allowed = new Set([
    "--input",
    "--artifacts",
    "--output",
    "--benchmark-output",
    "--package-dist"
  ]);
  const values = new Map();
  for (let index = 0; index < argumentsList.length; index += 1) {
    const option = argumentsList[index];
    if (!allowed.has(option)) {
      throw new QualificationInputError(`Unknown option '${option}'.`);
    }
    if (values.has(option)) {
      throw new QualificationInputError(`Option '${option}' was provided twice.`);
    }
    const value = argumentsList[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new QualificationInputError(`Option '${option}' requires a path.`);
    }
    values.set(option, value);
    index += 1;
  }
  for (const required of [
    "--input",
    "--artifacts",
    "--output",
    "--benchmark-output"
  ]) {
    if (!values.has(required)) {
      throw new QualificationInputError(`Missing required option '${required}'.`);
    }
  }
  return {
    help: false,
    input: resolve(values.get("--input")),
    artifacts: resolve(values.get("--artifacts")),
    output: resolve(values.get("--output")),
    benchmarkOutput: resolve(values.get("--benchmark-output")),
    packageDist: resolve(values.get("--package-dist") ?? DEFAULT_PACKAGE_DIST)
  };
}

export function normalizeQualificationInput(document) {
  assertExactKeys(document, ["items", "schema_version"], "$");
  if (document.schema_version !== SCHEMA_VERSION) {
    throw new QualificationInputError(
      `input.schema_version must equal ${SCHEMA_VERSION}.`
    );
  }
  if (!Array.isArray(document.items) || document.items.length !== EXPECTED_ITEM_COUNT) {
    throw new QualificationInputError(
      `input.items must contain exactly ${EXPECTED_ITEM_COUNT} items.`
    );
  }

  const seen = new Set();
  const roleOffsets = {
    corpus: 0,
    query: EXPECTED_ROLE_COUNTS.corpus,
    diagnostic: EXPECTED_ROLE_COUNTS.corpus + EXPECTED_ROLE_COUNTS.query
  };
  const items = document.items.map((rawItem, index) => {
    assertExactKeys(rawItem, ["id", "role", "text"], `input.items[${index}]`);
    if (
      typeof rawItem.id !== "string" ||
      typeof rawItem.text !== "string" ||
      rawItem.text.trim().length === 0 ||
      !Object.hasOwn(EXPECTED_ROLE_COUNTS, rawItem.role)
    ) {
      throw new QualificationInputError(
        `input.items[${index}] requires non-empty id/text and a frozen role.`
      );
    }
    if (seen.has(rawItem.id)) {
      throw new QualificationInputError(`Duplicate input id '${rawItem.id}'.`);
    }
    seen.add(rawItem.id);

    const role = rawItem.role;
    const roleIndex = index - roleOffsets[role];
    const expectedId =
      roleIndex >= 0 && roleIndex < EXPECTED_ROLE_COUNTS[role]
        ? `${role}-${String(roleIndex).padStart(3, "0")}`
        : undefined;
    if (rawItem.id !== expectedId) {
      throw new QualificationInputError(
        `input.items[${index}].id must equal '${expectedId ?? "<frozen-order-id>"}'.`
      );
    }
    return Object.freeze({
      id: rawItem.id,
      role,
      text: rawItem.text
    });
  });
  return Object.freeze(items);
}

export function nearestRankPercentile(sortedValues, fraction) {
  if (
    !Array.isArray(sortedValues) ||
    sortedValues.length === 0 ||
    !Number.isFinite(fraction) ||
    fraction <= 0 ||
    fraction > 1
  ) {
    throw new QualificationInputError("Percentile input is invalid.");
  }
  const index = Math.min(
    sortedValues.length - 1,
    Math.max(0, Math.ceil(sortedValues.length * fraction) - 1)
  );
  return sortedValues[index];
}

export function validateCandidateDocument(document, inputItems) {
  assertExactKeys(document, ["items", "model", "schema_version"], "$candidate");
  if (document.schema_version !== SCHEMA_VERSION) {
    throw new QualificationInputError("candidate.schema_version must equal 1.");
  }
  assertExactKeys(
    document.model,
    [
      "dimension",
      "dtype",
      "identifier",
      "max_input_tokens",
      "normalized",
      "profile",
      "revision"
    ],
    "candidate.model"
  );
  const expectedModel = expectedModelDocument();
  for (const [key, value] of Object.entries(expectedModel)) {
    if (document.model[key] !== value) {
      throw new QualificationInputError(
        `candidate.model.${key} does not match the frozen contract.`
      );
    }
  }
  if (!Array.isArray(document.items) || document.items.length !== inputItems.length) {
    throw new QualificationInputError("candidate.items count does not match input.");
  }
  for (let row = 0; row < document.items.length; row += 1) {
    const item = document.items[row];
    assertExactKeys(item, ["embedding", "id"], `candidate.items[${row}]`);
    if (item.id !== inputItems[row].id) {
      throw new QualificationInputError(
        `candidate.items[${row}].id does not preserve input order.`
      );
    }
    validateVector(item.embedding, `candidate.items[${row}].embedding`);
  }
}

async function run(options) {
  if (options.output === options.benchmarkOutput) {
    throw new QualificationInputError(
      "--output and --benchmark-output must be different paths."
    );
  }
  const input = parseJsonStrict(await readFile(options.input, "utf8"), "input");
  const inputItems = normalizeQualificationInput(input);
  const internals = await importBuiltInternals(options.packageDist);

  const artifacts = internals.PINNED_ARTIFACTS;
  if (!Array.isArray(artifacts) || artifacts.length !== 6) {
    throw new QualificationInputError(
      "Built browser package does not expose the frozen six-file artifact contract."
    );
  }
  const fetcher = localArtifactFetcher(options.artifacts, artifacts);
  const store = new internals.MemoryArtifactStore(
    "memory:browser-embedding-live-qualification"
  );
  const dependencies = {
    artifacts,
    fetcher,
    createStore: () => store
  };

  const prepareService = new internals.EmbeddingWorkerService(dependencies);
  const preparationStart = performance.now();
  await prepareService.prefetch({ localOnly: false });
  const artifactPreparationMs = performance.now() - preparationStart;
  await prepareService.close();

  const service = new internals.EmbeddingWorkerService(dependencies);
  const initializationStart = performance.now();
  await service.load({ localOnly: true, execution: "wasm" });
  const cachedInitializationMs = performance.now() - initializationStart;
  if (service.provider !== "wasm") {
    throw new QualificationInputError(
      `Qualification requires WASM, but service selected '${service.provider}'.`
    );
  }

  const benchmarkText = "local ".repeat(BENCHMARK_INPUT_TOKENS - 2).trim();
  const tokenizer = new internals.PinnedMiniLmTokenizer(
    await readArtifact(options.artifacts, "tokenizer/tokenizer.json"),
    await readArtifact(options.artifacts, "tokenizer/tokenizer_config.json")
  );
  const benchmarkEncoding = tokenizer.tokenize([benchmarkText]);
  if (
    benchmarkEncoding.batchSize !== 1 ||
    benchmarkEncoding.sequenceLength !== BENCHMARK_INPUT_TOKENS
  ) {
    throw new QualificationInputError(
      `Benchmark input tokenized to ${benchmarkEncoding.sequenceLength}; expected ${BENCHMARK_INPUT_TOKENS}.`
    );
  }

  const firstStart = performance.now();
  validateRuntimeVector(await service.embed(benchmarkText), "first inference");
  const firstInferenceMs = performance.now() - firstStart;

  for (let index = 0; index < WARMUPS; index += 1) {
    validateRuntimeVector(await service.embed(benchmarkText), `warmup ${index}`);
  }
  const samples = [];
  for (let index = 0; index < MEASURED; index += 1) {
    const start = performance.now();
    const vector = await service.embed(benchmarkText);
    samples.push(performance.now() - start);
    validateRuntimeVector(vector, `measurement ${index}`);
  }
  samples.sort((left, right) => left - right);

  const texts = inputItems.map((item) => item.text);
  const vectors = await service.embedBatch(texts);
  if (vectors.length !== inputItems.length * DIMENSION) {
    throw new QualificationInputError(
      `Conformance output has ${vectors.length} values; expected ${inputItems.length * DIMENSION}.`
    );
  }

  const candidate = {
    schema_version: SCHEMA_VERSION,
    model: expectedModelDocument(),
    items: inputItems.map((item, index) => ({
      id: item.id,
      embedding: Array.from(
        vectors.subarray(index * DIMENSION, (index + 1) * DIMENSION)
      )
    }))
  };
  validateCandidateDocument(candidate, inputItems);

  const mean =
    samples.reduce((total, value) => total + value, 0) / samples.length;
  const benchmark = {
    schema_version: SCHEMA_VERSION,
    kind: "retrievalkit_browser_embedding_live_qualification",
    model: expectedModelDocument(),
    runtime: {
      package: "onnxruntime-web",
      version: "1.27.0",
      execution_provider: service.provider,
      worker_service: true,
      artifact_source: "local_cli_path",
      artifact_network_requests: 0
    },
    benchmark: {
      input_tokens: BENCHMARK_INPUT_TOKENS,
      batch_size: 1,
      warmups: WARMUPS,
      measured: MEASURED,
      artifact_preparation_ms: artifactPreparationMs,
      cached_initialization_ms: cachedInitializationMs,
      first_inference_ms: firstInferenceMs,
      warm_inference_ms: {
        minimum: samples[0],
        mean,
        p50: nearestRankPercentile(samples, 0.5),
        p95: nearestRankPercentile(samples, 0.95),
        p99: nearestRankPercentile(samples, 0.99),
        maximum: samples.at(-1)
      }
    },
    input: {
      conformance_items: inputItems.length,
      benchmark_text_sha256: createHash("sha256")
        .update(benchmarkText, "utf8")
        .digest("hex")
    }
  };

  await service.close();
  await writeJsonAtomically(options.output, candidate);
  await writeJsonAtomically(options.benchmarkOutput, benchmark);
  process.stdout.write(
    `${JSON.stringify({
      candidate: options.output,
      benchmark: options.benchmarkOutput,
      provider: benchmark.runtime.execution_provider,
      cached_initialization_ms: cachedInitializationMs,
      first_inference_ms: firstInferenceMs,
      warm_p95_ms: benchmark.benchmark.warm_inference_ms.p95
    })}\n`
  );
}

async function importBuiltInternals(packageDist) {
  let packageDocument;
  try {
    packageDocument = parseJsonStrict(
      await readFile(resolve(packageDist, "../package.json"), "utf8"),
      "browser embedding package.json"
    );
  } catch (error) {
    throw new QualificationInputError(
      `Cannot verify the built browser embedding package metadata: ${error instanceof Error ? error.message : String(error)}`
    );
  }
  const expectedDependencies = {
    "@huggingface/tokenizers": "0.1.3",
    "onnxruntime-web": "1.27.0"
  };
  for (const [name, version] of Object.entries(expectedDependencies)) {
    if (packageDocument.dependencies?.[name] !== version) {
      throw new QualificationInputError(
        `Built browser package must pin ${name} exactly to ${version}.`
      );
    }
  }

  const modules = {};
  for (const name of ["constants", "service", "store", "tokenizer"]) {
    const path = join(packageDist, `${name}.js`);
    try {
      modules[name] = await import(pathToFileURL(path).href);
    } catch (error) {
      throw new QualificationInputError(
        `Cannot import built browser embedding internal '${path}'. Run the package build first. ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }
  const internals = {
    PINNED_ARTIFACTS: modules.constants.PINNED_ARTIFACTS,
    EmbeddingWorkerService: modules.service.EmbeddingWorkerService,
    MemoryArtifactStore: modules.store.MemoryArtifactStore,
    PinnedMiniLmTokenizer: modules.tokenizer.PinnedMiniLmTokenizer
  };
  for (const [name, value] of Object.entries(internals)) {
    if (value === undefined) {
      throw new QualificationInputError(
        `Built browser embedding package is missing internal export '${name}'.`
      );
    }
  }
  return internals;
}

function localArtifactFetcher(artifactRoot, artifacts) {
  const byUrl = new Map(artifacts.map((artifact) => [artifact.url, artifact.path]));
  return async (url, signal) => {
    if (signal?.aborted === true) {
      throw signal.reason instanceof Error
        ? signal.reason
        : new QualificationInputError("Local artifact read was cancelled.");
    }
    const artifactPath = byUrl.get(url);
    if (artifactPath === undefined) {
      throw new QualificationInputError(`Unexpected artifact URL '${url}'.`);
    }
    return await readArtifact(artifactRoot, artifactPath);
  };
}

async function readArtifact(root, artifactPath) {
  const resolvedRoot = resolve(root);
  const path = resolve(resolvedRoot, artifactPath);
  const pathRelative = relative(resolvedRoot, path);
  if (
    pathRelative === "" ||
    pathRelative.startsWith("..") ||
    isAbsolute(pathRelative)
  ) {
    throw new QualificationInputError(
      `Unsafe or invalid artifact path '${artifactPath}'.`
    );
  }
  try {
    const bytes = await readFile(path);
    return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  } catch (error) {
    throw new QualificationInputError(
      `Cannot read frozen artifact '${path}': ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

function expectedModelDocument() {
  return {
    identifier: "sentence-transformers/all-MiniLM-L6-v2",
    revision: "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
    profile: "fp32",
    dtype: "float32",
    dimension: DIMENSION,
    max_input_tokens: 256,
    normalized: true
  };
}

function validateRuntimeVector(vector, label) {
  if (!(vector instanceof Float32Array)) {
    throw new QualificationInputError(`${label} did not return Float32Array.`);
  }
  validateVector([...vector], label);
}

function validateVector(vector, path) {
  if (!Array.isArray(vector) || vector.length !== DIMENSION) {
    throw new QualificationInputError(
      `${path} must contain exactly ${DIMENSION} values.`
    );
  }
  let squaredNorm = 0;
  for (let index = 0; index < vector.length; index += 1) {
    const value = vector[index];
    if (typeof value !== "number" || !Number.isFinite(value)) {
      throw new QualificationInputError(`${path}[${index}] must be finite.`);
    }
    const float32 = Math.fround(value);
    if (!Number.isFinite(float32)) {
      throw new QualificationInputError(
        `${path}[${index}] is not float32-representable.`
      );
    }
    squaredNorm += float32 * float32;
  }
  const norm = Math.sqrt(squaredNorm);
  if (Math.abs(norm - 1) > NORM_TOLERANCE) {
    throw new QualificationInputError(
      `${path} L2 norm ${norm} is outside tolerance ${NORM_TOLERANCE}.`
    );
  }
}

function assertExactKeys(value, keys, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new QualificationInputError(`${path} must be an object.`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new QualificationInputError(
      `${path} keys must be exactly ${expected.join(", ")}.`
    );
  }
}

function parseJsonStrict(text, label) {
  try {
    const parsed = JSON.parse(text);
    rejectNonFinite(parsed, label);
    return parsed;
  } catch (error) {
    if (error instanceof QualificationInputError) throw error;
    throw new QualificationInputError(
      `${label} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

function rejectNonFinite(value, path) {
  if (typeof value === "number" && !Number.isFinite(value)) {
    throw new QualificationInputError(`${path} contains a non-finite number.`);
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectNonFinite(item, `${path}[${index}]`));
  } else if (value !== null && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      rejectNonFinite(item, `${path}.${key}`);
    }
  }
}

async function writeJsonAtomically(path, document) {
  const rendered = `${JSON.stringify(document, null, 2)}\n`;
  await mkdir(dirname(path), { recursive: true });
  const temporary = join(
    dirname(path),
    `.${basename(path)}.${process.pid}.temporary`
  );
  await writeFile(temporary, rendered, { encoding: "utf8", flag: "wx" });
  await rename(temporary, path);
}

async function main() {
  let options;
  try {
    options = parseArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(HELP);
      return 0;
    }
    await run(options);
    return 0;
  } catch (error) {
    process.stderr.write(
      `browser embedding qualification failed: ${formatErrorChain(error)}\n`
    );
    return 2;
  }
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

if (
  process.argv[1] !== undefined &&
  resolve(process.argv[1]) === resolve(SCRIPT_PATH)
) {
  process.exitCode = await main();
}
