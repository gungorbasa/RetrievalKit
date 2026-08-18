import type {
  BrowserCapabilities,
  RetrievalKitWasmAdapter,
  WasmDatabaseOptions,
  WasmDocumentBatch,
  WasmGraphRecordBatch,
  WasmGraphSelection,
  WasmHandle,
  WasmSearchQuery
} from "./adapter.js";
import {
  RetrievalKitDimensionError,
  RetrievalKitError,
  RetrievalKitGraphError,
  RetrievalKitInputError,
  RetrievalKitLifecycleError,
  RetrievalKitQueryError
} from "./errors.js";
import type {
  CandidateProjection,
  Filter,
  GraphNodeId,
  GraphQuery,
  GraphScalar,
  GraphSchema,
  GraphSelectionData,
  GraphTruncationReason,
  Metadata,
  MetadataValue,
  RecordValue,
  SearchResult
} from "./types.js";

export interface GeneratedRetrievalDatabase {
  addRecordsBatch(records: unknown, embeddings: Float32Array, dimension: number): number;
  build(): void;
  vectorSearch(embedding: Float32Array, options: unknown): unknown;
  bm25Search(text: string, options: unknown): unknown;
  hybridSearch(embedding: Float32Array, options: unknown): unknown;
  close(): void;
}

export interface GeneratedGraphDatabase {
  addRecordsBatch(records: unknown): number;
  build(): void;
  query(query: unknown): unknown;
  projectCandidates(selectionId: number, filter: unknown): unknown;
  releaseSelection(selectionId: number): boolean;
  close(): void;
}

export interface GeneratedGraphRetrievalDatabase {
  addRecordsBatch(records: unknown, embeddings: Float32Array, dimension: number): number;
  build(): void;
  graphQuery(query: unknown): unknown;
  projectCandidates(selectionId: number, filter: unknown): unknown;
  vectorSearch(
    embedding: Float32Array,
    options: unknown,
    selectionId?: number
  ): unknown;
  bm25Search(text: string, options: unknown, selectionId?: number): unknown;
  hybridSearch(
    embedding: Float32Array,
    options: unknown,
    selectionId?: number
  ): unknown;
  releaseSelection(selectionId: number): boolean;
  close(): void;
}

export interface GeneratedWasmModule {
  readonly buildCapabilities: () => unknown;
  readonly RetrievalDatabase: new (
    corpusId: string,
    metric: string,
    encoding: string,
    bm25K1: number,
    bm25B: number,
    stopWords: readonly string[]
  ) => GeneratedRetrievalDatabase;
  readonly GraphDatabase: new (
    corpusId: string,
    schema: unknown
  ) => GeneratedGraphDatabase;
  readonly GraphRetrievalDatabase: new (
    corpusId: string,
    schema: unknown,
    metric: string,
    encoding: string,
    bm25K1: number,
    bm25B: number,
    stopWords: readonly string[]
  ) => GeneratedGraphRetrievalDatabase;
}

export interface GeneratedWasmTierLoaders {
  /**
   * Loads and initializes the portable wasm-bindgen module.
   */
  readonly portable: () => GeneratedWasmModule | Promise<GeneratedWasmModule>;
  /**
   * Loads and initializes the separate SIMD128 wasm-bindgen module.
   */
  readonly simd128?: () => GeneratedWasmModule | Promise<GeneratedWasmModule>;
  /**
   * Test seam for deterministic tier-selection coverage.
   */
  readonly supportsSimd128?: () => boolean;
}

type GeneratedDatabase =
  | { readonly kind: "retrieval"; readonly value: GeneratedRetrievalDatabase }
  | { readonly kind: "graph"; readonly value: GeneratedGraphDatabase }
  | {
      readonly kind: "graphRetrieval";
      readonly value: GeneratedGraphRetrievalDatabase;
    };

interface OwnedSelection {
  readonly databaseHandle: WasmHandle;
  readonly selectionId: number;
}

/**
 * Creates the production adapter around an injected wasm-bindgen module.
 * Passing a loader keeps generated filenames and initialization strategy in
 * the application-owned Worker entry.
 */
