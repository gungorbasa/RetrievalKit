import { EMBEDDING_DIMENSION, MODEL_INFO } from "./constants.js";
import {
  EmbeddingClosedError,
  EmbeddingOutputError
} from "./errors.js";
import type {
  WorkerLoadPayload,
  WorkerLoadResult,
  WorkerPrefetchPayload
} from "./protocol.js";
import { EmbeddingWorkerRpc } from "./rpc.js";
import { assertEmbedding, validateEmbedding } from "./runtime.js";
import type {
  BrowserEmbedderLoadOptions,
  BrowserEmbedderPrefetchOptions,
  EmbeddingModelInfo,
  ExecutionProvider,
  OperationControl
} from "./types.js";

/**
 * Worker-owned, local FP32 MiniLM embedder.
 *
 * All acquisition and inference occurs in the caller-supplied dedicated
 * module Worker. RetrievalKit database packages are not imported.
 */
export class BrowserEmbedder implements AsyncDisposable, Disposable {
  readonly #rpc: EmbeddingWorkerRpc;
  public readonly provider: ExecutionProvider;
  #closed = false;

  private constructor(rpc: EmbeddingWorkerRpc, provider: ExecutionProvider) {
    this.#rpc = rpc;
    this.provider = provider;
  }

  public static async load(
    options: BrowserEmbedderLoadOptions
  ): Promise<BrowserEmbedder> {
    const rpc = createRpc(options.worker);
    const payload: WorkerLoadPayload = {
      ...(options.cacheName === undefined ? {} : { cacheName: options.cacheName }),
      localOnly: options.localOnly ?? false,
      execution: options.execution ?? "auto"
    };
    try {
      const result = await rpc.request<WorkerLoadResult>("load", payload, options.signal);
      if (result.provider !== "webgpu" && result.provider !== "wasm") {
        throw new EmbeddingOutputError("Worker returned an invalid execution provider.");
      }
      return new BrowserEmbedder(rpc, result.provider);
    } catch (error) {
      rpc.terminate();
      throw error;
    }
  }

  public static async prefetch(
    options: BrowserEmbedderPrefetchOptions
  ): Promise<void> {
    const rpc = createRpc(options.worker);
    const payload: WorkerPrefetchPayload = {
      ...(options.cacheName === undefined ? {} : { cacheName: options.cacheName }),
      localOnly: options.localOnly ?? false
    };
    try {
      await rpc.request("prefetch", payload, options.signal);
    } finally {
      rpc.terminate();
    }
  }

  public get closed(): boolean {
    return this.#closed || this.#rpc.closed;
  }

  public get modelInfo(): EmbeddingModelInfo {
    this.#requireOpen();
    return MODEL_INFO;
  }

  public async embed(text: string, control: OperationControl = {}): Promise<Float32Array> {
    this.#requireOpen();
    const output = await this.#rpc.request<Float32Array>("embed", text, control.signal);
    assertEmbedding(output);
    return output;
  }

  public async embedBatch(
    texts: readonly string[],
    control: OperationControl = {}
  ): Promise<readonly Float32Array[]> {
    this.#requireOpen();
    const contiguous = await this.#rpc.request<Float32Array>(
      "embedBatch",
      [...texts],
      control.signal
    );
    if (
      !(contiguous instanceof Float32Array) ||
      contiguous.length !== texts.length * EMBEDDING_DIMENSION
    ) {
      throw new EmbeddingOutputError("Worker returned an invalid embedding batch.");
    }
    const rows: Float32Array[] = [];
    for (let row = 0; row < texts.length; row += 1) {
      rows.push(
        validateEmbedding(
          contiguous.subarray(
            row * EMBEDDING_DIMENSION,
            (row + 1) * EMBEDDING_DIMENSION
          )
        )
      );
    }
    return rows;
  }

  public async close(): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    try {
      await this.#rpc.request("close", undefined);
    } finally {
      this.#rpc.terminate();
    }
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  #requireOpen(): void {
    if (this.#closed) throw new EmbeddingClosedError();
    this.#rpc.assertOpen();
  }
}

function createRpc(
  workerOrFactory: BrowserEmbedderLoadOptions["worker"]
): EmbeddingWorkerRpc {
  const worker =
    typeof workerOrFactory === "function" ? workerOrFactory() : workerOrFactory;
  return new EmbeddingWorkerRpc(worker);
}
