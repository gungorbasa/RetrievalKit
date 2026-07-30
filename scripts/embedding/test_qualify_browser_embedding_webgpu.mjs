import assert from "node:assert/strict";
import test from "node:test";

import {
  WebGpuQualificationError,
  nearestRankPercentile,
  parseArguments,
  qualificationWorkerSource,
  rewriteBuiltModule,
  validateWorkerResult
} from "./qualify-browser-embedding-webgpu.mjs";

test("parses the offline WebGPU launcher options", () => {
  const options = parseArguments([
    "--artifacts",
    "local-artifacts",
    "--output",
    "report.json",
    "--timeout-ms",
    "120000"
  ]);
  assert.equal(options.help, false);
  assert.equal(options.timeoutMs, 120000);
  assert.match(options.packageDist, /wrappers\/browser-embedding\/dist$/);
});

test("help does not require paths and invalid timeouts fail", () => {
  assert.deepEqual(parseArguments(["--help"]), { help: true });
  assert.throws(
    () =>
      parseArguments([
        "--artifacts",
        "artifacts",
        "--output",
        "output.json",
        "--timeout-ms",
        "999"
      ]),
    WebGpuQualificationError
  );
});

test("rewrites only the built package bare browser dependencies", () => {
  const source =
    'import { Tokenizer } from "@huggingface/tokenizers";\\n' +
    'const gpu = import("onnxruntime-web/webgpu");\\n' +
    'const wasm = import("onnxruntime-web/wasm");\\n';
  const rewritten = rewriteBuiltModule(source);
  assert.equal(rewritten.includes('from "/vendor/tokenizers.mjs"'), true);
  assert.equal(
    rewritten.includes('import("/vendor/ort.webgpu.bundle.min.mjs")'),
    true
  );
  assert.equal(
    rewritten.includes('import("/vendor/ort.wasm.bundle.min.mjs")'),
    true
  );
});

test("generated Worker is fixed to WebGPU and the 50/750 contract", () => {
  const source = qualificationWorkerSource();
  assert.match(source, /execution: "webgpu"/);
  assert.match(source, /const WARMUPS = 50/);
  assert.match(source, /const MEASURED = 750/);
  assert.match(source, /new EmbeddingWorkerService/);
  assert.match(source, /DedicatedWorkerGlobalScope/);
  assert.match(source, /navigator/);
});

test("validates the exact Worker result and rejects WASM fallback", () => {
  const result = {
    ok: true,
    benchmark: {
      provider: "webgpu",
      dimension: 384,
      normalized: true,
      input_tokens: 32,
      batch_size: 1,
      warmups: 50,
      measured: 750,
      artifact_requests: 6,
      cached_initialization_ms: 100,
      first_inference_ms: 5,
      samples_ms: new Array(750).fill(2)
    }
  };
  assert.equal(validateWorkerResult(result).provider, "webgpu");
  result.benchmark.provider = "wasm";
  assert.throws(() => validateWorkerResult(result), /fixed WebGPU/);
});

test("uses nearest-rank percentiles", () => {
  assert.equal(nearestRankPercentile([1, 2, 3, 4], 0.5), 2);
  assert.equal(nearestRankPercentile([1, 2, 3, 4], 0.95), 4);
});
