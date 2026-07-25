import {
  binding,
  type NativeGraphHandle,
  type NativeGraphSelection
} from "./binding.js";
import type {
  NativeFilter,
  NativeGraphQuery,
  NativeGraphRecordInput,
  NativeGraphResult,
  NativeGraphScalar,
  NativeHybridHit,
  NativeMetadataEntry,
  NativeMetadataValue,
  NativeNodeId,
  NativeRecordValue,
  NativeSearchHit
} from "./native-types.js";

export type VectorMetric = "cosine" | "dotProduct";
export type VectorEncoding = "f32" | "f16" | "bf16" | "i8";
export interface TimestampMillis {
  readonly kind: "timestampMillis";
  readonly value: bigint;
}
export interface FloatingPoint {
  readonly kind: "float";
  readonly value: number;
}
export type MetadataValue =
  | string
  | boolean
  | bigint
  | number
  | TimestampMillis
  | FloatingPoint;
export type Metadata = Readonly<Record<string, MetadataValue>>;
export interface RecordValueMap {
  readonly [field: string]: RecordValue;
}
export type RecordValue =
  | null
  | string
  | boolean
  | bigint
  | number
  | readonly RecordValue[]
  | RecordValueMap;

export function timestampMillis(value: bigint | number): TimestampMillis {
  return { kind: "timestampMillis", value: safeBigInt(value, "timestamp") };
}
export function floatingPoint(value: number): FloatingPoint {
  if (!Number.isFinite(value)) {
    throw new RetrievalKitInputError(
      "Floating-point metadata must be finite.",
      "RK_INVALID_INPUT"
    );
  }
  return { kind: "float", value };
}

export type Filter =
  | { readonly kind: "equals"; readonly field: string; readonly value: MetadataValue }
  | { readonly kind: "notEquals"; readonly field: string; readonly value: MetadataValue }
  | { readonly kind: "in"; readonly field: string; readonly values: readonly MetadataValue[] }
  | {
      readonly kind: "range";
      readonly field: string;
      readonly lower?: MetadataValue;
      readonly upper?: MetadataValue;
    }
  | { readonly kind: "exists"; readonly field: string }
  | { readonly kind: "all"; readonly filters: readonly Filter[] }
  | { readonly kind: "any"; readonly filters: readonly Filter[] };

export interface RecordNodeSchema {
  readonly recordType: string;
  readonly nodeType: string;
  readonly queryableFields?: readonly (readonly string[])[];
}
export interface RelationshipSchema {
  readonly relationship: string;
  readonly sourceNodeType: string;
  readonly targetNodeType: string;
  readonly sourceField: readonly string[];
  readonly cardinality: "one" | "optionalOne" | "many";
  readonly missingTarget?: "error" | "omitEdge";
  readonly duplicateReferences?: "error" | "deduplicate";
  readonly allowSelfEdge?: boolean;
  readonly inverseRelationship?: string;
}
export interface GraphSchema {
  readonly recordNodes: readonly RecordNodeSchema[];
  readonly relationships?: readonly RelationshipSchema[];
  readonly chunkNodes?: {
    readonly nodeType: string;
    readonly ownsRelationship: string;
    readonly inverseRelationship?: string;
  };
}

export interface GraphRecord {
  readonly id: string;
  readonly type: string;
  readonly fields?: Readonly<Record<string, RecordValue>>;
  readonly content?: string;
  readonly metadata?: Metadata;
}
export interface GraphOnlyRecordInput extends GraphRecord {
  readonly retrieval?: never;
}
export interface SingleEmbeddingRetrieval {
  readonly kind: "content";
  readonly embedding: Float32Array;
}
export interface DocumentRetrieval {
  readonly kind: "documents";
  readonly documents: readonly {
    readonly id: string;
    readonly text: string;
    readonly metadata?: Metadata;
    readonly embedding: Float32Array;
  }[];
}
export interface GraphRetrievalRecordInput extends GraphRecord {
  readonly retrieval?: SingleEmbeddingRetrieval | DocumentRetrieval;
}

export interface GraphBuilderOptions {
  readonly corpusId: string;
  readonly schema: GraphSchema;
}
export interface GraphRetrievalBuilderOptions extends GraphBuilderOptions {
  readonly metric?: VectorMetric;
  readonly encoding?: VectorEncoding;
}