export function createGeneratedWasmAdapter(
  module:
    | GeneratedWasmModule
    | (() => GeneratedWasmModule | Promise<GeneratedWasmModule>)
): RetrievalKitWasmAdapter {
  return new GeneratedWasmAdapter(module);
}

/**
 * Selects exactly one generated artifact before the first database is created.
 * SIMD128 is preferred when both the artifact and browser capability exist;
 * otherwise the portable artifact is used.
 */
export function createAdaptiveGeneratedWasmAdapter(
  loaders: GeneratedWasmTierLoaders
): RetrievalKitWasmAdapter {
  return createGeneratedWasmAdapter(async () => {
    const supportsSimd =
      loaders.supportsSimd128?.() ?? browserSupportsWasmSimd128();
    if (supportsSimd && loaders.simd128 !== undefined) {
      return loaders.simd128();
    }
    return loaders.portable();
  });
}

/**
 * Validates the smallest WebAssembly module that returns a v128 constant.
 * Unsupported engines reject it without downloading or instantiating the
 * RetrievalKit SIMD artifact.
 */
export function browserSupportsWasmSimd128(): boolean {
  if (typeof WebAssembly === "undefined") return false;
  return WebAssembly.validate(
    new Uint8Array([
      0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01,
      0x60, 0x00, 0x01, 0x7b, 0x03, 0x02, 0x01, 0x00, 0x0a, 0x16, 0x01,
      0x14, 0x00, 0xfd, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b
    ])
  );
}

class GeneratedWasmAdapter implements RetrievalKitWasmAdapter {
  readonly #load:
    | GeneratedWasmModule
    | (() => GeneratedWasmModule | Promise<GeneratedWasmModule>);
  readonly #databases = new Map<WasmHandle, GeneratedDatabase>();
  readonly #selections = new Map<WasmHandle, OwnedSelection>();
  #module?: GeneratedWasmModule;
  #nextDatabase = 1;
  #nextSelection = 1;

  public constructor(
    load:
      | GeneratedWasmModule
      | (() => GeneratedWasmModule | Promise<GeneratedWasmModule>)
  ) {
    this.#load = load;
  }

  public async initialize(signal: AbortSignal): Promise<BrowserCapabilities> {
    requireNotAborted(signal);
    this.#module =
      typeof this.#load === "function" ? await this.#load() : this.#load;
    requireNotAborted(signal);
    const generatedModule = this.#module;
    return capabilityDto(
      generatedCall(() => generatedModule.buildCapabilities())
    );
  }

  public async createDatabase(
    request: WasmDatabaseOptions,
    signal: AbortSignal
  ): Promise<WasmHandle> {
    requireNotAborted(signal);
    const module = this.#requireModule();
    const database = generatedCall<GeneratedDatabase>(() => {
      switch (request.kind) {
        case "retrieval":
          return {
            kind: "retrieval",
            value: new module.RetrievalDatabase(
              request.options.corpusId,
              request.options.metric ?? "cosine",
              request.options.encoding ?? "i8",
              request.options.bm25?.k1 ?? 1.2,
              request.options.bm25?.b ?? 0.75,
              request.options.bm25?.stopWords ?? []
            )
          };
        case "graph":
          return {
            kind: "graph",
            value: new module.GraphDatabase(
              request.options.corpusId,
              graphSchemaDto(request.options.schema)
            )
          };
        case "graphRetrieval":
          return {
            kind: "graphRetrieval",
            value: new module.GraphRetrievalDatabase(
              request.options.corpusId,
              graphSchemaDto(request.options.schema),
              request.options.metric ?? "cosine",
              request.options.encoding ?? "i8",
              request.options.bm25?.k1 ?? 1.2,
              request.options.bm25?.b ?? 0.75,
              request.options.bm25?.stopWords ?? []
            )
          };
      }
    });
    const handle = `database-${this.#nextDatabase++}`;
    this.#databases.set(handle, database);
    return handle;
  }

