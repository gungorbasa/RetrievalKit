import { describe, expect, it } from "vitest";
import type {
  BrowserCapabilities,
  RetrievalKitWasmAdapter,
  WasmDatabaseOptions,
  WasmDocumentBatch,
  WasmGraphRecordBatch,
  WasmGraphSelection,
  WasmHandle,
  WasmSearchQuery
} from "../src/adapter.js";
import {
  GraphSelection,
  RetrievalKitBrowser,
  RetrievalKitCancelledError,
  RetrievalKitLifecycleError,
  RetrievalKitWorkerError
} from "../src/index.js";
import type {
  CandidateProjection,
  Filter,
  GraphQuery,
  SearchResult
} from "../src/types.js";
import type {
  WorkerIncomingMessage,
  WorkerLike,
  WorkerScopeLike
} from "../src/protocol.js";
import { installRetrievalKitWorker } from "../src/worker.js";

const capabilities: BrowserCapabilities = {
  execution: "dedicated-worker",
  performanceTier: "portable",
  persistence: false,
  threads: false,
  simd: false,
  structuredDtos: true,
  bulkFloat32Embeddings: true
};

class MockAdapter implements RetrievalKitWasmAdapter {
  public readonly created: WasmDatabaseOptions[] = [];
  public documentBatch?: WasmDocumentBatch;
  public graphBatch?: WasmGraphRecordBatch;
  public lastSearch?: WasmSearchQuery;
  public searchGate?: Promise<void>;
  #nextHandle = 1;

  public async initialize(_signal: AbortSignal): Promise<BrowserCapabilities> {
    return capabilities;
  }

  public async createDatabase(
    request: WasmDatabaseOptions,
    _signal: AbortSignal
  ): Promise<WasmHandle> {
    this.created.push(request);
    return `database-${this.#nextHandle++}`;
  }

  public async addDocuments(
    _handle: WasmHandle,
    documents: WasmDocumentBatch,
    _signal: AbortSignal
  ): Promise<void> {
    this.documentBatch = documents;
  }

  public async addGraphRecords(
    _handle: WasmHandle,
    records: WasmGraphRecordBatch,
    _signal: AbortSignal
  ): Promise<void> {
    this.graphBatch = records;
  }

  public async build(_handle: WasmHandle, _signal: AbortSignal): Promise<void> {}

  public async search(
    _handle: WasmHandle,
    query: WasmSearchQuery,
    signal: AbortSignal
  ): Promise<readonly SearchResult[]> {
    this.lastSearch = query;
    if (this.searchGate !== undefined) {
      await Promise.race([
        this.searchGate,
        new Promise<never>((_resolve, reject) => {
          signal.addEventListener(
            "abort",
            () => reject(new Error("adapter cancelled")),
            { once: true }
          );
        })
      ]);
    }
    return [
      {
        documentId: "document-1",
        text: "result",
        metadata: {},
        score: 1,
        vectorScore: 1,
        trace: { kind: "vector", vectorScore: 1 }
      }
    ];
  }

  public async graphQuery(
    _handle: WasmHandle,
    _query: GraphQuery,
    _signal: AbortSignal
  ): Promise<WasmGraphSelection> {
    return {
      handle: `selection-${this.#nextHandle++}`,
      data: {
        matches: [],
        trace: {
          seedCount: 1,
          visitedStates: 1,
          traversedEdges: 0,
          resultCount: 0,
          diagnostics: 0
        }
      }
    };
  }

  public async projectCandidates(
    _databaseHandle: WasmHandle,
    _selectionHandle: WasmHandle,
    _where: Filter | undefined,
    _signal: AbortSignal
  ): Promise<CandidateProjection> {
    return {
      candidates: [],
      sourceNodes: 0,
      projectedChunksBeforeFilter: 0,
      projectedChunksAfterFilter: 0
    };
  }

  public async close(_handle: WasmHandle): Promise<void> {}
}

