import type * as Ort from "onnxruntime-web";

import { EMBEDDING_DIMENSION } from "./constants.js";
import {
  cancelled,
  EmbeddingOutputError,
  EmbeddingRuntimeError
} from "./errors.js";
import { PinnedMiniLmTokenizer } from "./tokenizer.js";
import type { ExecutionPreference, ExecutionProvider } from "./types.js";

export interface EmbeddingRuntime {
  readonly provider: ExecutionProvider;
  embed(texts: readonly string[], signal?: AbortSignal): Promise<Float32Array>;
  close(): Promise<void>;
}

export interface EmbeddingRuntimeFactory {
  create(
    model: Uint8Array,
    tokenizer: Uint8Array,
    tokenizerConfig: Uint8Array,
    execution: ExecutionPreference
  ): Promise<EmbeddingRuntime>;
}

export const onnxRuntimeFactory: EmbeddingRuntimeFactory = {
  async create(model, tokenizerBytes, tokenizerConfigBytes, execution): Promise<EmbeddingRuntime> {
    const tokenizer = new PinnedMiniLmTokenizer(tokenizerBytes, tokenizerConfigBytes);
    const providers: readonly ExecutionProvider[] =
      execution === "auto" ? ["webgpu", "wasm"] : [execution];
    let lastError: unknown;
    for (const provider of providers) {
      if (provider === "webgpu" && !(await hasUsableWebGpu())) continue;
      let runtime: OnnxEmbeddingRuntime | undefined;
      try {
        const ort =
          provider === "webgpu"
            ? await import("onnxruntime-web/webgpu")
            : await import("onnxruntime-web/wasm");
        configureOrt(ort);
        const session = await ort.InferenceSession.create(model.slice(), {
          executionProviders:
            execution === "auto" && provider === "webgpu"
              ? ["webgpu", "wasm"]
              : [provider],
          graphOptimizationLevel: "all"
        });
        runtime = new OnnxEmbeddingRuntime(ort, session, tokenizer, provider);
        await runtime.embed(["RetrievalKit warmup"]);
        return runtime;
      } catch (error) {
        lastError = error;
        await runtime?.close().catch(() => undefined);
      }
    }
    throw new EmbeddingRuntimeError(
      `Unable to initialize the pinned FP32 MiniLM model with execution '${execution}'.`,
      lastError
    );
  }
};

class OnnxEmbeddingRuntime implements EmbeddingRuntime {
  public readonly provider: ExecutionProvider;
  readonly #ort: typeof Ort;
  readonly #session: Ort.InferenceSession;
  readonly #tokenizer: PinnedMiniLmTokenizer;
  #closed = false;

  public constructor(
    ort: typeof Ort,
    session: Ort.InferenceSession,
    tokenizer: PinnedMiniLmTokenizer,
    provider: ExecutionProvider
  ) {
    this.#ort = ort;
    this.#session = session;
    this.#tokenizer = tokenizer;
    this.provider = provider;
  }

  public async embed(
    texts: readonly string[],
    signal?: AbortSignal
  ): Promise<Float32Array> {
    if (this.#closed) throw new EmbeddingRuntimeError("Embedding runtime is closed.");
    cancelled(signal);
    const encoded = this.#tokenizer.tokenize(texts);
    const shape: readonly [number, number] = [
      encoded.batchSize,
      encoded.sequenceLength
    ];
    const feeds = {
      input_ids: new this.#ort.Tensor("int64", encoded.inputIds, shape),
      attention_mask: new this.#ort.Tensor("int64", encoded.attentionMask, shape),
      token_type_ids: new this.#ort.Tensor("int64", encoded.tokenTypeIds, shape)
    };
    let results: Ort.InferenceSession.ReturnType | undefined;
    try {
      results = await this.#session.run(feeds);
      cancelled(signal);
      const tensor = results.embedding ?? Object.values(results)[0];
      if (tensor === undefined) {
        throw new EmbeddingOutputError("The pinned model returned no embedding tensor.");
      }
      const data = await tensor.getData();
      if (!(data instanceof Float32Array)) {
        throw new EmbeddingOutputError("The pinned model did not return Float32 output.");
      }
      if (data.length !== texts.length * EMBEDDING_DIMENSION) {
        throw new EmbeddingOutputError(
          `Embedding batch contains ${data.length} values; expected ${texts.length * EMBEDDING_DIMENSION}.`
        );
      }
      const output = new Float32Array(data);
      for (let row = 0; row < texts.length; row += 1) {
        assertEmbedding(
          output.subarray(row * EMBEDDING_DIMENSION, (row + 1) * EMBEDDING_DIMENSION)
        );
      }
      return output;
    } catch (error) {
      if (error instanceof EmbeddingOutputError) throw error;
      throw new EmbeddingRuntimeError("ONNX Runtime Web inference failed.", error);
    } finally {
      for (const tensor of Object.values(results ?? {})) tensor.dispose();
    }
  }

  public async close(): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    await this.#session.release();
  }
}

export function validateEmbedding(input: Float32Array): Float32Array {
  assertEmbedding(input);
  return new Float32Array(input);
}

export function assertEmbedding(input: Float32Array): void {
  if (input.length !== EMBEDDING_DIMENSION) {
    throw new EmbeddingOutputError(
      `Embedding contains ${input.length} values; expected ${EMBEDDING_DIMENSION}.`
    );
  }
  let squaredNorm = 0;
  for (const value of input) {
    if (!Number.isFinite(value)) {
      throw new EmbeddingOutputError("Embedding contains a non-finite value.");
    }
    squaredNorm += value * value;
  }
  const norm = Math.sqrt(squaredNorm);
  if (!Number.isFinite(norm) || Math.abs(norm - 1) > 1e-4) {
    throw new EmbeddingOutputError(
      `Embedding L2 norm is ${norm}; expected unit normalization within 1e-4.`
    );
  }
}

async function hasUsableWebGpu(): Promise<boolean> {
  if (typeof navigator === "undefined") return false;
  const gpu = (
    navigator as Navigator & {
      readonly gpu?: {
        requestAdapter(): Promise<object | null>;
      };
    }
  ).gpu;
  if (gpu === undefined) return false;
  try {
    return (await gpu.requestAdapter()) !== null;
  } catch {
    return false;
  }
}

const configuredOrtEnvironments = new WeakSet<object>();

function configureOrt(ort: typeof Ort): void {
  if (configuredOrtEnvironments.has(ort.env)) return;
  configuredOrtEnvironments.add(ort.env);
  ort.env.wasm.numThreads = 1;
  ort.env.wasm.proxy = false;
  ort.env.wasm.wasmPaths = new URL("./runtime/", import.meta.url).href;
}
