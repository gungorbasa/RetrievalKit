import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { binding, type NativeOnnxEmbedder } from "./binding.js";
import type { NativeLoadOptions, NativeModelInfo } from "./native-types.js";

export type EmbeddingErrorCode =
  | "RK_EMBEDDING_INPUT"
  | "RK_EMBEDDING_UNAVAILABLE"
  | "RK_EMBEDDING_ARTIFACT"
  | "RK_EMBEDDING_IO"
  | "RK_EMBEDDING_TOKENIZER"
  | "RK_EMBEDDING_RUNTIME"
  | "RK_EMBEDDING_MODEL"
  | "RK_EMBEDDING_OUTPUT"
  | "RK_EMBEDDING_STATE"
  | "RK_EMBEDDING_CLOSED";

const errorCodes = new Set<EmbeddingErrorCode>([
  "RK_EMBEDDING_INPUT",
  "RK_EMBEDDING_UNAVAILABLE",
  "RK_EMBEDDING_ARTIFACT",
  "RK_EMBEDDING_IO",
  "RK_EMBEDDING_TOKENIZER",
  "RK_EMBEDDING_RUNTIME",
  "RK_EMBEDDING_MODEL",
  "RK_EMBEDDING_OUTPUT",
  "RK_EMBEDDING_STATE",
  "RK_EMBEDDING_CLOSED"
]);

export class EmbeddingKitError extends Error {
  readonly code: EmbeddingErrorCode;
  override readonly cause?: unknown;

  constructor(message: string, code: EmbeddingErrorCode, cause?: unknown) {
    super(message);
    this.name = "EmbeddingKitError";
    this.code = code;
    this.cause = cause;
  }
}

export class EmbeddingInputError extends EmbeddingKitError {
  constructor(message: string, cause?: unknown) {
    super(message, "RK_EMBEDDING_INPUT", cause);
    this.name = "EmbeddingInputError";
  }
}

export class EmbeddingClosedError extends EmbeddingKitError {
  constructor(message = "The ONNX embedder is closed.", cause?: unknown) {
    super(message, "RK_EMBEDDING_CLOSED", cause);
    this.name = "EmbeddingClosedError";
  }
}

export interface EmbeddingModelInfo {
  readonly identifier: string;
  readonly dimension: 384;
  readonly maxInputTokens: 256;
  readonly normalized: true;
  readonly precision: "fp32";
  readonly sourceRevision: string;
  readonly runtime: "onnxruntime";
  readonly runtimeVersion: "1.24.3";
}

export interface OnnxEmbedderOptions {
  /**
   * Override the OS cache location used for verified model artifacts.
   */
  readonly cacheDirectory?: string;
  /**
   * Refuse network access and load only a previously verified cache entry.
   */
  readonly localOnly?: boolean;
  /**
   * Path to the application-managed official ONNX Runtime 1.24.3 library.
   * When omitted, the package-local verified runtime is preferred, followed
   * by RETRIEVALKIT_ONNX_RUNTIME_LIBRARY.
   */
  readonly runtimeLibraryPath?: string;
}

export interface PrefetchOptions {
  readonly cacheDirectory?: string;
  readonly localOnly?: boolean;
}

const packageRuntimePath = fileURLToPath(
  new URL("../runtime/libonnxruntime.1.24.3.dylib", import.meta.url)
);

/**
 * Local FP32 MiniLM embeddings backed by the official ONNX Runtime 1.24.3.
 *
 * Model acquisition happens only in load() or prefetch(). All native work is
 * scheduled away from the JavaScript event loop.
 */
export class OnnxEmbedder implements AsyncDisposable, Disposable {
  readonly #native: NativeOnnxEmbedder;
  #closed = false;

  private constructor(native: NativeOnnxEmbedder) {
    this.#native = native;
  }

  static async load(options: OnnxEmbedderOptions = {}): Promise<OnnxEmbedder> {
    const native = new binding.NativeOnnxEmbedder();
    const runtime = resolveRuntime(options.runtimeLibraryPath);
    const nativeOptions: NativeLoadOptions = {
      ...(options.cacheDirectory === undefined
        ? {}
        : { cacheDirectory: options.cacheDirectory }),
      ...(options.localOnly === undefined
        ? {}
        : { localOnly: options.localOnly }),
      ...(runtime.path === undefined
        ? {}
        : { runtimeLibraryPath: runtime.path }),
      ...(runtime.packageLocal ? { verifyPackageRuntime: true } : {})
    };
    try {
      await native.initialize(nativeOptions);
      return new OnnxEmbedder(native);
    } catch (error) {
      await native.close().catch(() => undefined);
      throw mapError(error);
    }
  }

