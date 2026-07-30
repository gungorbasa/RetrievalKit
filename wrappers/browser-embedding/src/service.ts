import { acquireArtifacts, defaultArtifactFetcher, type ArtifactFetcher } from "./acquire.js";
import {
  PINNED_ARTIFACTS,
  type ArtifactSpec
} from "./constants.js";
import {
  cancelled,
  EmbeddingClosedError,
  EmbeddingInputError,
  EmbeddingRuntimeError
} from "./errors.js";
import {
  assertEmbedding,
  onnxRuntimeFactory,
  type EmbeddingRuntime,
  type EmbeddingRuntimeFactory
} from "./runtime.js";
import {
  BrowserCacheArtifactStore,
  type ArtifactStore
} from "./store.js";
import type { ExecutionPreference, ExecutionProvider } from "./types.js";

export interface EmbeddingWorkerDependencies {
  readonly artifacts?: readonly ArtifactSpec[];
  readonly fetcher?: ArtifactFetcher;
  readonly createStore?: (cacheName?: string) => ArtifactStore;
  readonly runtimeFactory?: EmbeddingRuntimeFactory;
}

export interface ServiceLoadOptions {
  readonly cacheName?: string;
  readonly localOnly: boolean;
  readonly execution: ExecutionPreference;
}

export interface ServicePrefetchOptions {
  readonly cacheName?: string;
  readonly localOnly: boolean;
}

export class EmbeddingWorkerService {
  readonly #artifacts: readonly ArtifactSpec[];
  readonly #fetcher: ArtifactFetcher;
  readonly #createStore: (cacheName?: string) => ArtifactStore;
  readonly #runtimeFactory: EmbeddingRuntimeFactory;
  #runtime: EmbeddingRuntime | undefined;
  #closed = false;

  public constructor(dependencies: EmbeddingWorkerDependencies = {}) {
    this.#artifacts = dependencies.artifacts ?? PINNED_ARTIFACTS;
    this.#fetcher = dependencies.fetcher ?? defaultArtifactFetcher;
    this.#createStore =
      dependencies.createStore ??
      ((cacheName) =>
        cacheName === undefined
          ? new BrowserCacheArtifactStore()
          : new BrowserCacheArtifactStore(cacheName));
    this.#runtimeFactory = dependencies.runtimeFactory ?? onnxRuntimeFactory;
  }

  public get provider(): ExecutionProvider {
    return this.#loadedRuntime().provider;
  }

  public async prefetch(
    options: ServicePrefetchOptions,
    signal?: AbortSignal
  ): Promise<void> {
    this.#requireOpen();
    await this.#acquire(options.cacheName, options.localOnly, signal);
  }

  public async load(options: ServiceLoadOptions, signal?: AbortSignal): Promise<void> {
    this.#requireOpen();
    if (this.#runtime !== undefined) {
      throw new EmbeddingRuntimeError("This Worker already owns a loaded embedder.");
    }
    const acquired = await this.#acquire(options.cacheName, options.localOnly, signal);
    cancelled(signal);
    const runtime = await this.#runtimeFactory.create(
      await acquired.read("onnx/all-MiniLM-L6-v2-fp32.onnx"),
      await acquired.read("tokenizer/tokenizer.json"),
      await acquired.read("tokenizer/tokenizer_config.json"),
      options.execution
    );
    if (signal?.aborted === true || this.#closed) {
      await runtime.close();
      cancelled(signal);
      throw new EmbeddingClosedError();
    }
    this.#runtime = runtime;
  }

  public async embed(text: string, signal?: AbortSignal): Promise<Float32Array> {
    const runtime = this.#loadedRuntime();
    validateText(text);
    cancelled(signal);
    const output = await runtime.embed([text], signal);
    assertEmbedding(output);
    return output;
  }

  public async embedBatch(
    texts: readonly string[],
    signal?: AbortSignal
  ): Promise<Float32Array> {
    const runtime = this.#loadedRuntime();
    if (texts.length === 0) throw new EmbeddingInputError("Embedding batch cannot be empty.");
    texts.forEach(validateText);
    cancelled(signal);
    const output = await runtime.embed(texts, signal);
    if (output.length !== texts.length * 384) {
      throw new EmbeddingRuntimeError(
        `Runtime returned ${output.length} values for ${texts.length} inputs.`
      );
    }
    for (let row = 0; row < texts.length; row += 1) {
      assertEmbedding(output.subarray(row * 384, (row + 1) * 384));
    }
    return output;
  }

  public async close(): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    const runtime = this.#runtime;
    this.#runtime = undefined;
    await runtime?.close();
  }

  async #acquire(cacheName: string | undefined, localOnly: boolean, signal?: AbortSignal) {
    const store = this.#createStore(cacheName);
    return await acquireArtifacts({
      artifacts: this.#artifacts,
      store,
      fetcher: this.#fetcher,
      localOnly,
      ...(signal === undefined ? {} : { signal })
    });
  }

  #requireOpen(): void {
    if (this.#closed) throw new EmbeddingClosedError();
  }

  #loadedRuntime(): EmbeddingRuntime {
    this.#requireOpen();
    if (this.#runtime === undefined) {
      throw new EmbeddingRuntimeError("The Worker embedder has not been loaded.");
    }
    return this.#runtime;
  }
}

function validateText(text: string): void {
  if (typeof text !== "string" || text.trim().length === 0) {
    throw new EmbeddingInputError("Embedding text cannot be empty.");
  }
}
