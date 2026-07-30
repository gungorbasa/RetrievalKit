import {
  deserializeError,
  EmbeddingCancelledError,
  EmbeddingClosedError,
  EmbeddingWorkerError
} from "./errors.js";
import type {
  BrowserEmbeddingWorkerLike,
  WorkerMethod,
  WorkerRequest,
  WorkerResponse
} from "./protocol.js";

interface Pending {
  readonly resolve: (value: unknown) => void;
  readonly reject: (reason: unknown) => void;
  readonly removeAbort?: () => void;
}

export class EmbeddingWorkerRpc {
  readonly #worker: BrowserEmbeddingWorkerLike;
  readonly #pending = new Map<number, Pending>();
  #nextId = 1;
  #closed = false;
  #failure?: EmbeddingWorkerError;

  public constructor(worker: BrowserEmbeddingWorkerLike) {
    this.#worker = worker;
    worker.addEventListener("message", this.#onMessage);
    worker.addEventListener("error", this.#onError);
    worker.addEventListener("messageerror", this.#onMessageError);
  }

  public get closed(): boolean {
    return this.#closed;
  }

  public assertOpen(): void {
    if (this.#failure !== undefined) throw this.#failure;
    if (this.#closed) throw new EmbeddingClosedError();
  }

  public request<T>(
    method: WorkerMethod,
    payload: unknown,
    signal?: AbortSignal
  ): Promise<T> {
    try {
      this.assertOpen();
    } catch (error) {
      return Promise.reject(
        error instanceof Error
          ? error
          : new EmbeddingWorkerError("Embedding Worker is unavailable.", error)
      );
    }
    if (signal?.aborted === true) return Promise.reject(new EmbeddingCancelledError());
    const id = this.#nextId++;
    return new Promise<T>((resolve, reject) => {
      const abort = (): void => {
        this.#worker.postMessage({ kind: "cancel", id });
        this.#settle(id, false, new EmbeddingCancelledError());
      };
      signal?.addEventListener("abort", abort, { once: true });
      this.#pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        ...(signal === undefined
          ? {}
          : { removeAbort: () => signal.removeEventListener("abort", abort) })
      });
      const request: WorkerRequest = { kind: "request", id, method, payload };
      try {
        this.#worker.postMessage(request);
      } catch (error) {
        this.#settle(
          id,
          false,
          new EmbeddingWorkerError("Failed to post a message to the embedding Worker.", error)
        );
      }
    });
  }

  public terminate(reason = new EmbeddingClosedError()): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#removeListeners();
    for (const id of [...this.#pending.keys()]) this.#settle(id, false, reason);
    this.#worker.terminate?.();
  }

  readonly #onMessage: EventListener = (event): void => {
    const response = (event as MessageEvent<unknown>).data;
    if (!isResponse(response)) return;
    this.#settle(
      response.id,
      response.ok,
      response.ok ? response.value : deserializeError(response.error)
    );
  };

  readonly #onError: EventListener = (event): void => {
    const error = event as ErrorEvent;
    this.#fail(
      new EmbeddingWorkerError(
        error.message === ""
          ? "The embedding Worker crashed."
          : `The embedding Worker crashed: ${error.message}`
      )
    );
  };

  readonly #onMessageError: EventListener = (): void => {
    this.#fail(new EmbeddingWorkerError("The embedding Worker message could not be decoded."));
  };

  #settle(id: number, success: boolean, value: unknown): void {
    const pending = this.#pending.get(id);
    if (pending === undefined) return;
    this.#pending.delete(id);
    pending.removeAbort?.();
    if (success) pending.resolve(value);
    else pending.reject(value);
  }

  #fail(error: EmbeddingWorkerError): void {
    if (this.#closed) return;
    this.#failure = error;
    this.#closed = true;
    this.#removeListeners();
    for (const id of [...this.#pending.keys()]) this.#settle(id, false, error);
    this.#worker.terminate?.();
  }

  #removeListeners(): void {
    this.#worker.removeEventListener("message", this.#onMessage);
    this.#worker.removeEventListener("error", this.#onError);
    this.#worker.removeEventListener("messageerror", this.#onMessageError);
  }
}

function isResponse(value: unknown): value is WorkerResponse {
  if (value === null || typeof value !== "object") return false;
  const candidate = value as Partial<WorkerResponse>;
  return candidate.kind === "response" && typeof candidate.id === "number";
}
