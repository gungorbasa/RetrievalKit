"use strict";

const assert = require("node:assert/strict");
const path = require("node:path");

const portableModule = process.argv[2];
const simdModule = process.argv[3];
if (portableModule === undefined || simdModule === undefined) {
  throw new Error(
    "usage: node node-simd-conformance.cjs <portable module> <simd128 module>"
  );
}

const portable = require(path.resolve(portableModule));
const simd128 = require(path.resolve(simdModule));

assert.equal(portable.buildCapabilities().performanceTier, "portable");
assert.equal(simd128.buildCapabilities().performanceTier, "simd128");

for (const dimension of [384, 396]) {
  const count = 64;
  const embeddings = new Float32Array(count * dimension);
  const records = [];
  for (let row = 0; row < count; row += 1) {
    let state = (row + 7) * 2654435761;
    const start = row * dimension;
    for (let column = 0; column < dimension; column += 1) {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      embeddings[start + column] = (state / 0xffff_ffff) * 2 - 1;
    }
    const text =
      row % 11 === 0
        ? `simd parity needle document ${row}`
        : `simd parity document ${row}`;
    records.push({
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
    });
  }
  const queryRow = 17;
  const query = embeddings.slice(
    queryRow * dimension,
    (queryRow + 1) * dimension
  );
  const portableDatabase = buildDatabase(
    portable,
    `portable-${dimension}`,
    records,
    embeddings,
    dimension
  );
  const simdDatabase = buildDatabase(
    simd128,
    `simd128-${dimension}`,
    records,
    embeddings,
    dimension
  );

  assert.deepEqual(
    simdDatabase.vectorSearch(query, { topK: 10 }),
    portableDatabase.vectorSearch(query, { topK: 10 }),
    `${dimension}d vector results must match`
  );
  assert.deepEqual(
    simdDatabase.hybridSearch(query, {
      text: "needle",
      topK: 10,
      alpha: 0.6,
      vectorCandidates: 50,
      keywordCandidates: 50
    }),
    portableDatabase.hybridSearch(query, {
      text: "needle",
      topK: 10,
      alpha: 0.6,
      vectorCandidates: 50,
      keywordCandidates: 50
    }),
    `${dimension}d hybrid results must match`
  );
  assert.deepEqual(
    simdDatabase.bm25Search("needle", { topK: 10 }),
    portableDatabase.bm25Search("needle", { topK: 10 }),
    `${dimension}d BM25 results must match`
  );
  portableDatabase.close();
  simdDatabase.close();
}

console.log("retrievalkit-wasm portable/SIMD128 conformance passed");

function buildDatabase(module, corpusId, records, embeddings, dimension) {
  const database = new module.RetrievalDatabase(
    corpusId,
    "cosine",
    "i8"
  );
  assert.equal(
    database.addRecordsBatch(records, embeddings.slice(), dimension),
    records.length
  );
  database.build();
  return database;
}