export interface RecordNodeId {
  readonly kind: "record";
  readonly nodeType: string;
  readonly recordId: string;
}
export interface ChunkNodeId {
  readonly kind: "chunk";
  readonly nodeType: string;
  readonly recordId: string;
  readonly chunkKey: string;
}
export type GraphNodeId = RecordNodeId | ChunkNodeId;
export type GraphScalar = string | bigint | boolean;
export type GraphSeed =
  | { readonly kind: "nodes"; readonly nodes: readonly GraphNodeId[] }
  | {
      readonly kind: "equals";
      readonly nodeType: string;
      readonly field: readonly string[];
      readonly values: readonly GraphScalar[];
    };
export interface GraphQuery {
  readonly seed: GraphSeed;
  readonly traverse?: readonly {
    readonly relationship: string;
    readonly direction?: "outgoing" | "incoming";
    readonly minHops?: number;
    readonly maxHops?: number;
  }[];
  readonly limits?: {
    readonly maxHops?: number;
    readonly maxVisited?: number;
    readonly maxResults?: number;
    readonly maxWorkingBytes?: number;
  };
}
export interface GraphEdgeProvenance {
  readonly schemaRuleIndex: number;
  readonly sourceRecordId: string;
  readonly sourceField?: readonly string[];
  readonly derivedInverse: boolean;
  readonly builtIn: boolean;
}
export interface GraphPathEdge {
  readonly relationship: string;
  readonly source: GraphNodeId;
  readonly target: GraphNodeId;
  readonly occurrenceOrdinal: number;
  readonly provenance: GraphEdgeProvenance;
}
export interface GraphMatch {
  readonly node: GraphNodeId;
  readonly depth: number;
  readonly path: readonly GraphPathEdge[];
}
export interface GraphQueryTrace {
  readonly seedCount: number;
  readonly visitedStates: number;
  readonly traversedEdges: number;
  readonly resultCount: number;
  readonly diagnostics: number;
}
export type GraphTruncationReason =
  | "maxHops"
  | "maxVisited"
  | "maxResults"
  | "maxWorkingBytes";

export interface CandidateProjection {
  readonly candidates: readonly { readonly recordId: string; readonly chunkKey: string }[];
  readonly sourceNodes: number;
  readonly projectedChunksBeforeFilter: number;
  readonly projectedChunksAfterFilter: number;
}

export interface VectorSearch {
  readonly mode: "vector";
  readonly embedding: Float32Array;
  readonly limit?: number;
  readonly where?: Filter;
  readonly within?: GraphSelection;
}
export interface TextSearch {
  readonly mode: "text";
  readonly text: string;
  readonly limit?: number;
  readonly where?: Filter;
  readonly keywordCandidates?: number;
  readonly within?: GraphSelection;
}
export interface HybridSearch {
  readonly mode: "hybrid";
  readonly text: string;
  readonly embedding?: Float32Array;
  readonly alpha?: number;
  readonly limit?: number;
  readonly where?: Filter;
  readonly vectorCandidates?: number;
  readonly keywordCandidates?: number;
  readonly within?: GraphSelection;
}
export type SearchQuery = VectorSearch | TextSearch | HybridSearch;
export interface SearchResult {
  readonly documentId: string;
  readonly text: string;
  readonly metadata: Readonly<Record<string, MetadataValue>>;
  readonly score: number;
  readonly vectorScore?: number;
  readonly keywordScore?: number;
  readonly trace:
    | { readonly kind: "vector"; readonly vectorScore: number }
    | {
        readonly kind: "hybrid";
        readonly alpha: number;
        readonly vectorRank?: number;
        readonly keywordRank?: number;
        readonly normalizedVectorScore?: number;
        readonly normalizedKeywordScore?: number;
        readonly matchedTerms: readonly string[];
      };
}
export interface GraphFileSizeReport {
  readonly corpusBytes: number;
  readonly schemaBytes: number;
  readonly graphBytes: number;
  readonly totalBytes: number;
}

export class RetrievalKitError extends Error {
  public constructor(
    message: string,
    public readonly code = "RK_NATIVE",
    options?: ErrorOptions
  ) {
    super(message, options);
    this.name = new.target.name;
  }
}
export class RetrievalKitInputError extends RetrievalKitError {}
export class RetrievalKitDimensionError extends RetrievalKitError {}
export class RetrievalKitLifecycleError extends RetrievalKitError {}
export class RetrievalKitPersistenceError extends RetrievalKitError {}
export class RetrievalKitQueryError extends RetrievalKitError {}
export class RetrievalKitGraphError extends RetrievalKitError {}
export class RetrievalKitStaleSelectionError extends RetrievalKitGraphError {}

