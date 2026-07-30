import {
  graphRecordTransferables,
  toWasmDocumentBatch,
  toWasmGraphRecords,
  type BrowserCapabilities,
  type WasmGraphSelection,
  type WasmHandle,
  type WasmSearchQuery
} from "./adapter.js";
import {
  RetrievalKitInputError,
  RetrievalKitLifecycleError
} from "./errors.js";
import type { WorkerLike } from "./protocol.js";
import { WorkerRpcClient } from "./rpc-client.js";
import type {
  CandidateProjection,
  DocumentInput,
  Filter,
  GraphBuilderOptions,
  GraphOnlyRecordInput,
  GraphQuery,
  GraphRetrievalBuilderOptions,
  GraphRetrievalRecordInput,
  GraphSelectionData,
  GraphSelectionReference,
  RetrievalBuilderOptions,
  SearchControl,
  SearchQuery,
  SearchResult
} from "./types.js";

export interface RetrievalKitBrowserOptions {
  /**
   * A dedicated module Worker whose entry calls installRetrievalKitWorker().
   * A factory is preferred so this client owns Worker termination.
   */
  readonly worker: WorkerLike | (() => WorkerLike);
}

export class RetrievalKitBrowser {
  readonly #rpc: WorkerRpcClient;
  public readonly capabilities: BrowserCapabilities;

  private constructor(rpc: WorkerRpcClient, capabilities: BrowserCapabilities) {
    this.#rpc = rpc;
    this.capabilities = capabilities;
  }

  public static async create(
    options: RetrievalKitBrowserOptions
  ): Promise<RetrievalKitBrowser> {
    const worker =
      typeof options.worker === "function" ? options.worker() : options.worker;
    const rpc = new WorkerRpcClient(worker);
    try {
      const capabilities = await rpc.request<BrowserCapabilities>(
        "initialize",
        undefined
      );
      return new RetrievalKitBrowser(rpc, capabilities);
    } catch (error) {
      rpc.close();
      throw error;
    }
  }

