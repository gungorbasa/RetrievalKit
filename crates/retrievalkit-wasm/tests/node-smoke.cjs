"use strict";

const assert = require("node:assert/strict");
const path = require("node:path");

const generatedModule = process.argv[2];
const expectedTier = process.argv[3] ?? "portable";
if (generatedModule === undefined) {
  throw new Error(
    "usage: node node-smoke.cjs <absolute path to wasm-bindgen Node module> [portable|simd128]"
  );
}
if (expectedTier !== "portable" && expectedTier !== "simd128") {
  throw new Error(`unsupported expected performance tier '${expectedTier}'`);
}
const retrievalkit = require(path.resolve(generatedModule));

function record(id, text, embeddingIndex, fields = []) {
  return {
    id,
    recordType: "Topic",
    fields,
    content: text,
    metadata: [],
    chunks: [
      {
        key: id,
        text,
        metadata: [],
        embeddingIndex
      }
    ]
  };
}

function graphSchema() {
  return {
    recordNodes: [
      {
        recordType: "Topic",
        nodeType: "Topic",
        queryableFields: []
      }
    ],
    relationships: [
      {
        relationshipType: "links",
        sourceNodeType: "Topic",
        targetNodeType: "Topic",
        sourceField: ["related"],
        cardinality: "optionalOne",
        missingTarget: "error",
        duplicateReferences: "error",
        allowSelfEdge: false
      }
    ]
  };
}

function graphQuery() {
  return {
    seed: {
      kind: "nodeIds",
      nodes: [
        {
          nodeType: "Topic",
          sourceKind: "record",
          recordId: "source"
        }
      ]
    },
    steps: [
      {
        relationship: "links",
        direction: "outgoing",
        minHops: 1,
        maxHops: 1
      }
    ]
  };
}

const sourceFields = [
  {
    field: "related",
    value: { kind: "string", value: "target" }
  }
];
const records = [
  record("source", "fast browser retrieval", 0, sourceFields),
  record("target", "target server notes", 1)
];
const embeddings = new Float32Array([1, 0, 0, 1]);
const searchOptions = { topK: 1 };

const capabilities = retrievalkit.buildCapabilities();
assert.deepEqual(
  {
    persistence: capabilities.persistence,
    threads: capabilities.threads,
    simd: capabilities.simd,
    performanceTier: capabilities.performanceTier
  },
  {
    persistence: false,
    threads: false,
    simd: expectedTier === "simd128",
    performanceTier: expectedTier
  }
);

const retrieval = new retrievalkit.RetrievalDatabase(
  "wasm-retrieval-smoke",
  "dotProduct",
  "f32"
);
assert.equal(
  retrieval.addRecordsBatch(records, embeddings.slice(), 2),
  2
);
retrieval.build();
assert.equal(
  retrieval.vectorSearch(new Float32Array([1, 0]), searchOptions)[0].documentId,
  "source"
);
assert.equal(
  retrieval.bm25Search("server", searchOptions)[0].documentId,
  "target"
);
assert.equal(
  retrieval.hybridSearch(new Float32Array([1, 0]), {
    text: "server",
    topK: 2,
    alpha: 0.5,
    vectorCandidates: 2,
    keywordCandidates: 2
  }).length,
  2
);
retrieval.close();
assert.throws(
  () => retrieval.vectorSearch(new Float32Array([1, 0]), searchOptions),
  /RK_INVALID_STATE/
);

for (const encoding of ["f16", "bf16", "i8"]) {
  const encoded = new retrievalkit.RetrievalDatabase(
    `wasm-${encoding}-smoke`,
    "dotProduct",
    encoding
  );
  assert.equal(encoded.addRecordsBatch(records, embeddings.slice(), 2), 2);
  encoded.build();
  assert.equal(
    encoded.vectorSearch(new Float32Array([1, 0]), searchOptions)[0].documentId,
    "source"
  );
  encoded.close();
}

const graph = new retrievalkit.GraphDatabase(
  "wasm-graph-smoke",
  graphSchema()
);
assert.equal(graph.addRecordsBatch(records), 2);
graph.build();
const graphSelection = graph.query(graphQuery());
assert.equal(graphSelection.matches[0].recordId, "target");
assert.equal(graphSelection.matches[0].path.length, 1);
assert.equal(graphSelection.matches[0].path[0].relationship, "links");
assert.equal(
  graph.projectCandidates(graphSelection.selectionId, undefined).candidates[0]
    .recordId,
  "target"
);
assert.equal(graph.releaseSelection(graphSelection.selectionId), true);

const combined = new retrievalkit.GraphRetrievalDatabase(
  "wasm-combined-smoke",
  graphSchema(),
  "dotProduct",
  "f32"
);
assert.equal(
  combined.addRecordsBatch(records, embeddings.slice(), 2),
  2
);
combined.build();
const combinedSelection = combined.graphQuery(graphQuery());
assert.equal(
  combined.vectorSearch(
    new Float32Array([0, 1]),
    searchOptions,
    combinedSelection.selectionId
  )[0].documentId,
  "target"
);
assert.equal(
  combined.bm25Search(
    "server",
    searchOptions,
    combinedSelection.selectionId
  )[0].documentId,
  "target"
);
assert.equal(
  combined.hybridSearch(
    new Float32Array([0, 1]),
    {
      text: "server",
      topK: 1,
      alpha: 0.5,
      vectorCandidates: 2,
      keywordCandidates: 2
    },
    combinedSelection.selectionId
  )[0].documentId,
  "target"
);
combined.close();

console.log("retrievalkit-wasm Node runtime smoke passed");
