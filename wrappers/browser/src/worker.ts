import type {
  RetrievalKitWasmAdapter,
  WasmDatabaseOptions,
  WasmDocumentBatch,
  WasmGraphRecordBatch,
  WasmHandle,
  WasmSearchQuery
} from "./adapter.js";
import { serializeError } from "./errors.js";
import type {
  WorkerIncomingMessage,
  WorkerRequest,
  WorkerResponse,
  WorkerScopeLike
} from "./protocol.js";
import type { Filter, GraphQuery } from "./types.js";

/**
 * Installs the RetrievalKit protocol in a dedicated Worker. Call this once from
 * the application-owned Worker entry after constructing the generated WASM
 * adapter.
 */
export function installRetrievalKitWorker(
  adapter: RetrievalKitWasmAdapter,
  scope: WorkerScopeLike = self
): () => void {
  const active = new Map<number, AbortController>();

  const listener = (event: MessageEvent<WorkerIncomingMessage>): void => {
    const message = event.data;
    if (message.kind === "cancel") {
      active.get(message.id)?.abort();
      return;
    }
    const controller = new AbortController();
    active.set(message.id, controller);
    void dispatch(adapter, message, controller.signal)
      .then((value) => {
        if (controller.signal.aborted) return;
        const response: WorkerResponse = {
          kind: "response",
          id: message.id,
          ok: true,
          value
        };
        scope.postMessage(response);
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        scope.postMessage({
          kind: "response",
          id: message.id,
          ok: false,
          error: serializeError(error)
        });
      })
      .finally(() => active.delete(message.id));
  };

  scope.addEventListener("message", listener);
  return () => {
    scope.removeEventListener("message", listener);
    for (const controller of active.values()) controller.abort();
    active.clear();
  };
}

async function dispatch(
  adapter: RetrievalKitWasmAdapter,
  request: WorkerRequest,
  signal: AbortSignal
): Promise<unknown> {
  switch (request.method) {
    case "initialize":
      return adapter.initialize(signal);
    case "createDatabase":
      return adapter.createDatabase(request.payload as WasmDatabaseOptions, signal);
    case "addDocuments": {
      const payload = request.payload as {
        readonly handle: WasmHandle;
        readonly documents: WasmDocumentBatch;
      };
      return adapter.addDocuments(payload.handle, payload.documents, signal);
    }
    case "addGraphRecords": {
      const payload = request.payload as {
        readonly handle: WasmHandle;
        readonly records: WasmGraphRecordBatch;
      };
      return adapter.addGraphRecords(payload.handle, payload.records, signal);
    }
    case "build": {
      const { handle } = request.payload as { readonly handle: WasmHandle };
      return adapter.build(handle, signal);
    }
    case "search": {
      const payload = request.payload as {
        readonly handle: WasmHandle;
        readonly query: WasmSearchQuery;
      };
      return adapter.search(payload.handle, payload.query, signal);
    }
    case "graphQuery": {
      const payload = request.payload as {
        readonly handle: WasmHandle;
        readonly query: GraphQuery;
      };
      return adapter.graphQuery(payload.handle, payload.query, signal);
    }
    case "projectCandidates": {
      const payload = request.payload as {
        readonly databaseHandle: WasmHandle;
        readonly selectionHandle: WasmHandle;
        readonly where?: Filter;
      };
      return adapter.projectCandidates(
        payload.databaseHandle,
        payload.selectionHandle,
        payload.where,
        signal
      );
    }
    case "closeHandle": {
      const { handle } = request.payload as { readonly handle: WasmHandle };
      return adapter.close(handle);
    }
  }
}