  static async prefetch(options: PrefetchOptions = {}): Promise<void> {
    try {
      await binding.prefetchModel({
        ...(options.cacheDirectory === undefined
          ? {}
          : { cacheDirectory: options.cacheDirectory }),
        ...(options.localOnly === undefined
          ? {}
          : { localOnly: options.localOnly })
      });
    } catch (error) {
      throw mapError(error);
    }
  }

  get closed(): boolean {
    return this.#closed || this.#native.closed;
  }

  get modelInfo(): EmbeddingModelInfo {
    this.#requireOpen();
    try {
      return validateModelInfo(this.#native.modelInfo());
    } catch (error) {
      throw mapError(error);
    }
  }

  async embed(text: string): Promise<Float32Array> {
    this.#requireOpen();
    validateText(text);
    try {
      return validateEmbedding(await this.#native.embed(text));
    } catch (error) {
      throw mapError(error);
    }
  }

  async embedBatch(texts: readonly string[]): Promise<readonly Float32Array[]> {
    this.#requireOpen();
    if (texts.length === 0) {
      throw new EmbeddingInputError("Embedding batch cannot be empty.");
    }
    for (const text of texts) {
      validateText(text);
    }
    try {
      const output = await this.#native.embedBatch([...texts]);
      return output.map(validateEmbedding);
    } catch (error) {
      throw mapError(error);
    }
  }

  async close(): Promise<void> {
    if (this.#closed) {
      return;
    }
    this.#closed = true;
    try {
      await this.#native.close();
    } catch (error) {
      throw mapError(error);
    }
  }

  async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }

  [Symbol.dispose](): void {
    void this.close();
  }

  #requireOpen(): void {
    if (this.closed) {
      throw new EmbeddingClosedError();
    }
  }
}

function resolveRuntime(explicitPath: string | undefined): {
  path?: string;
  packageLocal: boolean;
} {
  if (explicitPath !== undefined) {
    return { path: explicitPath, packageLocal: false };
  }
  if (existsSync(packageRuntimePath)) {
    return { path: packageRuntimePath, packageLocal: true };
  }
  return { packageLocal: false };
}

function validateText(text: string): void {
  if (text.trim().length === 0) {
    throw new EmbeddingInputError("Input text cannot be empty.");
  }
}

function validateEmbedding(value: Float32Array): Float32Array {
  if (!(value instanceof Float32Array) || value.length !== 384) {
    throw new EmbeddingKitError(
      "Native provider did not return a 384-value Float32Array.",
      "RK_EMBEDDING_OUTPUT"
    );
  }
  let squaredNorm = 0;
  for (const component of value) {
    if (!Number.isFinite(component)) {
      throw new EmbeddingKitError(
        "Native provider returned a non-finite embedding.",
        "RK_EMBEDDING_OUTPUT"
      );
    }
    squaredNorm += component * component;
  }
  const norm = Math.sqrt(squaredNorm);
  if (Math.abs(norm - 1) > 1e-4) {
    throw new EmbeddingKitError(
      `Native provider returned an embedding with L2 norm ${String(norm)}.`,
      "RK_EMBEDDING_OUTPUT"
    );
  }
  return value;
}

function validateModelInfo(info: NativeModelInfo): EmbeddingModelInfo {
  if (
    info.dimension !== 384 ||
    info.maxInputTokens !== 256 ||
    !info.normalized ||
    info.precision !== "fp32" ||
    info.runtime !== "onnxruntime" ||
    info.runtimeVersion !== "1.24.3"
  ) {
    throw new EmbeddingKitError(
      "Native provider does not satisfy the production FP32 embedding contract.",
      "RK_EMBEDDING_MODEL"
    );
  }
  return info as EmbeddingModelInfo;
}

function mapError(error: unknown): EmbeddingKitError {
  if (error instanceof EmbeddingKitError) {
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  const match = /^(RK_EMBEDDING_[A-Z]+):\s*(.*)$/s.exec(message);
  const code = match?.[1] as EmbeddingErrorCode | undefined;
  const detail = match?.[2] ?? message;
  if (code === "RK_EMBEDDING_INPUT") {
    return new EmbeddingInputError(detail, error);
  }
  if (code === "RK_EMBEDDING_CLOSED") {
    return new EmbeddingClosedError(detail, error);
  }
  return new EmbeddingKitError(
    detail,
    code !== undefined && errorCodes.has(code)
      ? code
      : "RK_EMBEDDING_STATE",
    error
  );
}