const selectionNatives = new WeakMap<GraphSelection, NativeGraphSelection>();
interface GraphSelectionData {
  readonly matches: readonly GraphMatch[];
  readonly truncated?: GraphTruncationReason;
  readonly trace: GraphQueryTrace;
}
const selectionData = new WeakMap<GraphSelection, GraphSelectionData>();

export abstract class GraphSelection {
  #closing?: Promise<void>;

  protected constructor() {}
  public get matches(): readonly GraphMatch[] {
    return dataForSelection(this).matches;
  }
  public get truncated(): GraphTruncationReason | undefined {
    return dataForSelection(this).truncated;
  }
  public get trace(): GraphQueryTrace {
    return dataForSelection(this).trace;
  }
  public get closed(): boolean {
    return nativeForSelection(this).closed;
  }
  public close(): Promise<void> {
    this.#closing ??= nativeCall(() => nativeForSelection(this).close());
    return this.#closing;
  }
  public [Symbol.dispose](): void {
    void this.close();
  }
  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

class GraphSelectionImpl extends GraphSelection {
  public constructor(native: NativeGraphSelection, result: NativeGraphResult) {
    super();
    selectionNatives.set(this, native);
    selectionData.set(this, {
      matches: result.matches.map((match) => ({
        node: fromNativeNode(match.node),
        depth: match.depth,
        path: match.path.map((edge) => ({
          relationship: edge.relationship,
          source: fromNativeNode(edge.source),
          target: fromNativeNode(edge.target),
          occurrenceOrdinal: edge.occurrenceOrdinal,
          provenance: edge.provenance
        }))
      })),
      ...(result.truncated === undefined ? {} : { truncated: result.truncated }),
      trace: result.trace
    });
  }
}

function createGraphSelection(
  native: NativeGraphSelection,
  result: NativeGraphResult
): GraphSelection {
  return new GraphSelectionImpl(native, result);
}

function nativeForSelection(selection: GraphSelection): NativeGraphSelection {
  const native = selectionNatives.get(selection);
  if (native === undefined) {
    throw new RetrievalKitLifecycleError(
      "GraphSelection native ownership is unavailable; obtain selections from graph.query().",
      "RK_LIFECYCLE"
    );
  }
  return native;
}

function dataForSelection(selection: GraphSelection): GraphSelectionData {
  const data = selectionData.get(selection);
  if (data === undefined) {
    throw new RetrievalKitLifecycleError(
      "GraphSelection result ownership is unavailable; obtain selections from graph.query().",
      "RK_LIFECYCLE"
    );
  }
  return data;
}

export class GraphDatabaseBuilder {
  readonly #native: NativeGraphHandle;
  #consumed = false;
  #transferred = false;
  #closing?: Promise<void>;
  public constructor(options: GraphBuilderOptions) {
    this.#native = new binding.NativeGraphHandle(
      "graph",
      options.corpusId,
      toNativeSchema(options.schema)
    );
  }
  public async add(records: Iterable<GraphOnlyRecordInput>): Promise<void> {
    this.#requireActive();
    const values = [...records].map(toNativeGraphRecord);
    if (values.length > 0) await nativeCall(() => this.#native.addRecords(values));
  }
  public async build(): Promise<GraphDatabase> {
    this.#requireActive();
    try {
      await nativeCall(() => this.#native.build());
      this.#consumed = true;
      this.#transferred = true;
      return createGraphDatabase(this.#native);
    } catch (error) {
      this.#consumed = true;
      throw error;
    }
  }
  public close(): Promise<void> {
    if (this.#transferred) return Promise.resolve();
    this.#consumed = true;
    this.#closing ??= nativeCall(() => this.#native.close());
    return this.#closing;
  }
  public [Symbol.dispose](): void {
    void this.close();
  }
  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
  #requireActive(): void {
    if (this.#consumed) {
      throw new RetrievalKitLifecycleError(
        "GraphDatabaseBuilder has already been consumed; create a new builder.",
        "RK_LIFECYCLE"
      );
    }
  }
}

export class GraphRetrievalDatabaseBuilder {
  readonly #native: NativeGraphHandle;
  #consumed = false;
  #transferred = false;
  #closing?: Promise<void>;
  public constructor(options: GraphRetrievalBuilderOptions) {
    this.#native = new binding.NativeGraphHandle(
      "combined",
      options.corpusId,
      toNativeSchema(options.schema),
      options.metric ?? "cosine",
      options.encoding ?? "i8"
    );
  }
  public async add(records: Iterable<GraphRetrievalRecordInput>): Promise<void> {
    this.#requireActive();
    const values = [...records].map(toNativeGraphRecord);
    if (values.length > 0) await nativeCall(() => this.#native.addRecords(values));
  }
  public async build(): Promise<GraphRetrievalDatabase> {
    this.#requireActive();
    try {
      await nativeCall(() => this.#native.build());
      this.#consumed = true;
      this.#transferred = true;
      return createGraphRetrievalDatabase(this.#native);
    } catch (error) {
      this.#consumed = true;
      throw error;
    }
  }
  public close(): Promise<void> {
    if (this.#transferred) return Promise.resolve();
    this.#consumed = true;
    this.#closing ??= nativeCall(() => this.#native.close());
    return this.#closing;
  }
  public [Symbol.dispose](): void {
    void this.close();
  }
  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
  #requireActive(): void {
    if (this.#consumed) {
      throw new RetrievalKitLifecycleError(
        "GraphRetrievalDatabaseBuilder has already been consumed; create a new builder.",
        "RK_LIFECYCLE"
      );
    }
  }
}

export interface GraphQueryOperations {
  query(query: GraphQuery): Promise<GraphSelection>;
  projectCandidates(
    selection: GraphSelection,
    options?: { readonly where?: Filter }
  ): Promise<CandidateProjection>;
}

export interface RetrievalOperations {
  search(query: SearchQuery): Promise<SearchResult[]>;
}

class GraphQueryView implements GraphQueryOperations {
  public constructor(private readonly native: NativeGraphHandle) {}
  public async query(query: GraphQuery): Promise<GraphSelection> {
    const selection = new binding.NativeGraphSelection();
    try {
      const result = await nativeCall(() =>
        this.native.query(toNativeGraphQuery(query), selection)
      );
      return createGraphSelection(selection, result);
    } catch (error) {
      await selection.close();
      throw error;
    }
  }
  public async projectCandidates(
    selection: GraphSelection,
    options: { readonly where?: Filter } = {}
  ): Promise<CandidateProjection> {
    return nativeCall(() =>
      this.native.projectCandidates(
        nativeForSelection(selection),
        options.where === undefined ? undefined : toNativeFilter(options.where)
      )
    );
  }
}

class RetrievalQueryView implements RetrievalOperations {
  public constructor(private readonly native: NativeGraphHandle) {}
  public async search(query: SearchQuery): Promise<SearchResult[]> {
    const limit = positiveInteger(query.limit ?? 10, "limit");
    const filter = query.where === undefined ? undefined : toNativeFilter(query.where);
    const selection =
      query.within === undefined ? undefined : nativeForSelection(query.within);
    switch (query.mode) {
      case "vector":
        return (
          await nativeCall(() =>
            this.native.semanticSearch(query.embedding, limit, filter, selection)
          )
        ).map(fromNativeVectorHit);
      case "text":
        return (
          await nativeCall(() =>
            this.native.hybridSearch(
              query.text,
              undefined,
              limit,
              filter,
              0,
              0,
              optionalPositiveInteger(query.keywordCandidates, "keywordCandidates"),
              selection
            )
          )
        ).map(fromNativeHybridHit);
      case "hybrid": {
        const alpha = query.alpha ?? 0.6;
        return (
          await nativeCall(() =>
            this.native.hybridSearch(
              query.text,
              query.embedding,
              limit,
              filter,
              alpha,
              optionalPositiveInteger(query.vectorCandidates, "vectorCandidates"),
              optionalPositiveInteger(query.keywordCandidates, "keywordCandidates"),
              selection
            )
          )
        ).map(fromNativeHybridHit);
      }
    }
  }
}

const graphNatives = new WeakMap<GraphDatabaseLifecycle, NativeGraphHandle>();

abstract class GraphDatabaseLifecycle {
  #closing?: Promise<void>;
  protected constructor() {}
  public get closed(): boolean {
    return nativeForGraph(this).closed;
  }
  public async save(path: string): Promise<GraphFileSizeReport> {
    return nativeCall(() => nativeForGraph(this).save(path));
  }
  public close(): Promise<void> {
    this.#closing ??= nativeCall(() => nativeForGraph(this).close());
    return this.#closing;
  }
  public [Symbol.dispose](): void {
    void this.close();
  }
  public async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}

export abstract class GraphDatabase extends GraphDatabaseLifecycle {
  public readonly graph: GraphQueryOperations;
  protected constructor(graph: GraphQueryOperations) {
    super();
    this.graph = graph;
  }
  public static async load(path: string): Promise<GraphDatabase> {
    const native = binding.NativeGraphHandle.empty();
    await nativeCall(() => native.load("graph", path));
    return createGraphDatabase(native);
  }
  public static async validate(path: string): Promise<void> {
    await nativeCall(() => binding.validateGraph("graph", path));
  }
}

export abstract class GraphRetrievalDatabase extends GraphDatabaseLifecycle {
  public readonly graph: GraphQueryOperations;
  public readonly retrieval: RetrievalOperations;
  protected constructor(graph: GraphQueryOperations, retrieval: RetrievalOperations) {
    super();
    this.graph = graph;
    this.retrieval = retrieval;
  }
  public static async load(path: string): Promise<GraphRetrievalDatabase> {
    const native = binding.NativeGraphHandle.empty();
    await nativeCall(() => native.load("combined", path));
    return createGraphRetrievalDatabase(native);
  }
  public static async validate(path: string): Promise<void> {
    await nativeCall(() => binding.validateGraph("combined", path));
  }
}

class GraphDatabaseImpl extends GraphDatabase {
  public constructor(native: NativeGraphHandle) {
    super(new GraphQueryView(native));
    graphNatives.set(this, native);
  }
}

class GraphRetrievalDatabaseImpl extends GraphRetrievalDatabase {
  public constructor(native: NativeGraphHandle) {
    super(new GraphQueryView(native), new RetrievalQueryView(native));
    graphNatives.set(this, native);
  }
}

function createGraphDatabase(native: NativeGraphHandle): GraphDatabase {
  return new GraphDatabaseImpl(native);
}

function createGraphRetrievalDatabase(native: NativeGraphHandle): GraphRetrievalDatabase {
  return new GraphRetrievalDatabaseImpl(native);
}

function nativeForGraph(database: GraphDatabaseLifecycle): NativeGraphHandle {
  const native = graphNatives.get(database);
  if (native === undefined) {
    throw new RetrievalKitLifecycleError(
      "Graph database native ownership is unavailable; create it with a builder or load().",
      "RK_LIFECYCLE"
    );
  }
  return native;
}

function toNativeSchema(schema: GraphSchema) {
  return {
    recordNodes: schema.recordNodes.map((mapping) => ({
      recordType: mapping.recordType,
      nodeType: mapping.nodeType,
      queryableFields: mapping.queryableFields?.map((path) => [...path]) ?? []
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

function toNativeGraphRecord(record: GraphRetrievalRecordInput): NativeGraphRecordInput {
  const base = {
    id: record.id,
    recordType: record.type,
    fields: Object.entries(record.fields ?? {}).map(([field, value]) => ({
      field,
      value: toNativeRecordValue(value)
    })),
    ...(record.content === undefined ? {} : { content: record.content }),
    metadata: toNativeMetadata(record.metadata),
    documents: []
  };
  if (record.retrieval?.kind === "content") {
    return { ...base, embedding: record.retrieval.embedding };
  }
  if (record.retrieval?.kind === "documents") {
    return {
      ...base,
      documents: record.retrieval.documents.map((document) => ({
        id: document.id,
        text: document.text,
        metadata: toNativeMetadata(document.metadata),
        embedding: document.embedding
      }))
    };
  }
  return base;
}

function toNativeRecordValue(value: RecordValue): NativeRecordValue {
  if (value === null) return { kind: "null" };
  if (typeof value === "string") return { kind: "string", stringValue: value };
  if (typeof value === "boolean") return { kind: "boolean", booleanValue: value };
  if (typeof value === "bigint") return { kind: "integer", integerValue: value.toString() };
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new RetrievalKitInputError("Record numbers must be finite.", "RK_INVALID_INPUT");
    }
    return Number.isSafeInteger(value)
      ? { kind: "integer", integerValue: BigInt(value).toString() }
      : { kind: "float", numberValue: value };
  }
  if (Array.isArray(value)) {
    return { kind: "list", listValue: value.map(toNativeRecordValue) };
  }
  return {
    kind: "map",
    mapValue: Object.entries(value).map(([field, child]) => ({
      field,
      value: toNativeRecordValue(child)
    }))
  };
}

function toNativeGraphQuery(query: GraphQuery): NativeGraphQuery {
  const seed =
    query.seed.kind === "nodes"
      ? { kind: "nodes", nodes: query.seed.nodes.map(toNativeNode) }
      : {
          kind: "equals",
          nodeType: query.seed.nodeType,
          field: [...query.seed.field],
          values: query.seed.values.map(toNativeScalar)
        };
  const steps =
    query.traverse?.map((step) => ({
      relationship: step.relationship,
      direction: step.direction ?? "outgoing",
      minHops: unsignedInteger(step.minHops ?? 1, "minHops"),
      maxHops: unsignedInteger(step.maxHops ?? 1, "maxHops")
    })) ?? [];
  return {
    seed,
    steps,
    ...(query.limits === undefined
      ? {}
      : {
          limits: {
            maxHops: unsignedInteger(query.limits.maxHops ?? 8, "maxHops"),
            maxVisited: positiveInteger(query.limits.maxVisited ?? 100_000, "maxVisited"),
            maxResults: positiveInteger(query.limits.maxResults ?? 10_000, "maxResults"),
            maxWorkingBytes: positiveInteger(
              query.limits.maxWorkingBytes ?? 64 * 1024 * 1024,
              "maxWorkingBytes"
            )
          }
        })
  };
}

function toNativeNode(node: GraphNodeId): NativeNodeId {
  return {
    nodeType: node.nodeType,
    sourceKind: node.kind,
    recordId: node.recordId,
    ...(node.kind === "chunk" ? { chunkKey: node.chunkKey } : {})
  };
}
function fromNativeNode(node: NativeNodeId): GraphNodeId {
  return node.sourceKind === "chunk"
    ? {
        kind: "chunk",
        nodeType: node.nodeType,
        recordId: node.recordId,
        chunkKey: node.chunkKey ?? ""
      }
    : { kind: "record", nodeType: node.nodeType, recordId: node.recordId };
}
function toNativeScalar(value: GraphScalar): NativeGraphScalar {
  if (typeof value === "string") return { kind: "string", stringValue: value };
  if (typeof value === "boolean") return { kind: "boolean", booleanValue: value };
  return { kind: "integer", integerValue: value.toString() };
}

function toNativeMetadata(metadata: Metadata | undefined): NativeMetadataEntry[] {
  return Object.entries(metadata ?? {}).map(([field, value]) => ({
    field,
    value: toNativeMetadataValue(value)
  }));
}
function toNativeMetadataValue(value: MetadataValue): NativeMetadataValue {
  if (typeof value === "string") return { kind: "string", stringValue: value };
  if (typeof value === "boolean") return { kind: "boolean", booleanValue: value };
  if (typeof value === "bigint") return { kind: "integer", integerValue: value.toString() };
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new RetrievalKitInputError("Metadata numbers must be finite.", "RK_INVALID_INPUT");
    }
    return Number.isSafeInteger(value)
      ? { kind: "integer", integerValue: BigInt(value).toString() }
      : { kind: "float", numberValue: value };
  }
  return value.kind === "timestampMillis"
    ? { kind: "timestamp", integerValue: value.value.toString() }
    : { kind: "float", numberValue: value.value };
}
function fromNativeMetadata(entries: NativeMetadataEntry[]): Record<string, MetadataValue> {
  return Object.fromEntries(
    entries.map(({ field, value }) => {
      const decoded: MetadataValue =
        value.kind === "string"
          ? (value.stringValue ?? "")
          : value.kind === "boolean"
            ? (value.booleanValue ?? false)
            : value.kind === "integer"
              ? BigInt(value.integerValue ?? "0")
              : value.kind === "timestamp"
                ? timestampMillis(BigInt(value.integerValue ?? "0"))
                : floatingPoint(value.numberValue ?? 0);
      return [field, decoded];
    })
  );
}
function toNativeFilter(filter: Filter): NativeFilter {
  if (filter.kind === "equals" || filter.kind === "notEquals") {
    return { kind: filter.kind, field: filter.field, value: toNativeMetadataValue(filter.value) };
  }
  if (filter.kind === "in") {
    return { kind: "in", field: filter.field, values: filter.values.map(toNativeMetadataValue) };
  }
  if (filter.kind === "range") {
    return {
      kind: "range",
      field: filter.field,
      ...(filter.lower === undefined ? {} : { lower: toNativeMetadataValue(filter.lower) }),
      ...(filter.upper === undefined ? {} : { upper: toNativeMetadataValue(filter.upper) })
    };
  }
  if (filter.kind === "exists") return { kind: "exists", field: filter.field };
  return { kind: filter.kind, children: filter.filters.map(toNativeFilter) };
}

function fromNativeVectorHit(hit: NativeSearchHit): SearchResult {
  return {
    documentId: hit.documentId,
    text: hit.text,
    metadata: fromNativeMetadata(hit.metadata),
    score: hit.score,
    vectorScore: hit.vectorScore,
    trace: { kind: "vector", vectorScore: hit.vectorScore }
  };
}
function fromNativeHybridHit(hit: NativeHybridHit): SearchResult {
  return {
    documentId: hit.documentId,
    text: hit.text,
    metadata: fromNativeMetadata(hit.metadata),
    score: hit.score,
    ...(hit.vectorScore === undefined ? {} : { vectorScore: hit.vectorScore }),
    ...(hit.keywordScore === undefined ? {} : { keywordScore: hit.keywordScore }),
    trace: {
      kind: "hybrid",
      alpha: hit.trace.alpha,
      ...(hit.trace.vectorRank === undefined ? {} : { vectorRank: hit.trace.vectorRank }),
      ...(hit.trace.keywordRank === undefined ? {} : { keywordRank: hit.trace.keywordRank }),
      ...(hit.trace.normalizedVectorScore === undefined
        ? {}
        : { normalizedVectorScore: hit.trace.normalizedVectorScore }),
      ...(hit.trace.normalizedKeywordScore === undefined
        ? {}
        : { normalizedKeywordScore: hit.trace.normalizedKeywordScore }),
      matchedTerms: hit.trace.matchedTerms
    }
  };
}
function positiveInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 0xffff_ffff) {
    throw new RetrievalKitInputError(
      `Invalid ${name} ${String(value)}; expected an integer from 1 through 4294967295.`,
      "RK_INVALID_INPUT"
    );
  }
  return value;
}
function unsignedInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new RetrievalKitInputError(
      `Invalid ${name} ${String(value)}; expected an integer from 0 through 4294967295.`,
      "RK_INVALID_INPUT"
    );
  }
  return value;
}
function optionalPositiveInteger(value: number | undefined, name: string): number | undefined {
  return value === undefined ? undefined : positiveInteger(value, name);
}
function safeBigInt(value: bigint | number, name: string): bigint {
  if (typeof value === "bigint") return value;
  if (!Number.isSafeInteger(value)) {
    throw new RetrievalKitInputError(
      `${name} must be a bigint or safe integer; got ${String(value)}.`,
      "RK_INVALID_INPUT"
    );
  }
  return BigInt(value);
}
async function nativeCall<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw normalizeError(error);
  }
}
function normalizeError(error: unknown): RetrievalKitError {
  if (error instanceof RetrievalKitError) return error;
  const cause = error instanceof Error ? error : new Error(String(error));
  const match = /^(RK_[A-Z_]+):\s*(.*)$/s.exec(cause.message);
  const code = match?.[1] ?? "RK_NATIVE";
  const message = match?.[2] ?? cause.message;
  const Constructor =
    code === "RK_DIMENSION"
      ? RetrievalKitDimensionError
      : code === "RK_CLOSED" || code === "RK_LIFECYCLE"
        ? RetrievalKitLifecycleError
        : code === "RK_PERSISTENCE"
          ? RetrievalKitPersistenceError
          : code === "RK_INVALID_QUERY"
            ? RetrievalKitQueryError
            : code === "RK_STALE_SELECTION"
              ? RetrievalKitStaleSelectionError
              : code.startsWith("RK_GRAPH")
                ? RetrievalKitGraphError
                : code === "RK_INVALID_INPUT" || code === "RK_MISSING_EMBEDDING"
                  ? RetrievalKitInputError
                  : RetrievalKitError;
  return new Constructor(message, code, { cause });
}
