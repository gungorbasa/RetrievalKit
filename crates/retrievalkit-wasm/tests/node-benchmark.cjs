"use strict";

const { performance } = require("node:perf_hooks");
const path = require("node:path");

const generatedModule = process.argv[2];
const count = Number(process.argv[3] ?? "10000");
const dimension = Number(process.argv[4] ?? "384");
const iterations = Number(process.argv[5] ?? "30");
const encoding = process.argv[6] ?? "f32";
const warmups = Number(process.argv[7] ?? "5");
if (
  generatedModule === undefined ||
  !Number.isSafeInteger(count) ||
  count <= 0 ||
  !Number.isSafeInteger(dimension) ||
  dimension <= 0 ||
  !Number.isSafeInteger(iterations) ||
  iterations <= 0 ||
  !Number.isSafeInteger(warmups) ||
  warmups < 0
) {
  throw new Error(
    "usage: node node-benchmark.cjs <generated module> [count] [dimension] [iterations] [encoding] [warmups]"
  );
}

const moduleStarted = performance.now();
const retrievalkit = require(path.resolve(generatedModule));
const moduleLoadMs = performance.now() - moduleStarted;
const capabilities = retrievalkit.buildCapabilities();

const embeddings = new Float32Array(count * dimension);
const records = new Array(count);
for (let row = 0; row < count; row += 1) {
  // A deterministic dense corpus exercises every multiply without requiring a
  // checked-in benchmark asset.
  let state = (row + 1) * 2654435761;
  const start = row * dimension;
  for (let column = 0; column < dimension; column += 1) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    embeddings[start + column] = (state / 0xffff_ffff) * 2 - 1;
  }
  const text =
    row % 97 === 0
      ? `browser benchmark needle document ${row}`
      : `browser benchmark document ${row}`;
  records[row] = {
    id: `document-${row}`,
    recordType: "Document",
    fields: [],
    content: text,
    metadata: [],
    chunks: [
      {
        key: `document-${row}`,
        text,
        metadata: [],
        embeddingIndex: row
      }
    ]
  };
}
const query = embeddings.slice(
  Math.min(17, count - 1) * dimension,
  (Math.min(17, count - 1) + 1) * dimension
);

const database = new retrievalkit.RetrievalDatabase(
  "portable-node-benchmark",
  "cosine",
  encoding
);
const ingestionStarted = performance.now();
database.addRecordsBatch(records, embeddings, dimension);
const ingestionMs = performance.now() - ingestionStarted;
const buildStarted = performance.now();
database.build();
const buildMs = performance.now() - buildStarted;

for (let index = 0; index < warmups; index += 1) {
  database.vectorSearch(query, { topK: 10 });
  database.bm25Search("needle", { topK: 10 });
  database.hybridSearch(query, {
    text: "needle",
    topK: 10,
    alpha: 0.6,
    vectorCandidates: 50,
    keywordCandidates: 50
  });
}

const vectorMs = measure(iterations, () =>
  database.vectorSearch(query, { topK: 10 })
);
const bm25Ms = measure(iterations, () =>
  database.bm25Search("needle", { topK: 10 })
);
const hybridMs = measure(iterations, () =>
  database.hybridSearch(query, {
    text: "needle",
    topK: 10,
    alpha: 0.6,
    vectorCandidates: 50,
    keywordCandidates: 50
  })
);
database.close();

console.log(
  JSON.stringify(
    {
      schemaVersion: 1,
      runtime: `node ${process.version}`,
      platform: `${process.platform}-${process.arch}`,
      performanceTier: capabilities.performanceTier,
      simd: capabilities.simd,
      count,
      dimension,
      encoding,
      topK: 10,
      warmups,
      iterations,
      moduleLoadMs,
      ingestionMs,
      buildMs,
      vectorMs: percentiles(vectorMs),
      bm25Ms: percentiles(bm25Ms),
      hybridMs: percentiles(hybridMs)
    },
    null,
    2
  )
);

function measure(sampleCount, operation) {
  const samples = [];
  for (let index = 0; index < sampleCount; index += 1) {
    const started = performance.now();
    operation();
    samples.push(performance.now() - started);
  }
  return samples;
}

function percentiles(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return {
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    min: sorted[0],
    max: sorted[sorted.length - 1]
  };
}

function percentile(sorted, quantile) {
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * quantile) - 1)];
}
