import type { MODEL_INFO } from "./constants.js";

export type ExecutionProvider = "webgpu" | "wasm";
export type ExecutionPreference = "auto" | ExecutionProvider;
export type EmbeddingModelInfo = typeof MODEL_INFO;

export interface OperationControl {
  readonly signal?: AbortSignal;
}

export interface BrowserEmbeddingWorkerLike {
  postMessage(message: unknown, transfer?: Transferable[]): void;
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
  terminate?(): void;
}

export interface BrowserEmbedderLoadOptions extends OperationControl {
  readonly worker: BrowserEmbeddingWorkerLike | (() => BrowserEmbeddingWorkerLike);
  readonly cacheName?: string;
  readonly localOnly?: boolean;
  readonly execution?: ExecutionPreference;
}

export interface BrowserEmbedderPrefetchOptions extends OperationControl {
  readonly worker: BrowserEmbeddingWorkerLike | (() => BrowserEmbeddingWorkerLike);
  readonly cacheName?: string;
  readonly localOnly?: boolean;
}
