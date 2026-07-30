import type { SerializedEmbeddingError } from "./errors.js";
import type {
  BrowserEmbeddingWorkerLike,
  ExecutionPreference,
  ExecutionProvider
} from "./types.js";

export type WorkerMethod = "prefetch" | "load" | "embed" | "embedBatch" | "close";

export interface WorkerLoadPayload {
  readonly cacheName?: string;
  readonly localOnly: boolean;
  readonly execution: ExecutionPreference;
}

export interface WorkerPrefetchPayload {
  readonly cacheName?: string;
  readonly localOnly: boolean;
}

export interface WorkerRequest {
  readonly kind: "request";
  readonly id: number;
  readonly method: WorkerMethod;
  readonly payload: unknown;
}

export interface WorkerCancel {
  readonly kind: "cancel";
  readonly id: number;
}

export type WorkerIncomingMessage = WorkerRequest | WorkerCancel;

export type WorkerResponse =
  | {
      readonly kind: "response";
      readonly id: number;
      readonly ok: true;
      readonly value: unknown;
      readonly transfer?: readonly ArrayBuffer[];
    }
  | {
      readonly kind: "response";
      readonly id: number;
      readonly ok: false;
      readonly error: SerializedEmbeddingError;
    };

export interface WorkerLoadResult {
  readonly provider: ExecutionProvider;
}

export interface WorkerScopeLike {
  postMessage(message: WorkerResponse, transfer?: Transferable[]): void;
  addEventListener(
    type: "message",
    listener: (event: MessageEvent<WorkerIncomingMessage>) => void
  ): void;
  removeEventListener(
    type: "message",
    listener: (event: MessageEvent<WorkerIncomingMessage>) => void
  ): void;
}

export type { BrowserEmbeddingWorkerLike };
