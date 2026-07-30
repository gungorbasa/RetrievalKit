import {
  deserializeError,
  RetrievalKitCancelledError,
  RetrievalKitLifecycleError,
  RetrievalKitWorkerError
} from "./errors.js";
import type {
  WorkerLike,
  WorkerMethod,
  WorkerRequest,
  WorkerResponse
} from "./protocol.js";
import type { SearchControl } from "./types.js";

interface PendingRequest {
  readonly resolve: (value: unknown) => void;
  readonly reject: (reason: unknown) => void;
  readonly supersedeKey?: string;
  readonly removeAbortListener?: () => void;
}

export class WorkerRpcClient {
  readonly #worker: WorkerLike;
  readonly #pending = new Map<number, PendingRequest>();
  readonly #superseded = new Map<string, number>();
  #nextId = 1;
  #closed = false;
  #failure?: RetrievalKitWorkerError;

  public constructor(worker: WorkerLike) {
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
    if (this.#closed) {
      throw new RetrievalKitLifecycleError(
        "RetrievalKit browser client is closed.",
        "RK_LIFECYCLE"
      );
    }
  }

  public request<T>(
    method: WorkerMethod,
    payload: unknown,
    transfer: Transferable[] = [],
    control: SearchControl = {}
  ): Promise<T> {
    try {
      this.assertOpen();
    } catch (error) {
      return Promise.reject(
        error instanceof Error
          ? error
          : new RetrievalKitWorkerError(String(error), "RK_WORKER")
      );
    }
    if (control.signal?.aborted === true) {
      return Promise.reject(cancelled(control.signal.reason));
    }

    const id = this.#nextId++;
    if (control.supersedeKey !== undefined) {
      const previous = this.#superseded.get(control.supersedeKey);
      if (previous !== undefined) this.#cancel(previous, "Request was superseded.");
      this.#superseded.set(control.supersedeKey, id);
    }

    return new Promise<T>((resolve, reject) => {
      const abort = (): void => {
        this.#cancel(id, control.signal?.reason);
      };
      if (control.signal !== undefined) {
        control.signal.addEventListener("abort", abort, { once: true });
      }
      const pending: PendingRequest = {
        resolve: (value) => resolve(value as T),
        reject,
        ...(control.supersedeKey === undefined
          ? {}
          : { supersedeKey: control.supersedeKey }),
        ...(control.signal === undefined
          ? {}
          : {
              removeAbortListener: () => {
                control.signal?.removeEventListener("abort", abort);
              }
            })
      };
      this.#pending.set(id, pending);
      const message: WorkerRequest = { kind: "request", id, method, payload };
      try {
        this.#worker.postMessage(message, transfer);
      } catch (error) {
        this.#settle(
          id,
          false,
          error instanceof Error
            ? error
            : new RetrievalKitWorkerError(String(error), "RK_WORKER")
        );
      }
    });
  }

  public close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#removeListeners();
    for (const id of [...this.#pending.keys()]) {
      this.#cancel(id, "RetrievalKit browser client was closed.");
    }
    this.#worker.terminate?.();
  }

  readonly #onMessage: EventListener = (event): void => {
    const response: unknown = (event as MessageEvent<unknown>).data;
    if (!isWorkerResponse(response)) return;
    if (response.ok) {
      this.#settle(response.id, true, response.value);
    } else {
      this.#settle(response.id, false, deserializeError(response.error));
    }
  };

  readonly #onError: EventListener = (event): void => {
    const errorEvent = event as ErrorEvent;
    this.#fail(
      new RetrievalKitWorkerError(
        errorEvent.message === ""
          ? "RetrievalKit Worker crashed."
          : `RetrievalKit Worker crashed: ${errorEvent.message}`,
        "RK_WORKER"
      )
    );
  };

  readonly #onMessageError: EventListener = (): void => {
    this.#fail(
      new RetrievalKitWorkerError(
        "RetrievalKit Worker could not deserialize a message.",
        "RK_WORKER_MESSAGE"
      )
    );
  };

  #cancel(id: number, reason: unknown): void {
    if (!this.#pending.has(id)) return;
    this.#worker.postMessage({ kind: "cancel", id });
    this.#settle(id, false, cancelled(reason));
  }

  #settle(id: number, success: boolean, value: unknown): void {
    const pending = this.#pending.get(id);
    if (pending === undefined) return;
    this.#pending.delete(id);
    pending.removeAbortListener?.();
    if (
      pending.supersedeKey !== undefined &&
      this.#superseded.get(pending.supersedeKey) === id
    ) {
      this.#superseded.delete(pending.supersedeKey);
    }
    if (success) pending.resolve(value);
    else pending.reject(value);
  }

  #fail(error: RetrievalKitWorkerError): void {
    if (this.#failure !== undefined || this.#closed) return;
    this.#failure = error;
    this.#closed = true;
    this.#removeListeners();
    for (const id of [...this.#pending.keys()]) {
      this.#settle(id, false, error);
    }
    this.#worker.terminate?.();
  }

  #removeListeners(): void {
    this.#worker.removeEventListener("message", this.#onMessage);
    this.#worker.removeEventListener("error", this.#onError);
    this.#worker.removeEventListener("messageerror", this.#onMessageError);
  }
}

function isWorkerResponse(value: unknown): value is WorkerResponse {
  if (value === null || typeof value !== "object") return false;
  const candidate = value as Partial<WorkerResponse>;
  return candidate.kind === "response" && typeof candidate.id === "number";
}

function cancelled(reason: unknown): RetrievalKitCancelledError {
  const suffix =
    reason instanceof Error
      ? ` ${reason.message}`
      : typeof reason === "string"
        ? ` ${reason}`
        : "";
  return new RetrievalKitCancelledError(
    `RetrievalKit request was cancelled.${suffix}`,
    "RK_CANCELLED"
  );
}
