import assert from "node:assert/strict";
import test from "node:test";

import {
  QualificationInputError,
  nearestRankPercentile,
  normalizeQualificationInput,
  parseArguments,
  validateCandidateDocument
} from "./qualify-browser-embedding.mjs";

const roles = [
  ["corpus", 48],
  ["query", 42],
  ["diagnostic", 4]
];

function validInput() {
  return {
    schema_version: 1,
    items: roles.flatMap(([role, count]) =>
      Array.from({ length: count }, (_, index) => ({
        id: `${role}-${String(index).padStart(3, "0")}`,
        role,
        text: `${role} text ${index}`
      }))
    )
  };
}

function unitVector() {
  return [1, ...new Array(383).fill(0)];
}

test("parses required paths and resolves the default built package", () => {
  const result = parseArguments([
    "--input",
    "input.json",
    "--artifacts",
    "artifacts",
    "--output",
    "candidate.json",
    "--benchmark-output",
    "benchmark.json"
  ]);
  assert.equal(result.help, false);
  assert.match(result.packageDist, /wrappers\/browser-embedding\/dist$/);
  assert.equal(result.input.endsWith("/input.json"), true);
});

test("help does not require qualification paths", () => {
  assert.deepEqual(parseArguments(["--help"]), { help: true });
});

test("rejects missing and unknown CLI options", () => {
  assert.throws(
    () => parseArguments(["--unknown", "value"]),
    QualificationInputError
  );
  assert.throws(
    () =>
      parseArguments([
        "--input",
        "input.json",
        "--artifacts",
        "artifacts",
        "--output",
        "candidate.json"
      ]),
    /--benchmark-output/
  );
});

test("accepts only the frozen ordered 94-item input", () => {
  const normalized = normalizeQualificationInput(validInput());
  assert.equal(normalized.length, 94);
  assert.equal(normalized[0].id, "corpus-000");
  assert.equal(normalized.at(-1).id, "diagnostic-003");

  const reordered = validInput();
  [reordered.items[0], reordered.items[1]] = [
    reordered.items[1],
    reordered.items[0]
  ];
  assert.throws(
    () => normalizeQualificationInput(reordered),
    /must equal 'corpus-000'/
  );

  const extraKey = validInput();
  extraKey.items[0].unexpected = true;
  assert.throws(
    () => normalizeQualificationInput(extraKey),
    /keys must be exactly/
  );
});

test("uses nearest-rank percentiles", () => {
  assert.equal(nearestRankPercentile([1, 2, 3, 4], 0.5), 2);
  assert.equal(nearestRankPercentile([1, 2, 3, 4], 0.95), 4);
  assert.throws(() => nearestRankPercentile([], 0.5), QualificationInputError);
});

test("validates the exact wrapper conformance output schema", () => {
  const items = normalizeQualificationInput(validInput());
  const candidate = {
    schema_version: 1,
    model: {
      identifier: "sentence-transformers/all-MiniLM-L6-v2",
      revision: "c9745ed1d9f207416be6d2e6f8de32d1f16199bf",
      profile: "fp32",
      dtype: "float32",
      dimension: 384,
      max_input_tokens: 256,
      normalized: true
    },
    items: items.map(({ id }) => ({ id, embedding: unitVector() }))
  };
  assert.doesNotThrow(() => validateCandidateDocument(candidate, items));
  candidate.items[3].embedding[0] = Number.NaN;
  assert.throws(
    () => validateCandidateDocument(candidate, items),
    /must be finite/
  );
});
