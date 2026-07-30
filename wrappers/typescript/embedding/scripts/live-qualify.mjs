import { readFile, writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";

import { OnnxEmbedder } from "../dist/index.js";

const [, , inputPath, outputPath] = process.argv;
if (inputPath === undefined || outputPath === undefined) {
  throw new Error(
    "Usage: node scripts/live-qualify.mjs <texts.json> <output.json>"
  );
}
const input = JSON.parse(await readFile(inputPath, "utf8"));
const items = normalizeInput(input);
const texts = items.map(({ text }) => text);

const options = {
  ...(process.env["RETRIEVALKIT_EMBEDDING_CACHE_DIRECTORY"] === undefined
    ? {}
    : {
        cacheDirectory:
          process.env["RETRIEVALKIT_EMBEDDING_CACHE_DIRECTORY"]
      }),
  ...(process.env["RETRIEVALKIT_EMBEDDING_LOCAL_ONLY"] === "1"
    ? { localOnly: true }
    : {}),
  ...(process.env["RETRIEVALKIT_ONNX_RUNTIME_LIBRARY"] === undefined
    ? {}
    : {
        runtimeLibraryPath:
          process.env["RETRIEVALKIT_ONNX_RUNTIME_LIBRARY"]
      })
};

const loadStart = performance.now();
const embedder = await OnnxEmbedder.load(options);
const loadMs = performance.now() - loadStart;
const firstStart = performance.now();
await embedder.embed(texts[0]);
const firstInferenceMs = performance.now() - firstStart;

const benchmarkText = Array.from(
  { length: 32 },
  (_, index) => `token${index}`
).join(" ");
for (let index = 0; index < 50; index += 1) {
  await embedder.embed(benchmarkText);
}
const samples = [];
for (let index = 0; index < 750; index += 1) {
  const start = performance.now();
  await embedder.embed(benchmarkText);
  samples.push(performance.now() - start);
}
samples.sort((left, right) => left - right);
const vectors = await embedder.embedBatch(texts);
const info = embedder.modelInfo;
const result = {
  schema_version: 1,
  model: {
    identifier: info.identifier,
    revision: info.sourceRevision,
    profile: "fp32",
    dtype: "float32",
    dimension: info.dimension,
    max_input_tokens: info.maxInputTokens,
    normalized: info.normalized
  },
  items: items.map(({ id }, index) => ({
    id,
    embedding: Array.from(vectors[index])
  }))
};
const benchmark = {
  schema_version: 1,
  model: result.model,
  benchmark: {
    buildMode: "release",
    inputTokens: 32,
    warmups: 50,
    measured: 750,
    loadMs,
    firstInferenceMs,
    warmEmbeddingMs: {
      p50: percentile(samples, 0.5),
      p95: percentile(samples, 0.95),
      p99: percentile(samples, 0.99)
    }
  }
};
await embedder.close();
await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`);
await writeFile(
  `${outputPath}.benchmark.json`,
  `${JSON.stringify(benchmark, null, 2)}\n`
);
process.stdout.write(`${JSON.stringify(benchmark.benchmark)}\n`);

function normalizeInput(input) {
  if (Array.isArray(input)) {
    if (
      input.length === 0 ||
      input.some((value) => typeof value !== "string" || value.trim() === "")
    ) {
      throw new Error("Input string array must contain non-empty strings.");
    }
    return input.map((text, index) => ({ id: String(index), text }));
  }
  if (
    input?.schema_version !== 1 ||
    !Array.isArray(input.items) ||
    input.items.length === 0
  ) {
    throw new Error(
      "Input must be a string array or {schema_version:1,items:[{id,text,role}]}."
    );
  }
  const seen = new Set();
  return input.items.map((item) => {
    if (
      typeof item?.id !== "string" ||
      item.id.length === 0 ||
      seen.has(item.id) ||
      typeof item.text !== "string" ||
      item.text.trim() === ""
    ) {
      throw new Error("Every input item requires a unique id and non-empty text.");
    }
    seen.add(item.id);
    return { id: item.id, text: item.text };
  });
}

function percentile(values, fraction) {
  const index = Math.min(
    values.length - 1,
    Math.max(0, Math.ceil(values.length * fraction) - 1)
  );
  return values[index];
}