  public async addDocuments(
    handle: WasmHandle,
    documents: WasmDocumentBatch,
    signal: AbortSignal
  ): Promise<void> {
    requireNotAborted(signal);
    const database = this.#database(handle, "retrieval");
    generatedCall(() =>
      database.value.addRecordsBatch(
        retrievalRecordDtos(documents),
        documents.embeddings,
        documents.dimension
      )
    );
  }

  public async addGraphRecords(
    handle: WasmHandle,
    batch: WasmGraphRecordBatch,
    signal: AbortSignal
  ): Promise<void> {
    requireNotAborted(signal);
    const database = this.#database(handle);
    if (database.kind === "retrieval") {
      lifecycle("Cannot add graph records to a retrieval-only database.");
    }
    const records = graphRecordDtos(batch, database.kind);
    if (database.kind === "graph") {
      generatedCall(() => database.value.addRecordsBatch(records));
      return;
    }
    generatedCall(() =>
      database.value.addRecordsBatch(records, batch.embeddings, batch.dimension)
    );
  }

  public async build(handle: WasmHandle, signal: AbortSignal): Promise<void> {
    requireNotAborted(signal);
    generatedCall(() => this.#database(handle).value.build());
  }

  public async search(
    handle: WasmHandle,
    query: WasmSearchQuery,
    signal: AbortSignal
  ): Promise<readonly SearchResult[]> {
    requireNotAborted(signal);
    const database = this.#database(handle);
    if (database.kind === "graph") {
      lifecycle("GraphDatabase does not support retrieval.");
    }
    const selectionId =
      query.within === undefined
        ? undefined
        : this.#selection(query.within, handle).selectionId;
    const options = searchOptionsDto(query);
    let raw: unknown;
    if (query.mode === "vector") {
      if (query.embedding === undefined) input("Vector search requires an embedding.");
      const embedding = query.embedding;
      raw = generatedCall(() =>
        database.kind === "retrieval"
          ? database.value.vectorSearch(embedding, options)
          : database.value.vectorSearch(embedding, options, selectionId)
      );
      return vectorResults(raw);
    }
    if (
      query.mode === "hybrid" &&
      query.embedding === undefined &&
      (query.alpha ?? 0.6) !== 0
    ) {
      input("Hybrid search requires an embedding unless alpha is 0.");
    }
    if (query.mode === "text" || query.embedding === undefined) {
      if (query.text === undefined) input("Text search requires text.");
      const text = query.text;
      raw = generatedCall(() =>
        database.kind === "retrieval"
          ? database.value.bm25Search(text, options)
          : database.value.bm25Search(text, options, selectionId)
      );
      return keywordResults(raw);
    }
    const hybridOptions = {
      ...options,
      text: query.text ?? "",
      alpha: query.alpha ?? 0.6,
      ...(query.vectorCandidates === undefined
        ? {}
        : { vectorCandidates: query.vectorCandidates }),
      ...(query.keywordCandidates === undefined
        ? {}
        : { keywordCandidates: query.keywordCandidates })
    };
    const embedding = query.embedding;
    raw = generatedCall(() =>
      database.kind === "retrieval"
        ? database.value.hybridSearch(embedding, hybridOptions)
        : database.value.hybridSearch(
            embedding,
            hybridOptions,
            selectionId
          )
    );
    return hybridResults(raw, query.alpha ?? 0.6);
  }

  public async graphQuery(
    handle: WasmHandle,
    query: GraphQuery,
    signal: AbortSignal
  ): Promise<WasmGraphSelection> {
    requireNotAborted(signal);
    const database = this.#database(handle);
    if (database.kind === "retrieval") {
      lifecycle("RetrievalDatabase does not support graph queries.");
    }
    const raw = generatedCall(() =>
      database.kind === "graph"
        ? database.value.query(graphQueryDto(query))
        : database.value.graphQuery(graphQueryDto(query))
    );
    const result = graphResultDto(raw);
    const selectionHandle = `selection-${this.#nextSelection++}`;
    this.#selections.set(selectionHandle, {
      databaseHandle: handle,
      selectionId: result.selectionId
    });
    return { handle: selectionHandle, data: result.data };
  }

  public async projectCandidates(
    databaseHandle: WasmHandle,
    selectionHandle: WasmHandle,
    where: Filter | undefined,
    signal: AbortSignal
  ): Promise<CandidateProjection> {
    requireNotAborted(signal);
    const database = this.#database(databaseHandle);
    if (database.kind === "retrieval") {
      lifecycle("RetrievalDatabase does not support candidate projection.");
    }
    const selection = this.#selection(selectionHandle, databaseHandle);
    const raw = generatedCall(() =>
      database.value.projectCandidates(
        selection.selectionId,
        where === undefined ? undefined : filterDto(where)
      )
    );
    return candidateProjectionDto(raw);
  }

  public async close(handle: WasmHandle): Promise<void> {
    const selection = this.#selections.get(handle);
    if (selection !== undefined) {
      const database = this.#database(selection.databaseHandle);
      if (database.kind === "retrieval") {
        lifecycle("Selection owner is not graph-capable.");
      }
      generatedCall(() => database.value.releaseSelection(selection.selectionId));
      this.#selections.delete(handle);
      return;
    }
    const database = this.#databases.get(handle);
    if (database === undefined) lifecycle(`Unknown or closed WASM handle '${handle}'.`);
    generatedCall(() => database.value.close());
    this.#databases.delete(handle);
    for (const [selectionHandle, owner] of this.#selections) {
      if (owner.databaseHandle === handle) this.#selections.delete(selectionHandle);
    }
  }

  #requireModule(): GeneratedWasmModule {
    if (this.#module === undefined) {
      lifecycle("Generated WASM module has not been initialized.");
    }
    return this.#module;
  }

  #database(handle: WasmHandle): GeneratedDatabase;
  #database(
    handle: WasmHandle,
    kind: "retrieval"
  ): Extract<GeneratedDatabase, { readonly kind: "retrieval" }>;
  #database(
    handle: WasmHandle,
    kind?: "retrieval"
  ): GeneratedDatabase {
    const database = this.#databases.get(handle);
    if (database === undefined) lifecycle(`Unknown or closed database handle '${handle}'.`);
    if (kind !== undefined && database.kind !== kind) {
      lifecycle(`Database handle '${handle}' has the wrong capability.`);
    }
    return database;
  }

  #selection(handle: WasmHandle, databaseHandle: WasmHandle): OwnedSelection {
    const selection = this.#selections.get(handle);
    if (selection === undefined) lifecycle(`Unknown or released selection '${handle}'.`);
    if (selection.databaseHandle !== databaseHandle) {
      lifecycle("Graph selection belongs to a different database.");
    }
    return selection;
  }
}

