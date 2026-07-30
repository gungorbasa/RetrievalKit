import type {
  CandidateProjection,
  Filter,
  GraphBuilderOptions,
  GraphQuery,
  GraphRetrievalBuilderOptions,
  GraphRetrievalRecordInput,
  GraphSelectionData,
  Metadata,
  RecordValue,
  RetrievalBuilderOptions,
  SearchResult
} from "./types.js";

export type DatabaseKind = "retrieval" | "graph" | "graphRetrieval";
export type WasmHandle = string;

export interface BrowserCapabilities {
  readonly execution: "dedicated-worker";
  readonly performanceTier: "portable" | "simd128";
  readonly persistence: false;
  readonly threads: false;
  readonly simd: boolean;
  readonly structuredDtos: true;
  readonly bulkFloat32Embeddings: true;
}

/**
 * Contiguous representation used at the JS/WASM boundary. Every document has
 * the same dimension and occupies one row in `embeddings`.
 */
export interface WasmDocumentBatch {
  readonly ids: readonly string[];
  readonly texts: readonly string[];
  readonly metadata: readonly (Metadata | undefined)[];
  readonly dimension: number;
  readonly embeddings: Float32Array;
}

export interface WasmGraphRecord {
  readonly id: string;
  readonly type: string;
  readonly fields?: Readonly<Record<string, RecordValue>>;
  readonly content?: string;
  readonly metadata?: Metadata;
  readonly retrieval?:
    | {
        readonly kind: "content";
        readonly embeddingOffset: number;
        readonly dimension: number;
      }
    | {
        readonly kind: "documents";
        readonly documents: {
          readonly ids: readonly string[];
          readonly texts: readonly string[];
          readonly metadata: readonly (Metadata | undefined)[];
          readonly embeddingOffset: number;
          readonly dimension: number;
        };
      };
}

export interface WasmGraphRecordBatch {
  readonly records: readonly WasmGraphRecord[];
  readonly dimension: number;
  /** The sole transferable embedding buffer for this operation. */
  readonly embeddings: Float32Array;
}

export interface WasmSearchQuery {
  readonly mode: "vector" | "text" | "hybrid";
  readonly text?: string;
  readonly embedding?: Float32Array;
  readonly alpha?: number;
  readonly limit: number;
  readonly where?: Filter;
  readonly vectorCandidates?: number;
  readonly keywordCandidates?: number;
  readonly within?: WasmHandle;
}

export interface WasmGraphSelection {
  readonly handle: WasmHandle;
  readonly data: GraphSelectionData;
}

export type WasmDatabaseOptions =
  | { readonly kind: "retrieval"; readonly options: RetrievalBuilderOptions }
  | { readonly kind: "graph"; readonly options: GraphBuilderOptions }
  | {
      readonly kind: "graphRetrieval";
      readonly options: GraphRetrievalBuilderOptions;
    };

/**
 * Narrow contract implemented by generated wasm-bindgen glue. It is injected
 * into the Worker so this package never imports Node/N-API code and can be
 * tested before generated WASM artifacts exist.
 *
 * Implementations must perform ranking, filtering, traversal, generation
 * validation, and persistence in WASM. `AbortSignal` is cooperative: a
 * synchronous WASM call cannot be preempted, but adapters should check it
 * between phases and before committing results.
 */
export interface RetrievalKitWasmAdapter {
  initialize(signal: AbortSignal): Promise<BrowserCapabilities>;
  createDatabase(
    request: WasmDatabaseOptions,
    signal: AbortSignal
  ): Promise<WasmHandle>;
  addDocuments(
    handle: WasmHandle,
    documents: WasmDocumentBatch,
    signal: AbortSignal
  ): Promise<void>;
  addGraphRecords(
    handle: WasmHandle,
    records: WasmGraphRecordBatch,
    signal: AbortSignal
  ): Promise<void>;
  build(handle: WasmHandle, signal: AbortSignal): Promise<void>;
  search(
    handle: WasmHandle,
    query: WasmSearchQuery,
    signal: AbortSignal
  ): Promise<readonly SearchResult[]>;
  graphQuery(
    handle: WasmHandle,
    query: GraphQuery,
    signal: AbortSignal
  ): Promise<WasmGraphSelection>;
  projectCandidates(
    databaseHandle: WasmHandle,
    selectionHandle: WasmHandle,
    where: Filter | undefined,
    signal: AbortSignal
  ): Promise<CandidateProjection>;
  close(handle: WasmHandle): Promise<void>;
}

