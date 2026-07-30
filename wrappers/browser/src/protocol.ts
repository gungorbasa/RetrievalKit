import type { SerializedError } from "./errors.js";

export type WorkerMethod =
  | "initialize"
  | "createDatabase"
  | "addDocuments"
  | "addGraphRecords"
  | "build"
  | "search"
  | "graphQuery"
  | "projectCandidates"
  | "closeHandle";

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
    }
  | {
      readonly kind: "response";
      readonly id: number;
      readonly ok: false;
      readonly error: SerializedError;
    };

export interface WorkerLike {
  postMessage(message: unknown, transfer?: Transferable[]): void;
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
  terminate?(): void;
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