function capabilityDto(value: unknown): BrowserCapabilities {
  const dto = object(value, "build capabilities");
  const performanceTier =
    dto.performanceTier === "portable" || dto.performanceTier === "simd128"
      ? dto.performanceTier
      : undefined;
  if (
    dto.execution !== "dedicated-worker" ||
    performanceTier === undefined ||
    dto.persistence !== false ||
    dto.threads !== false ||
    typeof dto.simd !== "boolean" ||
    dto.simd !== (performanceTier === "simd128") ||
    dto.structuredDtos !== true ||
    dto.bulkFloat32Embeddings !== true
  ) {
    input("Generated WASM module returned unsupported capabilities.");
  }
  return {
    execution: "dedicated-worker",
    performanceTier,
    persistence: false,
    threads: false,
    simd: dto.simd,
    structuredDtos: true,
    bulkFloat32Embeddings: true
  };
}

function retrievalRecordDtos(batch: WasmDocumentBatch): unknown[] {
  return batch.ids.map((id, index) => ({
    id,
    recordType: "Document",
    fields: [],
    content: batch.texts[index],
    metadata: [],
    chunks: [
      {
        key: id,
        text: batch.texts[index],
        metadata: metadataDto(batch.metadata[index]),
        embeddingIndex: index
      }
    ]
  }));
}