describe("RetrievalKitBrowser", () => {
  it("reports capabilities and transfers a contiguous copy of document embeddings", async () => {
    const adapter = new MockAdapter();
    const pair = createWorkerPair();
    installRetrievalKitWorker(adapter, pair.scope);
    const kit = await RetrievalKitBrowser.create({ worker: pair.worker });
    expect(kit.capabilities).toEqual(capabilities);

    const first = new Float32Array([1, 2]);
    const second = new Float32Array([3, 4]);
    const builder = kit.retrievalDatabase({ corpusId: "test", encoding: "f32" });
    await builder.add([
      { id: "a", text: "A", embedding: first },
      { id: "b", text: "B", embedding: second }
    ]);

    expect(first).toEqual(new Float32Array([1, 2]));
    expect(second).toEqual(new Float32Array([3, 4]));
    expect(adapter.documentBatch?.dimension).toBe(2);
    expect(adapter.documentBatch?.embeddings).toEqual(
      new Float32Array([1, 2, 3, 4])
    );
    expect(pair.requestTransferCounts).toContain(1);

    const database = await builder.build();
    const query = new Float32Array([1, 2]);
    await expect(
      database.search({ mode: "vector", embedding: query })
    ).resolves.toHaveLength(1);
    expect(query).toEqual(new Float32Array([1, 2]));
    await database.close();
    await expect(
      database.search({ mode: "text", text: "closed" })
    ).rejects.toBeInstanceOf(RetrievalKitLifecycleError);
    kit.close();
  });

  it("uses one embedding buffer for a graph batch and supports scoped retrieval", async () => {
    const adapter = new MockAdapter();
    const pair = createWorkerPair();
    installRetrievalKitWorker(adapter, pair.scope);
    const kit = await RetrievalKitBrowser.create({ worker: pair.worker });
    const builder = kit.graphRetrievalDatabase({
      corpusId: "graph",
      schema: { recordNodes: [{ recordType: "note", nodeType: "Note" }] }
    });
    await builder.add([
      {
        id: "one",
        type: "note",
        retrieval: {
          kind: "content",
          embedding: new Float32Array([1, 2])
        }
      },
      {
        id: "two",
        type: "note",
        retrieval: {
          kind: "documents",
          documents: [
            {
              id: "chunk",
              text: "chunk",
              embedding: new Float32Array([3, 4])
            }
          ]
        }
      }
    ]);
    expect(adapter.graphBatch?.embeddings).toEqual(
      new Float32Array([1, 2, 3, 4])
    );

    const database = await builder.build();
    const selection = await database.graph.query({
      seed: { kind: "nodes", nodes: [] }
    });
    expect(selection).toBeInstanceOf(GraphSelection);
    await database.retrieval.search({
      mode: "text",
      text: "scope",
      within: selection
    });
    expect(adapter.lastSearch?.within).toMatch(/^selection-/);
    await selection.close();
    await database.close();
    kit.close();
  });

  it("cancels a superseded request and does not deliver its stale result", async () => {
    const adapter = new MockAdapter();
    let release!: () => void;
    adapter.searchGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const pair = createWorkerPair();
    installRetrievalKitWorker(adapter, pair.scope);
    const kit = await RetrievalKitBrowser.create({ worker: pair.worker });
    const builder = kit.retrievalDatabase({ corpusId: "cancel" });
    const database = await builder.build();

    const first = database.search(
      { mode: "text", text: "first" },
      { supersedeKey: "typeahead" }
    );
    const second = database.search(
      { mode: "text", text: "second" },
      { supersedeKey: "typeahead" }
    );
    await expect(first).rejects.toBeInstanceOf(RetrievalKitCancelledError);
    release();
    await expect(second).resolves.toHaveLength(1);
    await database.close();
    kit.close();
  });

  it("rejects pending and future requests when the Worker crashes", async () => {
    const adapter = new MockAdapter();
    let release!: () => void;
    adapter.searchGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const pair = createWorkerPair();
    installRetrievalKitWorker(adapter, pair.scope);
    const kit = await RetrievalKitBrowser.create({ worker: pair.worker });
    const builder = kit.retrievalDatabase({ corpusId: "crash" });
    const database = await builder.build();
    const pending = database.search({ mode: "text", text: "pending" });

    pair.emitError("WASM Worker terminated");
    await expect(pending).rejects.toBeInstanceOf(RetrievalKitWorkerError);
    await expect(
      database.search({ mode: "text", text: "future" })
    ).rejects.toBeInstanceOf(RetrievalKitWorkerError);
    release();
  });

  it("rejects pending requests when Worker message deserialization fails", async () => {
    const adapter = new MockAdapter();
    let release!: () => void;
    adapter.searchGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const pair = createWorkerPair();
    installRetrievalKitWorker(adapter, pair.scope);
    const kit = await RetrievalKitBrowser.create({ worker: pair.worker });
    const database = await kit
      .retrievalDatabase({ corpusId: "message-error" })
      .build();
    const pending = database.search({ mode: "text", text: "pending" });

    pair.emitMessageError();
    await expect(pending).rejects.toMatchObject({
      name: "RetrievalKitWorkerError",
      code: "RK_WORKER_MESSAGE"
    });
    release();
  });
});

interface WorkerPair {
  readonly worker: WorkerLike;
  readonly scope: WorkerScopeLike;
  readonly requestTransferCounts: number[];
  emitError(message: string): void;
  emitMessageError(): void;
}

function createWorkerPair(): WorkerPair {
  const clientListeners = new Map<string, Set<EventListener>>();
  const workerListeners = new Set<
    (event: MessageEvent<WorkerIncomingMessage>) => void
  >();
  const requestTransferCounts: number[] = [];

  return {
    requestTransferCounts,
    emitError(message) {
      const event = new Event("error") as ErrorEvent;
      Object.defineProperty(event, "message", { value: message });
      for (const listener of clientListeners.get("error") ?? []) listener(event);
    },
    emitMessageError() {
      const event = new MessageEvent("messageerror");
      for (const listener of clientListeners.get("messageerror") ?? []) {
        listener(event);
      }
    },
    worker: {
      postMessage(message, transfer = []) {
        requestTransferCounts.push(transfer.length);
        const copied = structuredClone(message, {
          transfer
        }) as WorkerIncomingMessage;
        queueMicrotask(() => {
          for (const listener of workerListeners) {
            listener(new MessageEvent("message", { data: copied }));
          }
        });
      },
      addEventListener(type, listener) {
        const listeners = clientListeners.get(type) ?? new Set<EventListener>();
        listeners.add(listener);
        clientListeners.set(type, listeners);
      },
      removeEventListener(type, listener) {
        clientListeners.get(type)?.delete(listener);
      }
    },
    scope: {
      postMessage(message, transfer = []) {
        const copied = structuredClone(message, { transfer });
        queueMicrotask(() => {
          for (const listener of clientListeners.get("message") ?? []) {
            listener(new MessageEvent("message", { data: copied }));
          }
        });
      },
      addEventListener(_type, listener) {
        workerListeners.add(listener);
      },
      removeEventListener(_type, listener) {
        workerListeners.delete(listener);
      }
    }
  };
}
