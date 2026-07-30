import assert from "node:assert/strict";
import test from "node:test";

import {
  DesktopMatrixError,
  embeddingWorkerSource,
  harnessPageSource,
  parseArguments,
  percentileSummary,
  retrievalWorkerSource,
  rewriteEmbeddingModule,
  validateBrowserResult
} from "./qualify-browser-desktop-matrix.mjs";

test("parses a strict browser subset and execution provider", () => {
  const options = parseArguments([
    "--artifacts",
    "artifacts",
    "--output",
    "matrix.json",
    "--browsers",
    "chrome,safari",
    "--execution",
    "wasm",
    "--require-all"
  ]);
  assert.deepEqual(options.browsers, ["chrome", "safari"]);
  assert.equal(options.execution, "wasm");
  assert.equal(options.requireAll, true);
  assert.equal(options.chunks, 50_000);
});

test("rejects invalid browser and timeout options", () => {
  assert.throws(
    () =>
      parseArguments([
        "--artifacts",
        "artifacts",
        "--output",
        "matrix.json",
        "--browsers",
        "edge"
      ]),
    DesktopMatrixError
  );
  assert.throws(
    () =>
      parseArguments([
        "--artifacts",
        "artifacts",
        "--output",
        "matrix.json",
        "--timeout-ms",
        "100"
      ]),
    /30000/
  );
});

test("generates dedicated Worker entries for both package boundaries", () => {
  const embedding = embeddingWorkerSource();
  const retrieval = retrievalWorkerSource();
  assert.match(embedding, /installBrowserEmbeddingWorker/);
  assert.match(embedding, /PINNED_ARTIFACTS/);
  assert.match(embedding, /DedicatedWorkerGlobalScope/);
  assert.match(retrieval, /installRetrievalKitWorker/);
  assert.match(retrieval, /createAdaptiveGeneratedWasmAdapter/);
  assert.match(retrieval, /generated\/simd128/);
});

test("page uses real CacheStorage and fixed same-page 50/750 I8 benchmark", () => {
  const source = harnessPageSource("auto");
  assert.match(source, /const WARMUPS = 50/);
  assert.match(source, /const MEASURED = 750/);
  assert.match(source, /const CHUNKS = 50000/);
  assert.match(source, /caches\.open/);
  assert.match(source, /localOnly: true/);
  assert.match(source, /Promise\.all/);
  assert.match(source, /artifactRequests === 6/);
  assert.match(source, /interruptedAcquisitionCleaned/);
  assert.match(source, /interruption\.abort/);
  assert.match(source, /encoding: "i8"/);
  assert.match(source, /await embedder\.embed\(queryText\)/);
  assert.match(source, /await search\(vector\)/);
  assert.match(source, /sequenceLength !== 32/);
  assert.match(source, /cachedInitializationMs/);
  assert.match(source, /firstInferenceMs/);
  assert.match(source, /ingestionMs/);
  assert.match(source, /embedding_samples_ms/);
  assert.match(source, /Merhaba İstanbul/);
  assert.match(source, /"local "\.repeat\(400\)/);
});

test("rewrites browser-only bare imports", () => {
  const rewritten = rewriteEmbeddingModule(
    'import x from "@huggingface/tokenizers";\\n' +
      'import("onnxruntime-web/webgpu");\\n' +
      'import("onnxruntime-web/wasm");\\n'
  );
  assert.match(rewritten, /\/vendor\/tokenizers\.mjs/);
  assert.match(rewritten, /ort\.webgpu\.bundle\.min\.mjs/);
  assert.match(rewritten, /ort\.wasm\.bundle\.min\.mjs/);
});

test("validates all correctness gates and exact sample counts", () => {
  const result = fixture();
  assert.equal(validateBrowserResult(result).provider, "wasm");
  result.checks.unicode = false;
  assert.throws(() => validateBrowserResult(result), /qualification contract/);
});

test("computes nearest-rank summaries", () => {
  assert.deepEqual(percentileSummary([4, 1, 3, 2]), {
    minimum: 1,
    mean: 2.5,
    p50: 2,
    p95: 4,
    p99: 4,
    maximum: 4
  });
});

function fixture() {
  return {
    ok: true,
    provider: "wasm",
    retrieval_tier: "simd128",
    dimension: 384,
    finite: true,
    normalized: true,
    corpus_chunks: 50_000,
    artifact_requests: 6,
    input_tokens: 32,
    checks: {
      dedicated_workers: true,
      local_only_missing_rejected: true,
      concurrent_prefetch_deduplicated: true,
      interrupted_acquisition_cleaned: true,
      cached_local_only_load: true,
      corruption_rejected_and_recovered: true,
      unicode: true,
      truncation_256: true,
      empty_input_rejected: true,
      lifecycle_after_close_rejected: true
    },
    benchmark: {
      warmups: 50,
      measured: 750,
      cached_initialization_ms: 100,
      first_inference_ms: 20,
      ingestion_ms: 500,
      embedding_samples_ms: new Array(750).fill(1.5),
      end_to_end_samples_ms: new Array(750).fill(2),
      retrieval_samples_ms: new Array(750).fill(1)
    }
  };
}