function graphRecordDtos(
  batch: WasmGraphRecordBatch,
  databaseKind: "graph" | "graphRetrieval"
): unknown[] {
  return batch.records.map((record) => {
    const chunks: unknown[] = [];
    if (record.retrieval?.kind === "content") {
      chunks.push({
        key: record.id,
        text: record.content ?? "",
        metadata: [],
        embeddingIndex:
          batch.dimension === 0
            ? undefined
            : record.retrieval.embeddingOffset / batch.dimension
      });
    } else if (
      databaseKind === "graph" &&
      record.content !== undefined
    ) {
      chunks.push({
        key: record.id,
        text: record.content,
        metadata: []
      });
    } else if (record.retrieval?.kind === "documents") {
      record.retrieval.documents.ids.forEach((id, index) => {
        chunks.push({
          key: id,
          text: record.retrieval?.kind === "documents"
            ? record.retrieval.documents.texts[index]
            : "",
          metadata:
            record.retrieval?.kind === "documents"
              ? metadataDto(record.retrieval.documents.metadata[index])
              : [],
          embeddingIndex:
            batch.dimension === 0
              ? undefined
              : record.retrieval?.kind === "documents"
                ? record.retrieval.documents.embeddingOffset / batch.dimension + index
                : undefined
        });
      });
    }
    return {
      id: record.id,
      recordType: record.type,
      fields: Object.entries(record.fields ?? {}).map(([field, value]) => ({
        field,
        value: recordValueDto(value)
      })),
      ...(record.content === undefined ? {} : { content: record.content }),
      metadata: metadataDto(record.metadata),
      chunks
    };
  });
}

function graphSchemaDto(schema: GraphSchema): unknown {
  return {
    recordNodes: schema.recordNodes.map((node) => ({
      recordType: node.recordType,
      nodeType: node.nodeType,
      queryableFields: node.queryableFields?.map((path) => [...path]) ?? []
    })),
    relationships:
      schema.relationships?.map((relationship) => ({
        relationshipType: relationship.relationship,
        sourceNodeType: relationship.sourceNodeType,
        targetNodeType: relationship.targetNodeType,
        sourceField: [...relationship.sourceField],
        cardinality: relationship.cardinality,
        missingTarget: relationship.missingTarget ?? "error",
        duplicateReferences: relationship.duplicateReferences ?? "error",
        allowSelfEdge: relationship.allowSelfEdge ?? false,
        ...(relationship.inverseRelationship === undefined
          ? {}
          : { inverseRelationship: relationship.inverseRelationship })
      })) ?? [],
    ...(schema.chunkNodes === undefined
      ? {}
      : {
          chunkNodes: {
            nodeType: schema.chunkNodes.nodeType,
            ownsRelationship: schema.chunkNodes.ownsRelationship,
            ...(schema.chunkNodes.inverseRelationship === undefined
              ? {}
              : { inverseRelationship: schema.chunkNodes.inverseRelationship })
          }
        })
  };
}

function graphQueryDto(query: GraphQuery): unknown {
  return {
    seed:
      query.seed.kind === "nodes"
        ? {
            kind: "nodeIds",
            nodes: query.seed.nodes.map(nodeDto)
          }
        : {
            kind: "equals",
            nodeType: query.seed.nodeType,
            field: [...query.seed.field],
            values: query.seed.values.map(graphScalarDto)
          },
    steps:
      query.traverse?.map((step) => ({
        relationship: step.relationship,
        direction: step.direction ?? "outgoing",
        minHops: step.minHops ?? 1,
        maxHops: step.maxHops ?? 1
      })) ?? [],
    ...(query.limits === undefined
      ? {}
      : {
          limits: {
            maxHops: query.limits.maxHops ?? 8,
            maxVisited: query.limits.maxVisited ?? 100_000,
            maxResults: query.limits.maxResults ?? 10_000,
            maxWorkingBytes:
              query.limits.maxWorkingBytes ?? 64 * 1024 * 1024
          }
        })
  };
}

function nodeDto(node: GraphNodeId): unknown {
  return {
    nodeType: node.nodeType,
    sourceKind: node.kind,
    recordId: node.recordId,
    ...(node.kind === "chunk" ? { chunkKey: node.chunkKey } : {})
  };
}