  public retrievalDatabase(
    options: RetrievalBuilderOptions
  ): RetrievalDatabaseBuilder {
    this.#requireOpen();
    return new RetrievalDatabaseBuilder(this.#rpc, options);
  }

  public graphDatabase(options: GraphBuilderOptions): GraphDatabaseBuilder {
    this.#requireOpen();
    return new GraphDatabaseBuilder(this.#rpc, options);
  }

  public graphRetrievalDatabase(
    options: GraphRetrievalBuilderOptions
  ): GraphRetrievalDatabaseBuilder {
    this.#requireOpen();
    return new GraphRetrievalDatabaseBuilder(this.#rpc, options);
  }

  public get closed(): boolean {
    return this.#rpc.closed;
  }

  public close(): void {
    this.#rpc.close();
  }

  public [Symbol.dispose](): void {
    this.close();
  }

  #requireOpen(): void {
    this.#rpc.assertOpen();
  }
}

abstract class BuilderLifecycle {
  protected readonly rpc: WorkerRpcClient;
  readonly #handle: Promise<WasmHandle>;
  #consumed = false;
  #transferred = false;
  #closing?: Promise<void>;

  protected constructor(
    rpc: WorkerRpcClient,
    request:
      | {
          readonly kind: "retrieval";
          readonly options: RetrievalBuilderOptions;
        }
      | { readonly kind: "graph"; readonly options: GraphBuilderOptions }
      | {
          readonly kind: "graphRetrieval";
          readonly options: GraphRetrievalBuilderOptions;
        }
  ) {
    this.rpc = rpc;
    this.#handle = rpc.request<WasmHandle>("createDatabase", request);
  }

  protected requireActive(): void {
    if (this.#consumed) lifecycle(`${this.constructor.name} has already been consumed.`);
  }

  protected activeHandle(): Promise<WasmHandle> {
    this.requireActive();
    return this.#handle;
  }

  protected async finish<T>(
    create: (handle: WasmHandle) => T
  ): Promise<T> {
    this.requireActive();
    try {
      const handle = await this.#handle;
      await this.rpc.request("build", { handle });
      this.#consumed = true;
      this.#transferred = true;
      return create(handle);
    } catch (error) {
      this.#consumed = true;
      throw error;
    }
  }

  public close(): Promise<void> {
    if (this.#transferred) return Promise.resolve();
    this.#consumed = true;
    this.#closing ??= this.#handle.then((handle) =>
      this.rpc.request("closeHandle", { handle })
    );
    return this.#closing;
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

export class RetrievalDatabaseBuilder extends BuilderLifecycle {
  public constructor(rpc: WorkerRpcClient, options: RetrievalBuilderOptions) {
    super(rpc, { kind: "retrieval", options });
  }

  public async add(documents: Iterable<DocumentInput>): Promise<void> {
    const values = [...documents];
    if (values.length === 0) {
      this.requireActive();
      return;
    }
    const batch = toWasmDocumentBatch(values);
    const handle = await this.activeHandle();
    await this.rpc.request(
      "addDocuments",
      { handle, documents: batch },
      [batch.embeddings.buffer]
    );
  }

  public build(): Promise<RetrievalDatabase> {
    return this.finish((handle) => new RetrievalDatabase(this.rpc, handle));
  }
}

export class GraphDatabaseBuilder extends BuilderLifecycle {
  public constructor(rpc: WorkerRpcClient, options: GraphBuilderOptions) {
    super(rpc, { kind: "graph", options });
  }

  public async add(records: Iterable<GraphOnlyRecordInput>): Promise<void> {
    const values = [...records];
    if (values.length === 0) {
      this.requireActive();
      return;
    }
    const batch = toWasmGraphRecords(values);
    const handle = await this.activeHandle();
    await this.rpc.request(
      "addGraphRecords",
      { handle, records: batch },
      graphRecordTransferables(batch)
    );
  }

  public build(): Promise<GraphDatabase> {
    return this.finish((handle) => new GraphDatabase(this.rpc, handle));
  }
}

export class GraphRetrievalDatabaseBuilder extends BuilderLifecycle {
  public constructor(
    rpc: WorkerRpcClient,
    options: GraphRetrievalBuilderOptions
  ) {
    super(rpc, { kind: "graphRetrieval", options });
  }

  public async add(records: Iterable<GraphRetrievalRecordInput>): Promise<void> {
    const values = [...records];
    if (values.length === 0) {
      this.requireActive();
      return;
    }
    const batch = toWasmGraphRecords(values);
    const handle = await this.activeHandle();
    await this.rpc.request(
      "addGraphRecords",
      { handle, records: batch },
      graphRecordTransferables(batch)
    );
  }

  public build(): Promise<GraphRetrievalDatabase> {
    return this.finish(
      (handle) => new GraphRetrievalDatabase(this.rpc, handle)
    );
  }
}

abstract class DatabaseLifecycle {
  protected readonly rpc: WorkerRpcClient;
  protected readonly handle: WasmHandle;
  #closed = false;
  #closing?: Promise<void>;

  protected constructor(rpc: WorkerRpcClient, handle: WasmHandle) {
    this.rpc = rpc;
    this.handle = handle;
  }

  public get closed(): boolean {
    return this.#closed || this.rpc.closed;
  }

  public requireOpen(): void {
    if (this.#closed) lifecycle(`${this.constructor.name} is closed.`);
    this.rpc.assertOpen();
  }

  public close(): Promise<void> {
    if (this.#closing !== undefined) return this.#closing;
    this.#closed = true;
    this.#closing = this.rpc.request("closeHandle", { handle: this.handle });
    return this.#closing;
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

export class RetrievalDatabase extends DatabaseLifecycle {
  public constructor(rpc: WorkerRpcClient, handle: WasmHandle) {
    super(rpc, handle);
  }

  public async search(
    query: SearchQuery,
    control: SearchControl = {}
  ): Promise<readonly SearchResult[]> {
    this.requireOpen();
    rejectScopedQuery(query);
    return search(this.rpc, this.handle, query, control);
  }
}

interface SelectionOwner {
  readonly rpc: WorkerRpcClient;
  readonly handle: WasmHandle;
}
const selectionOwners = new WeakMap<GraphSelection, SelectionOwner>();

export class GraphSelection implements GraphSelectionReference {
  readonly #rpc: WorkerRpcClient;
  readonly #handle: WasmHandle;
  readonly #data: GraphSelectionData;
  #closed = false;
  #closing?: Promise<void>;

  public constructor(
    rpc: WorkerRpcClient,
    handle: WasmHandle,
    data: GraphSelectionData
  ) {
    this.#rpc = rpc;
    this.#handle = handle;
    this.#data = data;
    selectionOwners.set(this, { rpc, handle });
  }

  public get matches(): GraphSelectionData["matches"] {
    return this.#data.matches;
  }

  public get truncated(): GraphSelectionData["truncated"] {
    return this.#data.truncated;
  }

  public get trace(): GraphSelectionData["trace"] {
    return this.#data.trace;
  }

  public get closed(): boolean {
    return this.#closed || this.#rpc.closed;
  }

  public requireOpen(): void {
    if (this.#closed) lifecycle("GraphSelection is closed.");
    this.#rpc.assertOpen();
  }

  public close(): Promise<void> {
    if (this.#closing !== undefined) return this.#closing;
    this.#closed = true;
    this.#closing = this.#rpc.request("closeHandle", { handle: this.#handle });
    return this.#closing;
  }

  public [Symbol.dispose](): void {
    void this.close();
  }

  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

export interface GraphQueryOperations {
  query(query: GraphQuery, control?: SearchControl): Promise<GraphSelection>;
  projectCandidates(
    selection: GraphSelection,
    options?: { readonly where?: Filter; readonly signal?: AbortSignal }
  ): Promise<CandidateProjection>;
}

export interface RetrievalOperations {
  search(
    query: SearchQuery,
    control?: SearchControl
  ): Promise<readonly SearchResult[]>;
}

class GraphQueryView implements GraphQueryOperations {
  public constructor(
    private readonly owner: DatabaseLifecycle,
    private readonly rpc: WorkerRpcClient,
    private readonly handle: WasmHandle
  ) {}

  public async query(
    query: GraphQuery,
    control: SearchControl = {}
  ): Promise<GraphSelection> {
    this.owner.requireOpen();
    const selection = await this.rpc.request<WasmGraphSelection>(
      "graphQuery",
      { handle: this.handle, query },
      [],
      control
    );
    return new GraphSelection(this.rpc, selection.handle, selection.data);
  }

  public async projectCandidates(
    selection: GraphSelection,
    options: { readonly where?: Filter; readonly signal?: AbortSignal } = {}
  ): Promise<CandidateProjection> {
    this.owner.requireOpen();
    const selectionHandle = selectionHandleFor(selection, this.rpc);
    return this.rpc.request<CandidateProjection>(
      "projectCandidates",
      {
        databaseHandle: this.handle,
        selectionHandle,
        ...(options.where === undefined ? {} : { where: options.where })
      },
      [],
      options.signal === undefined ? {} : { signal: options.signal }
    );
  }
}

class RetrievalQueryView implements RetrievalOperations {
  public constructor(
    private readonly owner: DatabaseLifecycle,
    private readonly rpc: WorkerRpcClient,
    private readonly handle: WasmHandle
  ) {}

  public search(
    query: SearchQuery,
    control: SearchControl = {}
  ): Promise<readonly SearchResult[]> {
    this.owner.requireOpen();
    return search(this.rpc, this.handle, query, control);
  }
}

export class GraphDatabase extends DatabaseLifecycle {
  public readonly graph: GraphQueryOperations;

  public constructor(rpc: WorkerRpcClient, handle: WasmHandle) {
    super(rpc, handle);
    this.graph = new GraphQueryView(this, rpc, handle);
  }
}

export class GraphRetrievalDatabase extends DatabaseLifecycle {
  public readonly graph: GraphQueryOperations;
  public readonly retrieval: RetrievalOperations;

  public constructor(rpc: WorkerRpcClient, handle: WasmHandle) {
    super(rpc, handle);
    this.graph = new GraphQueryView(this, rpc, handle);
    this.retrieval = new RetrievalQueryView(this, rpc, handle);
  }
}

async function search(
  rpc: WorkerRpcClient,
  handle: WasmHandle,
  query: SearchQuery,
  control: SearchControl
): Promise<readonly SearchResult[]> {
  const limit = positiveInteger(query.limit ?? 10, "limit");
  let embedding: Float32Array | undefined;
  if ("embedding" in query && query.embedding !== undefined) {
    embedding = new Float32Array(query.embedding);
  }
  const within =
    query.within === undefined
      ? undefined
      : selectionHandleFor(query.within, rpc);
  const workerQuery: WasmSearchQuery = {
    mode: query.mode,
    limit,
    ...("text" in query ? { text: query.text } : {}),
    ...(embedding === undefined ? {} : { embedding }),
    ...("alpha" in query && query.alpha !== undefined
      ? { alpha: unitInterval(query.alpha, "alpha") }
      : {}),
    ...(query.where === undefined ? {} : { where: query.where }),
    ...("vectorCandidates" in query && query.vectorCandidates !== undefined
      ? {
          vectorCandidates: positiveInteger(
            query.vectorCandidates,
            "vectorCandidates"
          )
        }
      : {}),
    ...("keywordCandidates" in query && query.keywordCandidates !== undefined
      ? {
          keywordCandidates: positiveInteger(
            query.keywordCandidates,
            "keywordCandidates"
          )
        }
      : {}),
    ...(within === undefined ? {} : { within })
  };
  return rpc.request<readonly SearchResult[]>(
    "search",
    { handle, query: workerQuery },
    embedding === undefined ? [] : [embedding.buffer],
    control
  );
}

function selectionHandleFor(
  selection: GraphSelectionReference,
  rpc: WorkerRpcClient
): WasmHandle {
  if (!(selection instanceof GraphSelection)) {
    lifecycle("Graph selection was not created by this browser package.");
  }
  const owner = selectionOwners.get(selection);
  if (owner === undefined || owner.rpc !== rpc) {
    lifecycle("Graph selection belongs to a different RetrievalKit client.");
  }
  selection.requireOpen();
  return owner.handle;
}

function rejectScopedQuery(query: SearchQuery): void {
  if (query.within !== undefined) {
    throw new RetrievalKitInputError(
      "Scoped retrieval requires GraphRetrievalDatabase.",
      "RK_INVALID_INPUT"
    );
  }
}

function positiveInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RetrievalKitInputError(
      `${name} must be a positive integer.`,
      "RK_INVALID_INPUT"
    );
  }
  return value;
}

function unitInterval(value: number, name: string): number {
  if (!Number.isFinite(value) || value < 0 || value > 1) {
    throw new RetrievalKitInputError(
      `${name} must be between 0 and 1.`,
      "RK_INVALID_INPUT"
    );
  }
  return value;
}

function lifecycle(message: string): never {
  throw new RetrievalKitLifecycleError(message, "RK_LIFECYCLE");
}