export function toWasmGraphRecords(
  records: readonly GraphRetrievalRecordInput[]
): WasmGraphRecordBatch {
  const embeddingValues: number[] = [];
  let dimension = 0;
  const requireDimension = (value: number): void => {
    if (value === 0) throw new TypeError("Embeddings must have a positive dimension.");
    if (dimension === 0) dimension = value;
    if (value !== dimension) {
      throw new TypeError(
        `Embedding dimension mismatch: expected ${dimension}, received ${value}.`
      );
    }
  };
  const converted = records.map((record) => {
    const base = {
      id: record.id,
      type: record.type,
      ...(record.fields === undefined ? {} : { fields: record.fields }),
      ...(record.content === undefined ? {} : { content: record.content }),
      ...(record.metadata === undefined ? {} : { metadata: record.metadata })
    };
    if (record.retrieval?.kind === "content") {
      requireDimension(record.retrieval.embedding.length);
      const embeddingOffset = embeddingValues.length;
      embeddingValues.push(...record.retrieval.embedding);
      return {
        ...base,
        retrieval: {
          kind: "content" as const,
          embeddingOffset,
          dimension: record.retrieval.embedding.length
        }
      };
    }
    if (record.retrieval?.kind === "documents") {
      const documents = record.retrieval.documents;
      const batch = toWasmDocumentBatch(documents);
      if (batch.dimension !== 0) requireDimension(batch.dimension);
      const embeddingOffset = embeddingValues.length;
      embeddingValues.push(...batch.embeddings);
      return {
        ...base,
        retrieval: {
          kind: "documents" as const,
          documents: {
            ids: batch.ids,
            texts: batch.texts,
            metadata: batch.metadata,
            embeddingOffset,
            dimension: batch.dimension
          }
        }
      };
    }
    return base;
  });
  return {
    records: converted,
    dimension,
    embeddings: new Float32Array(embeddingValues)
  };
}

export function toWasmDocumentBatch(
  documents: readonly {
    readonly id: string;
    readonly text: string;
    readonly metadata?: Metadata;
    readonly embedding: Float32Array;
  }[]
): WasmDocumentBatch {
  if (documents.length === 0) {
    return { ids: [], texts: [], metadata: [], dimension: 0, embeddings: new Float32Array() };
  }
  const first = documents[0];
  if (first === undefined || first.embedding.length === 0) {
    throw new TypeError("Embeddings must have a positive dimension.");
  }
  const dimension = first.embedding.length;
  const embeddings = new Float32Array(documents.length * dimension);
  documents.forEach((document, index) => {
    if (document.embedding.length !== dimension) {
      throw new TypeError(
        `Embedding dimension mismatch at document ${index}: expected ${dimension}, received ${document.embedding.length}.`
      );
    }
    embeddings.set(document.embedding, index * dimension);
  });
  return {
    ids: documents.map(({ id }) => id),
    texts: documents.map(({ text }) => text),
    metadata: documents.map(({ metadata }) => metadata),
    dimension,
    embeddings
  };
}

export function graphRecordTransferables(
  records: WasmGraphRecordBatch
): Transferable[] {
  return records.embeddings.byteLength === 0 ? [] : [records.embeddings.buffer];
}

export {
  browserSupportsWasmSimd128,
  createAdaptiveGeneratedWasmAdapter,
  createGeneratedWasmAdapter,
  type GeneratedGraphDatabase,
  type GeneratedGraphRetrievalDatabase,
  type GeneratedRetrievalDatabase,
  type GeneratedWasmTierLoaders,
  type GeneratedWasmModule
} from "./generated-adapter.js";