function graphScalarDto(value: GraphScalar): unknown {
  if (typeof value === "string") return { kind: "string", value };
  if (typeof value === "boolean") return { kind: "boolean", value };
  return { kind: "integer", value: value.toString() };
}

function recordValueDto(value: RecordValue): unknown {
  if (value === null) return { kind: "null" };
  if (typeof value === "string") return { kind: "string", value };
  if (typeof value === "boolean") return { kind: "boolean", value };
  if (typeof value === "bigint") return { kind: "integer", value: value.toString() };
  if (typeof value === "number") {
    return Number.isSafeInteger(value)
      ? { kind: "integer", value: value.toString() }
      : { kind: "float", value };
  }
  if (Array.isArray(value)) {
    return { kind: "list", value: value.map(recordValueDto) };
  }
  return {
    kind: "map",
    value: Object.entries(value).map(([field, child]) => ({
      field,
      value: recordValueDto(child)
    }))
  };
}

function metadataDto(metadata: Metadata | undefined): unknown[] {
  return Object.entries(metadata ?? {}).map(([field, value]) => ({
    field,
    value: metadataValueDto(value)
  }));
}

function metadataValueDto(value: MetadataValue): unknown {
  if (typeof value === "string") return { kind: "string", value };
  if (typeof value === "boolean") return { kind: "boolean", value };
  if (typeof value === "bigint") return { kind: "integer", value: value.toString() };
  if (typeof value === "number") {
    return Number.isSafeInteger(value)
      ? { kind: "integer", value: value.toString() }
      : { kind: "float", value };
  }
  if (value.kind === "timestampMillis") {
    return { kind: "timestamp", value: value.value.toString() };
  }
  return { kind: "float", value: value.value };
}

function filterDto(filter: Filter): unknown {
  switch (filter.kind) {
    case "equals":
    case "notEquals":
      return {
        kind: filter.kind,
        field: filter.field,
        value: metadataValueDto(filter.value)
      };
    case "in":
      return {
        kind: "in",
        field: filter.field,
        values: filter.values.map(metadataValueDto)
      };
    case "range":
      return {
        kind: "range",
        field: filter.field,
        ...(filter.lower === undefined
          ? {}
          : { lower: metadataValueDto(filter.lower) }),
        ...(filter.upper === undefined
          ? {}
          : { upper: metadataValueDto(filter.upper) })
      };
    case "exists":
      return { kind: "exists", field: filter.field };
    case "all":
    case "any":
      return { kind: filter.kind, children: filter.filters.map(filterDto) };
  }
}

function searchOptionsDto(query: WasmSearchQuery): Record<string, unknown> {
  return {
    topK: query.limit,
    ...(query.where === undefined ? {} : { filter: filterDto(query.where) })
  };
}

function vectorResults(value: unknown): readonly SearchResult[] {
  return array(value, "vector results").map((item) => {
    const hit = object(item, "vector result");
    const vectorScore = number(hit.vectorScore, "vectorScore");
    return {
      documentId: string(hit.documentId, "documentId"),
      text: string(hit.text, "text"),
      metadata: metadataFromDto(hit.metadata),
      score: number(hit.score, "score"),
      vectorScore,
      trace: { kind: "vector", vectorScore }
    };
  });
}

function keywordResults(value: unknown): readonly SearchResult[] {
  return array(value, "keyword results").map((item) => {
    const hit = object(item, "keyword result");
    return {
      documentId: string(hit.documentId, "documentId"),
      text: string(hit.text, "text"),
      metadata: metadataFromDto(hit.metadata),
      score: number(hit.score, "score"),
      keywordScore: number(hit.score, "score"),
      trace: {
        kind: "keyword",
        matchedTerms: stringArray(hit.matchedTerms, "matchedTerms")
      }
    };
  });
}

