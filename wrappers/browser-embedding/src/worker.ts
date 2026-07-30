import { EmbeddingCancelledError, serializeError } from "./errors.js";
import type {
  WorkerIncomingMessage,
  WorkerLoadPayload,
  WorkerPrefetchPayload,
  WorkerRequest,
  WorkerResponse,
  WorkerScopeLike
} from "./protocol.js";
import {
  EmbeddingWorkerService,
  type EmbeddingWorkerDependencies
} from "./service.js";

/**
 * Installs the browser embedding protocol in the current dedicated Worker.
 * Applications should call this once from their module Worker entry.
 */
export function installBrowserEmbeddingWorker(
  dependencies: EmbeddingWorkerDependencies = {},
  scope: WorkerScopeLike = self
): () => void {
  const service = new EmbeddingWorkerService(dependencies);
  const controllers = new Map<number, AbortController>();
  let tail = Promise.resolve();
  let closed = false;

  const listener = (event: MessageEvent<WorkerIncomingMessage>): void => {
    const message = event.data;
    if (message.kind === "cancel") {
      controllers.get(message.id)?.abort();
      return;
    }
    const controller = new AbortController();
    controllers.set(message.id, controller);
    const task = tail.then(async () => {
      if (closed && message.method !== "close") {
        throw new Error("Embedding Worker is closed.");
      }
      return await dispatch(service, message, controller.signal);
    });
    tail = task.then(
      () => undefined,
      () => undefined
    );
    void task
      .then((value) => {
        if (controller.signal.aborted) throw new EmbeddingCancelledError();
        const transfer = transferables(value);
        const response: WorkerResponse = {
          kind: "response",
          id: message.id,
          ok: true,
          value
        };
        scope.postMessage(response, transfer);
        if (message.method === "close") closed = true;
      })
      .catch((error: unknown) => reply(scope, message.id, false, error))
      .finally(() => controllers.delete(message.id));
  };
  scope.addEventListener("message", listener);
  return () => {
    scope.removeEventListener("message", listener);
    for (const controller of controllers.values()) controller.abort();
    void service.close();
  };
}

async function dispatch(
  service: EmbeddingWorkerService,
  request: WorkerRequest,
  signal: AbortSignal
): Promise<unknown> {
  switch (request.method) {
    case "prefetch": {
      const payload = request.payload as WorkerPrefetchPayload;
      await service.prefetch(payload, signal);
      return undefined;
    }
    case "load": {
      const payload = request.payload as WorkerLoadPayload;
      await service.load(payload, signal);
      return { provider: service.provider };
    }
    case "embed":
      return await service.embed(request.payload as string, signal);
    case "embedBatch":
      return await service.embedBatch(request.payload as readonly string[], signal);
    case "close":
      await service.close();
      return undefined;
  }
}

function transferables(value: unknown): Transferable[] {
  if (value instanceof Float32Array) return [value.buffer];
  return [];
}

function reply(
  scope: WorkerScopeLike,
  id: number,
  success: boolean,
  value: unknown
): void {
  if (success) {
    scope.postMessage({ kind: "response", id, ok: true, value });
  } else {
    scope.postMessage({
      kind: "response",
      id,
      ok: false,
      error: serializeError(value)
    });
  }
}

export type { EmbeddingWorkerDependencies } from "./service.js";
export type { ArtifactFetcher } from "./acquire.js";
export type { ArtifactStore } from "./store.js";
export type { EmbeddingRuntime, EmbeddingRuntimeFactory } from "./runtime.js";
