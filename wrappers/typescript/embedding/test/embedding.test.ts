import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  EmbeddingClosedError,
  EmbeddingInputError,
  EmbeddingKitError,
  OnnxEmbedder
} from "../src/index.js";
import { binding } from "../src/binding.js";

const live = process.env["RETRIEVALKIT_EMBEDDING_LIVE_TEST"] === "1";
const describeLive = live ? describe : describe.skip;

describe("embedding package offline contract", () => {
  it("loads the native aggregate and exports its public class", () => {
    expect(OnnxEmbedder).toBeTypeOf("function");
    expect(binding.NativeOnnxEmbedder).toBeTypeOf("function");
  });

  it("maps an unavailable local-only prefetch to a typed error", async () => {
    const missing = await mkdtemp(join(tmpdir(), "retrievalkit-node-offline-"));
    try {
      await expect(
        OnnxEmbedder.prefetch({ cacheDirectory: missing, localOnly: true })
      ).rejects.toMatchObject({
        code: "RK_EMBEDDING_UNAVAILABLE"
      } satisfies Partial<EmbeddingKitError>);
    } finally {
      await rm(missing, { recursive: true, force: true });
    }
  });

  it("fails closed for an unqualified package-local runtime", async () => {
    await expect(
      binding._verifyPackageRuntime("/tmp/libonnxruntime-wrong.dylib")
    ).rejects.toThrow("RK_EMBEDDING_RUNTIME");
  });

  it("exposes typed public input errors", () => {
    const error = new EmbeddingInputError("Input text cannot be empty.");
    expect(error).toBeInstanceOf(EmbeddingKitError);
    expect(error.code).toBe("RK_EMBEDDING_INPUT");
  });
});

describeLive("ONNX embedding live qualification", () => {
  let cacheDirectory: string;

  beforeAll(async () => {
    cacheDirectory = await mkdtemp(join(tmpdir(), "retrievalkit-node-embedding-"));
  });

  afterAll(async () => {
    await rm(cacheDirectory, { recursive: true, force: true });
  });

  it("prefetches, loads locally, and embeds Unicode and batches", async () => {
    await OnnxEmbedder.prefetch({ cacheDirectory });
    const embedder = await OnnxEmbedder.load({
      cacheDirectory,
      localOnly: true
    });
    expect(embedder.modelInfo).toMatchObject({
      dimension: 384,
      maxInputTokens: 256,
      normalized: true,
      precision: "fp32",
      runtime: "onnxruntime",
      runtimeVersion: "1.24.3"
    });

    const unicode = await embedder.embed("İstanbul — こんにちは — 🥚");
    expect(unicode).toBeInstanceOf(Float32Array);
    expect(unicode).toHaveLength(384);
    const batch = await embedder.embedBatch(["vector search", "graph retrieval"]);
    expect(batch).toHaveLength(2);
    expect(batch.every((value) => value instanceof Float32Array)).toBe(true);

    await expect(embedder.embed("")).rejects.toBeInstanceOf(EmbeddingInputError);
    await expect(embedder.embedBatch([])).rejects.toBeInstanceOf(
      EmbeddingInputError
    );
    await embedder.close();
    await expect(embedder.embed("closed")).rejects.toBeInstanceOf(
      EmbeddingClosedError
    );
    expect(embedder.closed).toBe(true);
  });

  it("reports an unavailable local-only cache with a typed error", async () => {
    const missing = await mkdtemp(join(tmpdir(), "retrievalkit-node-missing-"));
    try {
      await expect(
        OnnxEmbedder.load({ cacheDirectory: missing, localOnly: true })
      ).rejects.toMatchObject({
        code: "RK_EMBEDDING_UNAVAILABLE"
      } satisfies Partial<EmbeddingKitError>);
    } finally {
      await rm(missing, { recursive: true, force: true });
    }
  });
});