function hybridResults(value: unknown, alpha: number): readonly SearchResult[] {
  return array(value, "hybrid results").map((item) => {
    const hit = object(item, "hybrid result");
    const vectorScore = optionalNumber(hit.vectorScore);
    const keywordScore = optionalNumber(hit.keywordScore);
    const vectorRank = optionalNumber(hit.vectorRank);
    const keywordRank = optionalNumber(hit.keywordRank);
    const normalizedVectorScore = optionalNumber(hit.normalizedVectorScore);
    const normalizedKeywordScore = optionalNumber(hit.normalizedKeywordScore);
    return {
      documentId: string(hit.documentId, "documentId"),
      text: string(hit.text, "text"),
      metadata: metadataFromDto(hit.metadata),
      score: number(hit.score, "score"),
      ...(vectorScore === undefined ? {} : { vectorScore }),
      ...(keywordScore === undefined ? {} : { keywordScore }),
      trace: {
        kind: "hybrid",
        alpha,
        ...(vectorRank === undefined ? {} : { vectorRank }),
        ...(keywordRank === undefined ? {} : { keywordRank }),
        ...(normalizedVectorScore === undefined
          ? {}
          : { normalizedVectorScore }),
        ...(normalizedKeywordScore === undefined
          ? {}
          : { normalizedKeywordScore }),
        matchedTerms: stringArray(hit.matchedTerms, "matchedTerms")
      }
    };
  });
}

function graphResultDto(value: unknown): {
  readonly selectionId: number;
  readonly data: GraphSelectionData;
} {
  const result = object(value, "graph result");
  const matches = array(result.matches, "graph matches").map((item) => {
    const match = object(item, "graph match");
    const node = graphNodeFromDto(match);
    const path = array(match.path, "graph path").map((item) => {
      const edge = object(item, "graph path edge");
      const sourceField =
        edge.sourceField === null || edge.sourceField === undefined
          ? undefined
          : stringArray(edge.sourceField, "sourceField");
      return {
        relationship: string(edge.relationship, "relationship"),
        source: graphNodeFromDto(edge.source),
        target: graphNodeFromDto(edge.target),
        occurrenceOrdinal: number(edge.occurrenceOrdinal, "occurrenceOrdinal"),
        provenance: {
          schemaRuleIndex: number(edge.schemaRuleIndex, "schemaRuleIndex"),
          sourceRecordId: string(edge.sourceRecordId, "sourceRecordId"),
          ...(sourceField === undefined ? {} : { sourceField }),
          derivedInverse: boolean(edge.derivedInverse, "derivedInverse"),
          builtIn: boolean(edge.builtIn, "builtIn")
        }
      };
    });
    return {
      node,
      depth: number(match.depth, "depth"),
      path
    };
  });
  const trace = object(result.trace, "graph trace");
  const truncatedValue = result.truncated;
  const truncated = truncationReason(truncatedValue);
  return {
    selectionId: number(result.selectionId, "selectionId"),
    data: {
      matches,
      ...(truncated === undefined ? {} : { truncated }),
      trace: {
        seedCount: number(trace.seedCount, "seedCount"),
        visitedStates: number(trace.visitedStates, "visitedStates"),
        traversedEdges: number(trace.traversedEdges, "traversedEdges"),
        resultCount: number(trace.resultCount, "resultCount"),
        diagnostics: number(trace.diagnostics, "diagnostics")
      }
    }
  };
}

function graphNodeFromDto(value: unknown): GraphNodeId {
  const node = object(value, "graph node");
  const sourceKind = string(node.sourceKind, "sourceKind");
  const nodeType = string(node.nodeType, "nodeType");
  const recordId = string(node.recordId, "recordId");
  if (sourceKind === "chunk") {
    return {
      kind: "chunk",
      nodeType,
      recordId,
      chunkKey: string(node.chunkKey, "chunkKey")
    };
  }
  if (sourceKind !== "record") input(`Invalid graph node source '${sourceKind}'.`);
  return { kind: "record", nodeType, recordId };
}

function truncationReason(value: unknown): GraphTruncationReason | undefined {
  if (value === null || value === undefined) return undefined;
  if (
    value === "maxHops" ||
    value === "maxVisited" ||
    value === "maxResults" ||
    value === "maxWorkingBytes"
  ) {
    return value;
  }
  input("Invalid graph truncation reason.");
}

function candidateProjectionDto(value: unknown): CandidateProjection {
  const projection = object(value, "candidate projection");
  return {
    candidates: array(projection.candidates, "candidates").map((item) => {
      const candidate = object(item, "candidate");
      return {
        recordId: string(candidate.recordId, "recordId"),
        chunkKey: string(candidate.chunkKey, "chunkKey")
      };
    }),
    sourceNodes: number(projection.sourceNodes, "sourceNodes"),
    projectedChunksBeforeFilter: number(
      projection.projectedChunksBeforeFilter,
      "projectedChunksBeforeFilter"
    ),
    projectedChunksAfterFilter: number(
      projection.projectedChunksAfterFilter,
      "projectedChunksAfterFilter"
    )
  };
}

function metadataFromDto(value: unknown): Metadata {
  const metadata: Record<string, MetadataValue> = {};
  for (const item of array(value, "metadata")) {
    const entry = object(item, "metadata entry");
    const encoded = object(entry.value, "metadata value");
    const kind = string(encoded.kind, "metadata kind");
    const raw = encoded.value;
    let decoded: MetadataValue;
    switch (kind) {
      case "string":
        decoded = string(raw, "metadata string");
        break;
      case "boolean":
        if (typeof raw !== "boolean") input("Invalid boolean metadata.");
        decoded = raw;
        break;
      case "integer":
        decoded = BigInt(string(raw, "metadata integer"));
        break;
      case "timestamp":
        decoded = {
          kind: "timestampMillis",
          value: BigInt(string(raw, "metadata timestamp"))
        };
        break;
      case "float":
        decoded = { kind: "float", value: number(raw, "metadata float") };
        break;
      default:
        input(`Unknown metadata kind '${kind}'.`);
    }
    metadata[string(entry.field, "metadata field")] = decoded;
  }
  return metadata;
}

function object(value: unknown, name: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    input(`Invalid ${name}.`);
  }
  return value as Record<string, unknown>;
}

function array(value: unknown, name: string): readonly unknown[] {
  if (!Array.isArray(value)) input(`Invalid ${name}.`);
  return value;
}

function string(value: unknown, name: string): string {
  if (typeof value !== "string") input(`Invalid ${name}.`);
  return value;
}

function number(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    input(`Invalid ${name}.`);
  }
  return value;
}

function boolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") input(`Invalid ${name}.`);
  return value;
}

function optionalNumber(value: unknown): number | undefined {
  return value === null || value === undefined ? undefined : number(value, "number");
}

function stringArray(value: unknown, name: string): readonly string[] {
  return array(value, name).map((item) => string(item, name));
}

function requireNotAborted(signal: AbortSignal): void {
  if (signal.aborted) {
    throw new DOMException("The operation was aborted.", "AbortError");
  }
}

function generatedCall<T>(operation: () => T): T {
  try {
    return operation();
  } catch (error) {
    if (error instanceof RetrievalKitError) throw error;
    const message = error instanceof Error ? error.message : String(error);
    if (message.startsWith("RK_INVALID_BOUNDARY")) {
      throw new RetrievalKitInputError(message, "RK_INVALID_BOUNDARY");
    }
    if (message.startsWith("RK_INVALID_STATE")) {
      throw new RetrievalKitLifecycleError(message, "RK_INVALID_STATE");
    }
    if (message.startsWith("RK_GRAPH")) {
      throw new RetrievalKitGraphError(message, "RK_GRAPH");
    }
    if (message.startsWith("RK_CORE")) {
      if (/\bdimension\b/i.test(message)) {
        throw new RetrievalKitDimensionError(message, "RK_CORE");
      }
      if (/\b(query|search|filter|alpha)\b/i.test(message)) {
        throw new RetrievalKitQueryError(message, "RK_CORE");
      }
      throw new RetrievalKitError(message, "RK_CORE");
    }
    throw error;
  }
}

function input(message: string): never {
  throw new RetrievalKitInputError(message, "RK_INVALID_INPUT");
}

function lifecycle(message: string): never {
  throw new RetrievalKitLifecycleError(message, "RK_LIFECYCLE");
}
